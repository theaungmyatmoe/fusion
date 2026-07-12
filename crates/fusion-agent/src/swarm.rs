//! Concurrent sub-agent swarm executor.
//!
//! Replaces the old synchronous `execute_delegate_write` with a full
//! task orchestrator supporting single, parallel, and background modes.
//!
//! Rate-limit strategy when spawning (from reference/opencode, pi, codex,
//! grok-cli):
//! - Semaphore caps how many sub-agents run at once (`swarm_max_concurrency`)
//! - Staggered starts avoid a thundering-herd of simultaneous first LLM calls
//! - LLM client holds a process-wide request gate + Retry-After backoff

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore};
use tokio::task::{JoinHandle, JoinSet};
use tokio_util::sync::CancellationToken;

use fusion_core::config::Config;
use fusion_core::task_session::{TaskSession, TaskStatus};
use fusion_llm::client::{
    adaptive_spawn_stagger, create_subagent_llm_client, ChatMessage, ChatOptions, LlmClient,
};

use crate::agent::AgentEvent;
use crate::persona::{get_persona, Persona};
use crate::tools::{build_tool_schemas_for_persona, ToolRegistry};

/// Default max parallel sub-agents. Kept low so parent + children share the
/// provider budget; the LLM client gate further limits in-flight HTTP calls.
const DEFAULT_SWARM_MAX_CONCURRENCY: usize = 2;
/// Delay between launching each parallel/background sub-agent.
const DEFAULT_SPAWN_STAGGER_MS: u64 = 750;

struct QueuedJob {
    index: usize,
    persona_name: String,
    persona: Persona,
    task_desc: String,
}

/// Result of a completed (or failed/timed-out) sub-agent task.
#[derive(Debug, Clone)]
pub struct TaskResult {
    pub task_id: String,
    pub persona: String,
    pub status: TaskStatus,
    pub summary: String,
    pub workspace_changes: String,
}

/// A handle to a running background task.
struct TaskHandle {
    pub task_id: String,
    pub persona: String,
    pub description: String,
    pub join_handle: JoinHandle<TaskResult>,
    pub cancel_token: CancellationToken,
}

/// The concurrent task swarm orchestrator.
pub struct TaskSwarm {
    active_tasks: Arc<Mutex<HashMap<String, TaskHandle>>>,
    semaphore: Arc<Semaphore>,
    max_concurrency: usize,
    /// Pause between spawning concurrent sub-agents to avoid burst 429s.
    spawn_stagger: Duration,
}

impl TaskSwarm {
    pub fn new(max_concurrency: usize) -> Self {
        Self::with_stagger(
            max_concurrency,
            Duration::from_millis(DEFAULT_SPAWN_STAGGER_MS),
        )
    }

    pub fn with_stagger(max_concurrency: usize, spawn_stagger: Duration) -> Self {
        let max_concurrency = max_concurrency.max(1);
        Self {
            active_tasks: Arc::new(Mutex::new(HashMap::new())),
            semaphore: Arc::new(Semaphore::new(max_concurrency)),
            max_concurrency,
            spawn_stagger,
        }
    }

    pub fn default_max_concurrency() -> usize {
        DEFAULT_SWARM_MAX_CONCURRENCY
    }

    /// Execute a `task` tool call. Dispatches to single, parallel, or background mode.
    pub async fn execute(
        &self,
        args: &serde_json::Value,
        config: &Config,
        cwd: &str,
        tx: tokio::sync::mpsc::UnboundedSender<AgentEvent>,
    ) -> String {
        // Determine execution mode
        let has_tasks_array = args.get("tasks").and_then(|v| v.as_array()).is_some_and(|a| !a.is_empty());
        let background = args.get("background").and_then(|v| v.as_bool()).unwrap_or(false);

        if has_tasks_array {
            self.execute_parallel(args, config, cwd, tx).await
        } else if background {
            self.execute_background(args, config, cwd, tx).await
        } else {
            self.execute_single(args, config, cwd, tx).await
        }
    }

    /// Single mode: spawn one sub-agent and await its result.
    async fn execute_single(
        &self,
        args: &serde_json::Value,
        config: &Config,
        cwd: &str,
        tx: tokio::sync::mpsc::UnboundedSender<AgentEvent>,
    ) -> String {
        let persona_name = args.get("persona").and_then(|v| v.as_str()).unwrap_or("worker");
        let task_desc = args.get("task").and_then(|v| v.as_str()).unwrap_or("");
        let resume_id = args.get("task_id").and_then(|v| v.as_str());

        if task_desc.is_empty() && resume_id.is_none() {
            return "Error: 'task' description is required for single mode.".to_string();
        }

        let persona = match get_persona(persona_name) {
            Some(p) => p,
            None => {
                return format!(
                    "Error: Unknown persona '{}'. Available: scout, worker, reviewer, planner.",
                    persona_name
                );
            }
        };

        // Acquire semaphore permit
        let _permit = self.semaphore.acquire().await.unwrap();

        let result = self
            .run_sub_agent(persona, task_desc, resume_id, config, cwd, tx.clone())
            .await;

        format_task_result(&result)
    }

    /// Parallel mode: true worker-pool queue.
    ///
    /// Jobs sit in a queue and only start when a concurrency slot frees —
    /// no fire-all-at-once thundering herd. Adaptive stagger grows under 429 pressure.
    async fn execute_parallel(
        &self,
        args: &serde_json::Value,
        config: &Config,
        cwd: &str,
        tx: tokio::sync::mpsc::UnboundedSender<AgentEvent>,
    ) -> String {
        let tasks = match args.get("tasks").and_then(|v| v.as_array()) {
            Some(arr) => arr.clone(),
            None => return "Error: 'tasks' array is required for parallel mode.".to_string(),
        };

        if tasks.len() > 8 {
            return format!(
                "Error: Too many parallel tasks ({}). Maximum is 8.",
                tasks.len()
            );
        }

        let mut queue: VecDeque<QueuedJob> = VecDeque::new();
        for (i, task_spec) in tasks.iter().enumerate() {
            let persona_name = task_spec
                .get("persona")
                .and_then(|v| v.as_str())
                .unwrap_or("worker");
            let task_desc = task_spec
                .get("task")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            match get_persona(persona_name) {
                Some(p) => queue.push_back(QueuedJob {
                    index: i,
                    persona_name: persona_name.to_string(),
                    persona: p.clone(),
                    task_desc: task_desc.to_string(),
                }),
                None => {
                    let _ = tx.send(AgentEvent::TextDelta(format!(
                        "\n[Swarm] Task {} skipped: unknown persona '{}'\n",
                        i + 1,
                        persona_name
                    )));
                }
            }
        }

        let total = queue.len();
        let _ = tx.send(AgentEvent::TextDelta(format!(
            "\n[Swarm] Queued {} parallel sub-agents (pool size: {}, base stagger: {}ms)...\n",
            total,
            self.max_concurrency,
            self.spawn_stagger.as_millis()
        )));

        let mut join_set: JoinSet<(usize, String, TaskResult)> = JoinSet::new();
        let mut results: Vec<Option<TaskResult>> = (0..tasks.len()).map(|_| None).collect();
        let mut started = 0usize;

        while !queue.is_empty() || !join_set.is_empty() {
            // Fill free pool slots from the queue (true worker pool).
            while !queue.is_empty() && join_set.len() < self.max_concurrency {
                let permit = match self.try_or_wait_permit(join_set.is_empty()).await {
                    Some(p) => p,
                    None => break, // at capacity; wait for a running job
                };

                // Adaptive stagger: grows when the LLM gate is under rate-limit pressure.
                if started > 0 {
                    let stagger = adaptive_spawn_stagger(self.spawn_stagger);
                    if !stagger.is_zero() {
                        tokio::time::sleep(stagger).await;
                    }
                }

                let job = queue.pop_front().unwrap();
                started += 1;
                let _ = tx.send(AgentEvent::TextDelta(format!(
                    "\n[Swarm] Starting queued task {}/{} ({}): {}...\n",
                    started,
                    total,
                    job.persona_name,
                    truncate_desc(&job.task_desc, 80)
                )));

                let config = config.clone();
                let cwd = cwd.to_string();
                let tx_job = tx.clone();
                join_set.spawn(async move {
                    let _permit = permit;
                    let result = run_sub_agent_standalone(
                        &job.persona,
                        &job.task_desc,
                        None,
                        &config,
                        &cwd,
                        tx_job,
                    )
                    .await;
                    (job.index, job.persona_name, result)
                });
            }

            // Wait for at least one job to finish before scheduling more.
            match join_set.join_next().await {
                Some(Ok((idx, _persona, result))) => {
                    if idx < results.len() {
                        results[idx] = Some(result);
                    }
                }
                Some(Err(e)) => {
                    let _ = tx.send(AgentEvent::TextDelta(format!(
                        "\n[Swarm] Sub-agent join error: {}\n",
                        e
                    )));
                }
                None => break,
            }
        }

        let ordered: Vec<TaskResult> = results.into_iter().flatten().collect();
        let completed = ordered
            .iter()
            .filter(|r| r.status == TaskStatus::Completed)
            .count();
        let mut output = format!(
            "Parallel execution complete: {}/{} succeeded.\n\n",
            completed,
            ordered.len()
        );
        for (i, r) in ordered.iter().enumerate() {
            output.push_str(&format!(
                "--- Task {} ({}) [{}] ---\ntask_id: {}\n{}\n\n",
                i + 1,
                r.persona,
                r.status,
                r.task_id,
                r.summary,
            ));
        }
        output
    }

    /// Acquire a swarm permit. If jobs are already running, use try_acquire so
    /// we can wait for a completion instead of blocking the scheduler loop.
    async fn try_or_wait_permit(&self, must_wait: bool) -> Option<OwnedSemaphorePermit> {
        if must_wait {
            return self.semaphore.clone().acquire_owned().await.ok();
        }
        self.semaphore.clone().try_acquire_owned().ok()
    }

    /// Background mode: spawn a sub-agent and return immediately with task_id.
    async fn execute_background(
        &self,
        args: &serde_json::Value,
        config: &Config,
        cwd: &str,
        tx: tokio::sync::mpsc::UnboundedSender<AgentEvent>,
    ) -> String {
        let persona_name = args.get("persona").and_then(|v| v.as_str()).unwrap_or("worker");
        let task_desc = args.get("task").and_then(|v| v.as_str()).unwrap_or("");

        if task_desc.is_empty() {
            return "Error: 'task' description is required.".to_string();
        }

        let persona = match get_persona(persona_name) {
            Some(p) => p,
            None => {
                return format!(
                    "Error: Unknown persona '{}'. Available: scout, worker, reviewer, planner.",
                    persona_name
                );
            }
        };

        // Create the task session now to get the ID
        let model_id = resolve_model_id(persona, config);
        let task_session =
            TaskSession::new(persona.name, task_desc, &model_id, cwd, None);
        let task_id = task_session.task_id.clone();
        let _ = task_session.save();

        let _ = tx.send(AgentEvent::TaskSpawned {
            task_id: task_id.clone(),
            persona: persona_name.to_string(),
            description: task_desc.to_string(),
        });

        // Spawn in background
        let config = config.clone();
        let cwd = cwd.to_string();
        let tx_bg = tx.clone();
        let persona = persona.clone();
        let task_desc_bg = task_desc.to_string();
        let task_id_clone = task_id.clone();
        let semaphore = self.semaphore.clone();
        let cancel_token = CancellationToken::new();
        let cancel_clone = cancel_token.clone();

        let join_handle = tokio::spawn(async move {
            let _permit = semaphore.acquire().await.unwrap();

            let result = tokio::select! {
                r = run_sub_agent_standalone(&persona, &task_desc_bg, None, &config, &cwd, tx_bg.clone()) => r,
                _ = cancel_clone.cancelled() => {
                    TaskResult {
                        task_id: task_id_clone.clone(),
                        persona: persona.name.to_string(),
                        status: TaskStatus::Failed("Cancelled".to_string()),
                        summary: "Task was cancelled by parent.".to_string(),
                        workspace_changes: String::new(),
                    }
                }
            };

            // Notify parent
            let _ = tx_bg.send(AgentEvent::TaskCompleted {
                task_id: result.task_id.clone(),
                summary: result.summary.clone(),
            });

            result
        });

        // Track the handle
        {
            let mut tasks = self.active_tasks.lock().await;
            tasks.insert(
                task_id.clone(),
                TaskHandle {
                    task_id: task_id.clone(),
                    persona: persona_name.to_string(),
                    description: task_desc.to_string(),
                    join_handle,
                    cancel_token,
                },
            );
        }

        format!(
            "Background task started.\n\
             task_id: {}\n\
             persona: {}\n\
             description: {}\n\n\
             The task is working in the background. You will be notified when it finishes.\n\
             DO NOT poll for progress or duplicate this task's work.\n\
             Work on non-overlapping tasks, or briefly tell the user what you launched and end your response.",
            task_id, persona_name, task_desc
        )
    }

    /// Run a sub-agent (used by single mode, which needs `&self` for the semaphore).
    async fn run_sub_agent(
        &self,
        persona: &Persona,
        task_desc: &str,
        resume_id: Option<&str>,
        config: &Config,
        cwd: &str,
        tx: tokio::sync::mpsc::UnboundedSender<AgentEvent>,
    ) -> TaskResult {
        run_sub_agent_standalone(persona, task_desc, resume_id, config, cwd, tx).await
    }

    /// Check the status of a background task by ID.
    pub async fn get_task_status(&self, task_id: &str) -> Option<String> {
        let tasks = self.active_tasks.lock().await;
        if let Some(handle) = tasks.get(task_id) {
            if handle.join_handle.is_finished() {
                Some(format!("{} ({}): finished", handle.task_id, handle.persona))
            } else {
                Some(format!(
                    "{} ({}): running — {}",
                    handle.task_id, handle.persona, handle.description
                ))
            }
        } else {
            // Check persisted sessions
            TaskSession::load(task_id)
                .ok()
                .map(|ts| format!("{} ({}): {}", ts.task_id, ts.persona, ts.status))
        }
    }

    /// Cancel a background task.
    pub async fn cancel_task(&self, task_id: &str) -> String {
        let mut tasks = self.active_tasks.lock().await;
        if let Some(handle) = tasks.remove(task_id) {
            handle.cancel_token.cancel();
            format!("Task {} cancelled.", task_id)
        } else {
            format!("Task {} not found or already completed.", task_id)
        }
    }

    /// List all active background tasks.
    pub async fn list_active_tasks(&self) -> Vec<(String, String, String)> {
        let tasks = self.active_tasks.lock().await;
        tasks
            .values()
            .map(|h| {
                let status = if h.join_handle.is_finished() {
                    "finished"
                } else {
                    "running"
                };
                (
                    h.task_id.clone(),
                    h.persona.clone(),
                    status.to_string(),
                )
            })
            .collect()
    }

    /// Drain completed background tasks and return their results.
    /// Called by the agent loop to inject results into the conversation.
    pub async fn drain_completed(&self) -> Vec<TaskResult> {
        let mut tasks = self.active_tasks.lock().await;
        let finished_ids: Vec<String> = tasks
            .iter()
            .filter(|(_, h)| h.join_handle.is_finished())
            .map(|(id, _)| id.clone())
            .collect();

        let mut results = Vec::new();
        for id in finished_ids {
            if let Some(handle) = tasks.remove(&id) {
                if let Ok(result) = handle.join_handle.await {
                    results.push(result);
                }
            }
        }
        results
    }
}

// ── Standalone sub-agent runner ──────────────────────────────────────────────

/// Run a sub-agent to completion. This is the core execution function shared
/// by all modes (single, parallel, background).
pub async fn run_sub_agent_standalone(
    persona: &Persona,
    task_desc: &str,
    resume_id: Option<&str>,
    config: &Config,
    cwd: &str,
    tx: tokio::sync::mpsc::UnboundedSender<AgentEvent>,
) -> TaskResult {
    // 1. Resolve model
    let model_id = resolve_model_id(persona, config);
    let mut sub_config = config.clone();
    sub_config.model = model_id.clone();

    // 2. Create or resume task session
    let mut task_session = if let Some(rid) = resume_id {
        match TaskSession::load(rid) {
            Ok(ts) => {
                let _ = tx.send(AgentEvent::TextDelta(format!(
                    "\n[Swarm] Resuming task {} ({})...\n",
                    ts.short_id(),
                    ts.persona
                )));
                ts
            }
            Err(_) => {
                return TaskResult {
                    task_id: rid.to_string(),
                    persona: persona.name.to_string(),
                    status: TaskStatus::Failed(format!("Task session '{}' not found", rid)),
                    summary: String::new(),
                    workspace_changes: String::new(),
                };
            }
        }
    } else {
        TaskSession::new(persona.name, task_desc, &model_id, cwd, None)
    };

    let task_id = task_session.task_id.clone();

    let _ = tx.send(AgentEvent::TaskSpawned {
        task_id: task_id.clone(),
        persona: persona.name.to_string(),
        description: task_desc.to_string(),
    });

    // 3. Build messages — system prompt + resumed context + new instruction
    let mut messages: Vec<ChatMessage> = Vec::new();

    let mut sys_prompt = format!(
        "{}\n\nWORKSPACE: {}\nMODEL: {}",
        persona.system_prompt, cwd, model_id
    );

    // Load any specialized local or global skills
    let skills = fusion_core::config::load_skills(cwd);
    if !skills.is_empty() {
        sys_prompt.push_str("\n\nAVAILABLE SPECIALIZED SKILLS AND BEST PRACTICES:\n");
        for (name, content) in skills {
            sys_prompt.push_str(&format!("--- SKILL: {} ---\n{}\n\n", name, content));
        }
    }

    // Load user taste preferences (personal coding styles)
    let taste_rules =
        fusion_core::taste::load_taste_rules(std::path::Path::new(cwd));
    sys_prompt.push_str("\n\nUSER CODING STYLE PREFERENCES (TASTE PROFILE):\n");
    if !taste_rules.is_empty() {
        sys_prompt.push_str(
            "Align all your code generations and edits to match these choices:\n",
        );
        for rule in taste_rules {
            sys_prompt.push_str(&format!(
                "- {} (Confidence: {:.2})\n",
                rule.rule, rule.confidence
            ));
        }
    } else {
        sys_prompt.push_str("Align your code to these default engineering taste guidelines:\n\
                         - Prefer clean, self-documenting code with minimal, high-value comments.\n\
                         - Write small, modular functions and components with a single clear responsibility.\n\
                         - Use highly descriptive and clear naming for variables, functions, and files.\n\
                         - Write robust error handling; avoid unwrap() or panics in production code.\n");
    }

    messages.push(ChatMessage::system(sys_prompt));

    // If resuming, inject prior conversation
    if resume_id.is_some() {
        for msg in &task_session.messages {
            messages.push(ChatMessage {
                role: msg.role.clone(),
                content: msg.content.clone(),
                name: msg.name.clone(),
                tool_call_id: msg.tool_call_id.clone(),
                tool_calls: None,
            });
        }
    }

    messages.push(ChatMessage::user(format!(
        "Task: {}",
        task_desc
    )));

    // 4. Build tool schemas for this persona
    let tool_schemas = build_tool_schemas_for_persona(persona.allowed_tools);

    // 5. Create LLM client (sub-agent priority leaves a slot for the parent)
    //    and tool registry.
    let llm = create_subagent_llm_client(&sub_config);
    let keenable_api_key = config
        .settings
        .get("keenable")
        .and_then(|v| v.get("api_key"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| std::env::var("KEENABLE_API_KEY").ok());
    let tool_registry = ToolRegistry::new(cwd, Some(cwd.to_string()), keenable_api_key);

    // 6. Agent loop
    let max_rounds = config
        .settings
        .get("subagent_max_rounds")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(12)
        .max(1) as usize;

    let max_tokens = Some(
        config
            .settings
            .get("subagent_max_tokens")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(16384)
            .clamp(1024, u32::MAX as u64) as u32,
    );

    let timeout_secs = config
        .settings
        .get("subagent_timeout_secs")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(900)
        .max(1);

    // Provider-aware pacing; settings override. Pressure multiplies later.
    let defaults = fusion_llm::client::provider_rate_defaults(&config.provider);
    let pacing_ms = config
        .settings
        .get("subagent_pacing_ms")
        .and_then(serde_json::Value::as_u64)
        .or_else(|| {
            config
                .settings
                .get("agent_pacing_ms")
                .and_then(serde_json::Value::as_u64)
        })
        .unwrap_or(defaults.subagent_pacing_ms);

    let sub_result = tokio::time::timeout(
        std::time::Duration::from_secs(timeout_secs),
        run_agent_loop(
            &llm,
            &tool_registry,
            &mut messages,
            &tool_schemas,
            max_rounds,
            max_tokens,
            pacing_ms,
            &task_id,
            &tx,
        ),
    )
    .await;

    // 7. Build result
    let (status, summary) = match sub_result {
        Ok(Ok(final_text)) => (TaskStatus::Completed, final_text),
        Ok(Err(e)) => (
            TaskStatus::Failed(e.to_string()),
            format!("Sub-agent error: {}", e),
        ),
        Err(_) => (
            TaskStatus::TimedOut,
            format!(
                "Sub-agent exceeded its {} second deadline; partial edits were preserved.",
                timeout_secs
            ),
        ),
    };

    // 8. Get workspace changes
    let changes_args = serde_json::json!({
        "command": "git status --short && git diff --stat",
        "timeout_secs": 10
    });
    let workspace_changes = execute_tool_for_swarm(&tool_registry, "run_command", &changes_args)
        .await
        .unwrap_or_else(|e| format!("Failed to inspect changes: {}", e));

    // 9. Persist session
    for msg in &messages {
        if msg.role != "system" {
            task_session.push_message(&msg.role, &msg.content);
        }
    }
    match &status {
        TaskStatus::Completed => task_session.complete(summary.clone()),
        TaskStatus::Failed(reason) => task_session.fail(reason.clone()),
        TaskStatus::TimedOut => task_session.timeout(),
        TaskStatus::Running => {} // shouldn't happen
    }
    let _ = task_session.save();

    let _ = tx.send(AgentEvent::TaskCompleted {
        task_id: task_id.clone(),
        summary: summary.clone(),
    });

    TaskResult {
        task_id,
        persona: persona.name.to_string(),
        status,
        summary,
        workspace_changes,
    }
}

/// Inner agent loop for a sub-agent (similar to Agent::process but standalone).
async fn run_agent_loop(
    llm: &LlmClient,
    tool_registry: &ToolRegistry,
    messages: &mut Vec<ChatMessage>,
    tool_schemas: &[serde_json::Value],
    max_rounds: usize,
    max_tokens: Option<u32>,
    pacing_ms: u64,
    task_id: &str,
    tx: &tokio::sync::mpsc::UnboundedSender<AgentEvent>,
) -> Result<String, String> {
    for round in 0..max_rounds {
        // Pace between rounds; stretch under rate-limit pressure.
        if round > 0 && pacing_ms > 0 {
            let pressure = fusion_llm::client::llm_rate_limit_pressure();
            let effective = match pressure {
                0 => pacing_ms,
                1 => pacing_ms.saturating_mul(2),
                _ => pacing_ms.saturating_mul(4),
            };
            tokio::time::sleep(Duration::from_millis(effective)).await;
        }

        // Nudge near end
        if round == max_rounds.saturating_sub(2) {
            messages.push(ChatMessage::system(
                "IMPORTANT: You are running low on remaining rounds. \
                 Finish up and provide a final summary. Do NOT call more tools unless absolutely necessary.",
            ));
        }

        let effective_max = if round >= max_rounds.saturating_sub(3) {
            Some(max_tokens.unwrap_or(16384).max(16384))
        } else {
            max_tokens
        };

        let options = ChatOptions {
            messages: messages.clone(),
            tools: Some(tool_schemas.to_vec()),
            temperature: Some(0.4),
            max_tokens: effective_max,
        };

        // Stream events to parent
        let (llm_tx, mut llm_rx) = tokio::sync::mpsc::unbounded_channel();
        let tx_fwd = tx.clone();
        let tid = task_id.to_string();
        let forwarder = tokio::spawn(async move {
            while let Some(event) = llm_rx.recv().await {
                match event {
                    fusion_llm::client::LlmEvent::Thinking(chunk) => {
                        let _ = tx_fwd.send(AgentEvent::TaskProgress {
                            task_id: tid.clone(),
                            event: Box::new(AgentEvent::Thinking(chunk)),
                        });
                    }
                    fusion_llm::client::LlmEvent::TextDelta(chunk) => {
                        let _ = tx_fwd.send(AgentEvent::TaskProgress {
                            task_id: tid.clone(),
                            event: Box::new(AgentEvent::TextDelta(chunk)),
                        });
                    }
                    fusion_llm::client::LlmEvent::Retrying {
                        attempt,
                        max_attempts,
                        delay_ms,
                        reason,
                    } => {
                        // Surface at top level so the UI always sees retries.
                        let _ = tx_fwd.send(AgentEvent::Retrying {
                            attempt,
                            max_attempts,
                            delay_ms,
                            reason: format!("sub-agent {}: {}", &tid[..8.min(tid.len())], reason),
                        });
                    }
                }
            }
        });

        let result = match llm.chat(options, Some(llm_tx)).await {
            Ok(res) => res,
            Err(e) => {
                let _ = forwarder.await;
                return Err(format!("LLM error: {}", e));
            }
        };
        let _ = forwarder.await;

        // Handle tool calls
        if !result.tool_calls.is_empty() {
            messages.push(ChatMessage::assistant_with_tools(
                result.content.clone(),
                result.tool_calls.clone(),
            ));

            for tc in &result.tool_calls {
                let args_str = serde_json::to_string(&tc.arguments).unwrap_or_default();
                let preview = if args_str.chars().count() > 200 {
                    format!("{}...", args_str.chars().take(200).collect::<String>())
                } else {
                    args_str
                };

                let _ = tx.send(AgentEvent::TaskProgress {
                    task_id: task_id.to_string(),
                    event: Box::new(AgentEvent::ToolCall {
                        name: format!("task:{}:{}", &task_id[..8.min(task_id.len())], tc.name),
                        args_preview: preview,
                    }),
                });

                let output = execute_tool_for_swarm(tool_registry, &tc.name, &tc.arguments)
                    .await
                    .unwrap_or_else(|e| format!("Tool error: {}", e));

                let _ = tx.send(AgentEvent::TaskProgress {
                    task_id: task_id.to_string(),
                    event: Box::new(AgentEvent::ToolResult {
                        name: format!("task:{}:{}", &task_id[..8.min(task_id.len())], tc.name),
                        output: output.clone(),
                    }),
                });

                messages.push(ChatMessage {
                    role: "tool".to_string(),
                    content: format!("Tool {} result:\n{}", tc.name, output),
                    name: Some(tc.name.clone()),
                    tool_call_id: Some(tc.id.clone()),
                    tool_calls: None,
                });
            }
            continue;
        }

        // Final response
        let final_text = if result.content.is_empty() {
            "(no response)".to_string()
        } else {
            result.content.clone()
        };
        messages.push(ChatMessage::assistant(&final_text));
        return Ok(final_text);
    }

    // Exhausted rounds — force text summary
    messages.push(ChatMessage::system(
        "You have exhausted all tool rounds. Provide a final text summary now.",
    ));

    let final_options = ChatOptions {
        messages: messages.clone(),
        tools: None,
        temperature: Some(0.4),
        max_tokens: Some(16384),
    };

    match llm.chat(final_options, None).await {
        Ok(result) => {
            let text = if result.content.is_empty() {
                "Sub-agent completed all rounds.".to_string()
            } else {
                result.content
            };
            messages.push(ChatMessage::assistant(&text));
            Ok(text)
        }
        Err(e) => Err(format!("Final summary error: {}", e)),
    }
}

/// Execute a tool in the swarm context (no streaming to TUI).
async fn execute_tool_for_swarm(
    registry: &ToolRegistry,
    name: &str,
    args: &serde_json::Value,
) -> Result<String, String> {
    registry.execute_streaming(name, args, None).await
}

/// Resolve which model ID to use for a persona.
fn resolve_model_id(persona: &Persona, config: &Config) -> String {
    if persona.use_small_model {
        if let Some(ref small) = config.small_model {
            if !small.is_empty() {
                return small.clone();
            }
        }
    }
    config.model.clone()
}

/// Format a TaskResult into a string for the parent agent.
fn format_task_result(result: &TaskResult) -> String {
    format!(
        "Sub-agent result\n\
         task_id: {}\n\
         persona: {}\n\
         status: {}\n\
         summary:\n{}\n\n\
         workspace changes:\n{}",
        result.task_id, result.persona, result.status, result.summary, result.workspace_changes
    )
}

fn truncate_desc(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max_chars).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_model_id_uses_small() {
        let config = Config {
            provider: fusion_core::config::Provider::Cloudflare,
            model: "big-model".to_string(),
            small_model: Some("small-model".to_string()),
            api_key: String::new(),
            base_url: String::new(),
            cloudflare_account_id: None,
            yolo: false,
            config_path: None,
            settings: Default::default(),
        };
        let persona = get_persona("worker").unwrap();
        assert_eq!(resolve_model_id(persona, &config), "small-model");
    }

    #[test]
    fn test_resolve_model_id_falls_back_to_main() {
        let config = Config {
            provider: fusion_core::config::Provider::Cloudflare,
            model: "big-model".to_string(),
            small_model: None,
            api_key: String::new(),
            base_url: String::new(),
            cloudflare_account_id: None,
            yolo: false,
            config_path: None,
            settings: Default::default(),
        };
        let persona = get_persona("worker").unwrap();
        assert_eq!(resolve_model_id(persona, &config), "big-model");
    }

    #[test]
    fn test_resolve_model_id_premium_persona_uses_main() {
        let config = Config {
            provider: fusion_core::config::Provider::Cloudflare,
            model: "big-model".to_string(),
            small_model: Some("small-model".to_string()),
            api_key: String::new(),
            base_url: String::new(),
            cloudflare_account_id: None,
            yolo: false,
            config_path: None,
            settings: Default::default(),
        };
        let persona = get_persona("reviewer").unwrap();
        assert_eq!(resolve_model_id(persona, &config), "big-model");
    }

    #[test]
    fn test_format_task_result() {
        let result = TaskResult {
            task_id: "task-abc123".to_string(),
            persona: "worker".to_string(),
            status: TaskStatus::Completed,
            summary: "Fixed the bug.".to_string(),
            workspace_changes: "M src/lib.rs".to_string(),
        };
        let output = format_task_result(&result);
        assert!(output.contains("task-abc123"));
        assert!(output.contains("worker"));
        assert!(output.contains("completed"));
        assert!(output.contains("Fixed the bug."));
    }

    #[test]
    fn test_swarm_creation() {
        let swarm = TaskSwarm::new(4);
        assert_eq!(swarm.max_concurrency, 4);
        assert_eq!(swarm.spawn_stagger, Duration::from_millis(DEFAULT_SPAWN_STAGGER_MS));
    }

    #[test]
    fn test_swarm_with_stagger() {
        let swarm = TaskSwarm::with_stagger(2, Duration::from_millis(500));
        assert_eq!(swarm.max_concurrency, 2);
        assert_eq!(swarm.spawn_stagger, Duration::from_millis(500));
    }

    #[test]
    fn test_default_max_concurrency_is_conservative() {
        assert!(TaskSwarm::default_max_concurrency() <= 2);
    }
}

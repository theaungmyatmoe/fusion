use fusion_core::config::Config;
use fusion_core::error::FusionError;
use fusion_llm::client::{ChatMessage, ChatOptions, create_llm_client, LlmClient};

use crate::tools::{ToolRegistry, build_tool_schemas};

/// Events emitted by the agent for the TUI to render.
#[derive(Debug, Clone)]
pub enum AgentEvent {
    Thinking(String),
    TextDelta(String),
    ToolCall { name: String, args_preview: String },
    ToolResult { name: String, output: String },
    FinalResponse(String),
    TodoUpdate(Vec<TodoItem>),
}

/// A todo item tracked by the agent.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TodoItem {
    pub content: String,
    pub status: String, // "pending", "in_progress", "done"
}

/// The core agent that manages conversation, tool calls, and LLM interactions.
pub struct Agent {
    config: Config,
    llm: LlmClient,
    messages: Vec<ChatMessage>,
    todos: Vec<TodoItem>,
    cwd: String,
    tool_registry: ToolRegistry,
    pub arbitrage_mode: bool,
}

const TERMUX_API_SKILL: &str = include_str!("termux_api_skill.md");

impl Agent {
    pub fn new(config: &Config, cwd: String) -> Self {
        let llm = create_llm_client(config);
        let keenable_api_key = config
            .settings
            .get("keenable")
            .and_then(|v| v.get("api_key"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| std::env::var("KEENABLE_API_KEY").ok());
        let tool_registry = ToolRegistry::new(&cwd, keenable_api_key);

        Self {
            config: config.clone(),
            llm,
            messages: Vec::new(),
            todos: Vec::new(),
            cwd,
            tool_registry,
            arbitrage_mode: false,
        }
    }

    /// Process a user message through the agent loop.
    pub async fn process(
        &mut self,
        user_message: &str,
        tx: tokio::sync::mpsc::UnboundedSender<AgentEvent>,
    ) -> Result<(), FusionError> {
        // Initialize system prompt on first message
        if self.messages.is_empty() {
            let mut sys_prompt = String::from(
                "You are Fusion, a powerful, autonomous coding agent optimized for terminal environments.\n\n\
                 ENVIRONMENT:\n\
                 You are running inside a terminal (CLI). Your text output is rendered in a plain terminal — not a browser, not a rich text editor.\n\
                 - Use plain text only. No markdown tables, no HTML, no images, no colored text.\n\
                 - Use simple markers like dashes (-) or asterisks (*) for lists.\n\
                 - Use indentation and blank lines for structure.\n\
                 - Keep lines under 100 characters when possible.\n\
                 - Use backticks for inline code and triple backticks for code blocks — these are rendered.\n\
                 - Never use unicode box-drawing, fancy borders, or ASCII art in your responses.\n\n\
                 WORKFLOW:\n\
                 1. Understand the request.\n\
                 2. Use tools (read_file, grep, search_replace, shell, get_symbols) to explore the codebase.\n\
                 3. Use search_replace for edits — it requires old_string to match exactly once for safety.\n\
                 4. Keep edits minimal, safe, and focused.\n\
                 5. Run tests or compilation steps via shell to verify correctness.\n\
                 6. Use todo tools to track your progress visibly.\n\n\
                 RUNNING SHELL COMMANDS:\n\
                 - Always run installation and scaffolding commands in NON-INTERACTIVE mode (using -y, --yes, or --non-interactive).\n\
                 - For example, instead of 'npm create vite@latest', run 'npm create vite@latest -y -- --template react'.\n\
                 - If a command waits for stdin/user input, it will HANG and TIMEOUT (e.g. after 120 seconds). Make sure there are no prompts.\n\n\
                 TASTE ENGINEERING (Anti-Slop Frontend Guidelines):\n\
                 When creating web landing pages, portfolios, or frontend interfaces:\n\
                 1. Perform Brief Inference: Output a one-line \"Design Read\" before generating any code, indicating the page kind, target audience, vibe, and design aesthetic.\n\
                 2. Set Dials: Set DESIGN_VARIANCE (1-10), MOTION_INTENSITY (1-10), and VISUAL_DENSITY (1-10) to guide layout asymmetry, animation levels, and whitespace.\n\
                 3. Avoid LLM Defaults: Do NOT default to AI-purple gradients, centered hero sections over dark mesh, slate-900 backgrounds, Inter font, and three identical cards. Be original and deliberate.\n\
                 4. Typography: Default to modern sans display fonts (Geist, Cabinet Grotesk, Satoshi) instead of Inter. Avoid Instrument Serif or Fraunces unless specifically requested. Ensure descenders on italic display headers are not clipped.\n\
                 5. Color & Radius: Use a single consistent accent color and a single corner-radius scale across the entire page (all-sharp, all-soft, or all-pill).\n\
                 6. Contrast & CTAs: Ensure all interactive elements pass WCAG AA contrast (min 4.5:1). CTA button labels must fit on one line without wrapping, and do not use duplicate CTA intents (e.g., mix 'Get in touch' and 'Contact us' on the same page).\n\n\
                 IMPORTANT: When you have gathered enough information and made the needed changes, \n\
                 you MUST provide a final text response summarizing your work. \n\
                 Do not keep calling tools indefinitely — be efficient and wrap up."
            );
            // Load any specialized local or global skills
            let skills = fusion_core::config::load_skills(&self.cwd);
            if !skills.is_empty() {
                sys_prompt.push_str("\n\nAVAILABLE SPECIALIZED SKILLS AND BEST PRACTICES:\n");
                for (name, content) in skills {
                    sys_prompt.push_str(&format!("--- SKILL: {} ---\n{}\n\n", name, content));
                }
            }

            // Load user taste preferences (personal coding styles)
            let taste_rules = fusion_core::taste::load_taste_rules(std::path::Path::new(&self.cwd));
            sys_prompt.push_str("\n\nUSER CODING STYLE PREFERENCES (TASTE PROFILE):\n");
            if !taste_rules.is_empty() {
                sys_prompt.push_str("Align all your code generations and edits to match these choices:\n");
                for rule in taste_rules {
                    sys_prompt.push_str(&format!("- {} (Confidence: {:.2})\n", rule.rule, rule.confidence));
                }
            } else {
                sys_prompt.push_str("Align your code to these default engineering taste guidelines:\n\
                                     - Prefer clean, self-documenting code with minimal, high-value comments.\n\
                                     - Write small, modular functions and components with a single clear responsibility.\n\
                                     - Use highly descriptive and clear naming for variables, functions, and files.\n\
                                     - Write robust error handling; avoid unwrap() or panics in production code.\n");
            }

            // Load user design preferences (UI/design patterns)
            let design_rules = fusion_core::design::load_design_rules(std::path::Path::new(&self.cwd));
            sys_prompt.push_str("\n\nUSER DESIGN PREFERENCES (DESIGN PROFILE):\n");
            if !design_rules.is_empty() {
                sys_prompt.push_str("Match all UI code, styling, and component choices to these design preferences:\n");
                for rule in design_rules {
                    sys_prompt.push_str(&format!("- {} (Confidence: {:.2})\n", rule.rule, rule.confidence));
                }
            } else {
                sys_prompt.push_str("Match all UI code, styling, and component choices to the existing codebase conventions.\n");
            }

            // Always embed Emil Kowalski's Design Engineering Principles by default
            sys_prompt.push_str("\nDESIGN ENGINEERING PRINCIPLES (EMIL KOWALSKI PHILOSOPHY):\n\
                                 Apply these motion and UI polish standards to all interface work:\n\
                                 - Never animate keyboard-initiated actions (command palette, shortcuts) — keep them instant.\n\
                                 - Specify exact transition properties; avoid 'transition: all' for clean performance.\n\
                                 - Never animate from scale(0) (nothing in the real world appears from absolute zero). Use scale(0.95) with opacity instead.\n\
                                 - Never use ease-in for UI enter transitions (feels sluggish). Always use ease-out with a custom curve (e.g., cubic-bezier(0.23, 1, 0.32, 1)).\n\
                                 - Buttons must feel responsive: transition transform over 100-160ms, scale down to 0.97 on :active.\n\
                                 - Origin-aware popovers: scale from their trigger, not the center.\n\
                                 - UI animations must remain under 300ms (100-160ms for button press, 150-250ms for dropdowns/selects, 200-500ms for modals/drawers).\n");


            if fusion_core::config::is_termux() {
                sys_prompt.push_str(
                    "\n\nTERMUX ENVIRONMENT PATHS:\n\
                     - Standard Linux paths like /bin/bash, /bin/sh, or /tmp DO NOT EXIST natively in Termux.\n\
                     - Always write shebangs in scripts as '#!/usr/bin/env bash' or '#!/usr/bin/env python' instead of hardcoded '/bin/bash'.\n\
                     - Create temporary files inside the current directory or under the Termux prefix ($PREFIX/tmp) instead of /tmp.\n\
                     - Termux utilities and binaries are located under the prefix '/data/data/com.termux/files/usr/'.\n"
                );
                sys_prompt.push_str("\n\nTERMUX API SKILL AND BEST PRACTICES:\n");
                sys_prompt.push_str(TERMUX_API_SKILL);
            } else if fusion_core::config::is_ish() {
                sys_prompt.push_str(
                    "\n\niSH (iOS ALPINE) ENVIRONMENT PATHS:\n\
                     - Standard bash is not installed by default. Always write scripts using '#!/bin/sh' or install bash via 'apk add bash' first.\n\
                     - Keep memory and disk writes low since iOS terminates long/heavy CPU-intensive processes.\n"
                );
            }

            if self.arbitrage_mode {
                sys_prompt.push_str(
                    "\n\nTOKEN ARBITRAGE MODE (ACTIVE):\n\
                     - You are the Premium Auditor/Planner model.\n\
                     - Your role is restricted to specification, planning, and validation/judgement.\n\
                     - **CRITICAL**: Do NOT use direct code writing tools (like write_file or search_replace) yourself. Instead, delegate ALL code writing and file editing tasks to the cheaper fast model by using the `delegate_write` tool.\n\
                     - When calling `delegate_write`, provide a clear, step-by-step description of the coding task, lists of files, and a test/build acceptance command.\n\
                     - After the `delegate_write` tool returns, review the returned git diff and verification outcomes. If there are remaining errors, call `delegate_write` again to fix them or refine the code.\n"
                );
            }

            self.messages.push(ChatMessage::system(sys_prompt));
        }

        self.messages.push(ChatMessage::user(user_message));

        let mut tool_schemas = build_tool_schemas();
        if self.arbitrage_mode {
            // Remove write_file and search_replace to force delegation
            tool_schemas.retain(|schema| {
                if let Some(func) = schema.get("function") {
                    if let Some(name) = func.get("name").and_then(|n| n.as_str()) {
                        return name != "write_file" && name != "search_replace";
                    }
                }
                true
            });

            // Add delegate_write tool schema
            tool_schemas.push(serde_json::json!({
                "type": "function",
                "function": {
                    "name": "delegate_write",
                    "description": "Delegate a coding/editing task to a fast, cheap model in the background. Mandatory in Arbitrage Mode.",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "task_description": {
                                "type": "string",
                                "description": "Detailed step-by-step instructions of what changes need to be made, context, and why."
                            },
                            "files": {
                                "type": "array",
                                "items": { "type": "string" },
                                "description": "The file paths (relative to project root) that need to be read or modified."
                            },
                            "acceptance_criteria": {
                                "type": "string",
                                "description": "A terminal command to run after editing to verify the changes (e.g. 'cargo test' or 'npm run build')."
                            }
                        },
                        "required": ["task_description", "files", "acceptance_criteria"]
                    }
                }
            }));
        }
        let max_rounds = 25;

        for round in 0..max_rounds {
            // Pacing delay to avoid triggering Cloudflare burst rate limits during tool use
            if round > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(2500)).await;
            }

            // Inject a wrap-up nudge when running low on rounds
            if round == max_rounds - 2 {
                self.messages.push(ChatMessage::system(
                    "IMPORTANT: You are running low on remaining rounds. \
                     Finish up your current work and provide a final text answer \
                     summarizing what you have done. Do NOT call more tools unless absolutely necessary."
                ));
            }

            // Boost max_tokens on final rounds so the model has enough budget to conclude
            let max_tokens = if round >= max_rounds - 3 {
                Some(16384)
            } else {
                None
            };

            let options = ChatOptions {
                messages: self.messages.clone(),
                tools: Some(tool_schemas.clone()),
                temperature: Some(0.4),
                max_tokens,
            };

            let (llm_tx, mut llm_rx) = tokio::sync::mpsc::unbounded_channel();
            let tx_clone = tx.clone();
            
            // Spawn background task to forward streaming events to TUI in real-time
            let forwarder = tokio::spawn(async move {
                while let Some(llm_event) = llm_rx.recv().await {
                    match llm_event {
                        fusion_llm::client::LlmEvent::Thinking(chunk) => {
                            let _ = tx_clone.send(AgentEvent::Thinking(chunk));
                        }
                        fusion_llm::client::LlmEvent::TextDelta(chunk) => {
                            let _ = tx_clone.send(AgentEvent::TextDelta(chunk));
                        }
                    }
                }
            });

            let result = match self.llm.chat(options, Some(llm_tx)).await {
                Ok(res) => res,
                Err(e) => {
                    let _ = forwarder.await;
                    return Err(e);
                }
            };
            let _ = forwarder.await;

            // Handle tool calls
            if !result.tool_calls.is_empty() {
                // Add assistant message with tool calls populated
                self.messages
                    .push(ChatMessage::assistant_with_tools(result.content.clone(), result.tool_calls.clone()));

                for tc in &result.tool_calls {
                    let args_str = serde_json::to_string(&tc.arguments).unwrap_or_default();
                    let preview = if args_str.chars().count() > 200 {
                        let truncated: String = args_str.chars().take(200).collect();
                        format!("{}...", truncated)
                    } else {
                        args_str.clone()
                    };

                    let _ = tx.send(AgentEvent::ToolCall {
                        name: tc.name.clone(),
                        args_preview: preview,
                    });

                    // Execute the tool
                    let output = if tc.name == "delegate_write" {
                        self.execute_delegate_write(&tc.arguments, tx.clone()).await
                    } else {
                        self.tool_registry
                            .execute(&tc.name, &tc.arguments)
                            .await
                            .unwrap_or_else(|e| format!("Tool execution error: {}", e))
                    };

                    let _ = tx.send(AgentEvent::ToolResult {
                        name: tc.name.clone(),
                        output: output.clone(),
                    });

                    // Handle todo updates
                    if tc.name == "todo_write" {
                        if let Ok(items) =
                            serde_json::from_value::<Vec<TodoItem>>(tc.arguments["todos"].clone())
                        {
                            self.todos = items.clone();
                            let _ = tx.send(AgentEvent::TodoUpdate(items));
                        }
                    }

                    self.messages.push(ChatMessage {
                        role: "tool".to_string(),
                        content: format!("Tool {} result:\n{}", tc.name, output),
                        name: Some(tc.name.clone()),
                        tool_call_id: Some(tc.id.clone()),
                        tool_calls: None,
                    });
                }
                continue; // next round
            }

            // Final response (no tool calls)
            let final_text = if result.content.is_empty() {
                "(no response)".to_string()
            } else {
                result.content.clone()
            };

            self.messages.push(ChatMessage::assistant(&final_text));
            let _ = tx.send(AgentEvent::FinalResponse(final_text));
            return Ok(());
        }

        // Force one final LLM call WITHOUT tools to guarantee a text summary
        self.messages.push(ChatMessage::system(
            "You have exhausted all available tool rounds. \
             You MUST now provide a final text response summarizing \
             what you accomplished and any remaining work. \
             Do NOT attempt any tool calls."
        ));

        let final_options = ChatOptions {
            messages: self.messages.clone(),
            tools: None, // No tools — forces text-only response
            temperature: Some(0.4),
            max_tokens: Some(16384),
        };

        let (llm_tx, mut llm_rx) = tokio::sync::mpsc::unbounded_channel();
        let tx_clone = tx.clone();
        let forwarder = tokio::spawn(async move {
            while let Some(llm_event) = llm_rx.recv().await {
                match llm_event {
                    fusion_llm::client::LlmEvent::Thinking(chunk) => {
                        let _ = tx_clone.send(AgentEvent::Thinking(chunk));
                    }
                    fusion_llm::client::LlmEvent::TextDelta(chunk) => {
                        let _ = tx_clone.send(AgentEvent::TextDelta(chunk));
                    }
                }
            }
        });

        match self.llm.chat(final_options, Some(llm_tx)).await {
            Ok(result) => {
                let _ = forwarder.await;
                let final_text = if result.content.is_empty() {
                    "Agent completed all rounds. No further summary available.".to_string()
                } else {
                    result.content
                };
                self.messages.push(ChatMessage::assistant(&final_text));
                let _ = tx.send(AgentEvent::FinalResponse(final_text));
            }
            Err(_) => {
                let _ = forwarder.await;
                let msg = "Agent completed all available rounds.".to_string();
                let _ = tx.send(AgentEvent::FinalResponse(msg));
            }
        }
        Ok(())
    }

    pub fn get_todos(&self) -> &[TodoItem] {
        &self.todos
    }

    pub fn get_history(&self) -> &[ChatMessage] {
        &self.messages
    }

    pub fn update_model(&mut self, model: &str) {
        self.llm.update_model(model);
    }

    async fn execute_delegate_write(
        &self,
        arguments: &serde_json::Value,
        tx: tokio::sync::mpsc::UnboundedSender<AgentEvent>,
    ) -> String {
        let task_description = arguments["task_description"].as_str().unwrap_or("");
        let files: Vec<String> = arguments["files"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let acceptance_criteria = arguments["acceptance_criteria"].as_str().unwrap_or("");

        let _ = tx.send(AgentEvent::TextDelta(format!(
            "\n[Arbitrage] Spawning fast coder sub-agent (Files: {:?})...\n",
            files
        )));

        // 1. Get the small model client
        let small_model_id = self.get_small_model_id();
        let mut small_config = self.config.clone();
        small_config.model = small_model_id.clone();
        let small_llm = create_llm_client(&small_config);

        // 2. Build the system prompt for the cheap model
        let files_list = files.join(", ");
        let sys_prompt = format!(
            "You are a fast, precise code writer assistant.\n\n\
             OBJECTIVE:\n\
             {}\n\n\
             TARGET FILES:\n\
             {}\n\n\
             ACCEPTANCE CRITERIA:\n\
             {}\n\n\
             INSTRUCTIONS:\n\
             1. Read the target files to understand current code.\n\
             2. Apply the necessary changes using search_replace or write_file.\n\
             3. Run the acceptance criteria command (run_command) to verify the build/tests.\n\
             4. If there are errors, correct them and re-run. Do NOT loop forever.\n\
             5. Once verified (or if you are stuck after 2 attempts), provide a summary and finish.",
            task_description, files_list, acceptance_criteria
        );

        // 3. Initialize message history for the sub-agent
        let mut sub_messages = vec![
            ChatMessage::system(sys_prompt),
            ChatMessage::user("Please implement the requested changes now."),
        ];

        // 4. Run the sub-agent loop (up to 4 rounds)
        let sub_tool_schemas = build_tool_schemas();
        let max_rounds = 4;
        let mut sub_summary = String::new();

        for _round in 0..max_rounds {
            let options = ChatOptions {
                messages: sub_messages.clone(),
                tools: Some(sub_tool_schemas.clone()),
                temperature: Some(0.2),
                max_tokens: None,
            };

            // Call the cheap LLM (non-streaming)
            let result = match small_llm.chat(options, None).await {
                Ok(res) => res,
                Err(e) => {
                    return format!("Arbitrage sub-agent LLM error: {}", e);
                }
            };

            if !result.content.is_empty() {
                sub_summary = result.content.clone();
            }

            if result.tool_calls.is_empty() {
                break;
            }

            sub_messages.push(ChatMessage::assistant_with_tools(
                result.content.clone(),
                result.tool_calls.clone(),
            ));

            for tc in &result.tool_calls {
                let args_str = serde_json::to_string(&tc.arguments).unwrap_or_default();
                let preview = if args_str.chars().count() > 100 {
                    let truncated: String = args_str.chars().take(100).collect();
                    format!("{}...", truncated)
                } else {
                    args_str.clone()
                };

                let _ = tx.send(AgentEvent::TextDelta(format!(
                    "  [Arbitrage Coder] calling {} with {}\n",
                    tc.name, preview
                )));

                let output = self
                    .tool_registry
                    .execute(&tc.name, &tc.arguments)
                    .await
                    .unwrap_or_else(|e| format!("Tool execution error: {}", e));

                sub_messages.push(ChatMessage {
                    role: "tool".to_string(),
                    content: format!("Tool {} result:\n{}", tc.name, output),
                    name: Some(tc.name.clone()),
                    tool_call_id: Some(tc.id.clone()),
                    tool_calls: None,
                });
            }
        }

        // 5. Gather git diff
        let diff_args = serde_json::json!({
            "command": "git diff",
            "timeout_secs": 10
        });
        let git_diff = self
            .tool_registry
            .execute("run_command", &diff_args)
            .await
            .unwrap_or_else(|_| "Failed to get git diff".to_string());

        // 6. Run verification command one last time
        let verify_args = serde_json::json!({
            "command": acceptance_criteria,
            "timeout_secs": 60
        });
        let verification = if !acceptance_criteria.trim().is_empty() {
            self.tool_registry
                .execute("run_command", &verify_args)
                .await
                .unwrap_or_else(|_| "Failed to run acceptance command".to_string())
        } else {
            "No acceptance command provided".to_string()
        };

        format!(
            "Arbitrage Sub-Agent Execution Results:\n\n\
             Summary:\n{}\n\n\
             Git Diff:\n```diff\n{}\n```\n\n\
             Verification Result:\n{}\n",
            sub_summary, git_diff, verification
        )
    }

    fn get_small_model_id(&self) -> String {
        if let Some(ref m) = self.config.small_model {
            if !m.is_empty() {
                return m.clone();
            }
        }
        "@cf/zai-org/glm-4.7-flash".to_string()
    }
}

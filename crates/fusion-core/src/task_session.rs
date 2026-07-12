use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::FusionError;
use crate::session::SessionMessage;

/// Status of a sub-agent task.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskStatus {
    Running,
    Completed,
    Failed(String),
    TimedOut,
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskStatus::Running => write!(f, "running"),
            TaskStatus::Completed => write!(f, "completed"),
            TaskStatus::Failed(reason) => write!(f, "failed: {}", reason),
            TaskStatus::TimedOut => write!(f, "timed_out"),
        }
    }
}

/// A persisted sub-agent task session, supporting resume via `task_id`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSession {
    pub task_id: String,
    pub parent_session_id: Option<String>,
    pub persona: String,
    pub status: TaskStatus,
    pub description: String,
    pub summary: Option<String>,
    pub model: String,
    pub cwd: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub messages: Vec<SessionMessage>,
}

/// Summary for listing task sessions without loading full messages.
#[derive(Debug, Clone)]
pub struct TaskSessionSummary {
    pub task_id: String,
    pub persona: String,
    pub status: TaskStatus,
    pub description: String,
    pub model: String,
    pub updated_at: u64,
    pub message_count: usize,
}

impl TaskSession {
    /// Create a new task session with a generated ID.
    pub fn new(
        persona: &str,
        description: &str,
        model: &str,
        cwd: &str,
        parent_session_id: Option<String>,
    ) -> Self {
        let now = now_epoch();
        Self {
            task_id: generate_task_id(),
            parent_session_id,
            persona: persona.to_string(),
            status: TaskStatus::Running,
            description: description.to_string(),
            summary: None,
            model: model.to_string(),
            cwd: cwd.to_string(),
            created_at: now,
            updated_at: now,
            messages: Vec::new(),
        }
    }

    /// Add a message to the task session.
    pub fn push_message(&mut self, role: &str, content: &str) {
        self.updated_at = now_epoch();
        self.messages.push(SessionMessage {
            role: role.to_string(),
            content: content.to_string(),
            tool_call_id: None,
            name: None,
        });
    }

    /// Mark the task as completed with a summary.
    pub fn complete(&mut self, summary: String) {
        self.status = TaskStatus::Completed;
        self.summary = Some(summary);
        self.updated_at = now_epoch();
    }

    /// Mark the task as failed with a reason.
    pub fn fail(&mut self, reason: String) {
        self.status = TaskStatus::Failed(reason);
        self.updated_at = now_epoch();
    }

    /// Mark the task as timed out.
    pub fn timeout(&mut self) {
        self.status = TaskStatus::TimedOut;
        self.updated_at = now_epoch();
    }

    /// Save task session to disk.
    pub fn save(&self) -> Result<PathBuf, FusionError> {
        let dir = task_sessions_dir()?;
        fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{}.json", self.task_id));
        let data = serde_json::to_string_pretty(self)?;
        fs::write(&path, data)?;
        Ok(path)
    }

    /// Load a task session by ID.
    pub fn load(task_id: &str) -> Result<Self, FusionError> {
        let dir = task_sessions_dir()?;
        let path = dir.join(format!("{}.json", task_id));
        if !path.exists() {
            return Err(FusionError::Config(format!(
                "Task session '{}' not found",
                task_id
            )));
        }
        let data = fs::read_to_string(&path)?;
        let session: TaskSession = serde_json::from_str(&data)?;
        Ok(session)
    }

    /// Delete a task session by ID.
    pub fn delete(task_id: &str) -> Result<(), FusionError> {
        let dir = task_sessions_dir()?;
        let path = dir.join(format!("{}.json", task_id));
        if path.exists() {
            fs::remove_file(&path)?;
        }
        Ok(())
    }

    /// Short display ID (first 8 chars).
    pub fn short_id(&self) -> &str {
        if self.task_id.len() >= 8 {
            &self.task_id[..8]
        } else {
            &self.task_id
        }
    }
}

/// List all saved task sessions, newest first.
pub fn list_task_sessions() -> Result<Vec<TaskSessionSummary>, FusionError> {
    let dir = task_sessions_dir()?;
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut sessions: Vec<TaskSessionSummary> = Vec::new();

    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        if let Ok(data) = fs::read_to_string(&path) {
            if let Ok(session) = serde_json::from_str::<TaskSession>(&data) {
                sessions.push(TaskSessionSummary {
                    task_id: session.task_id,
                    persona: session.persona,
                    status: session.status,
                    description: session.description,
                    model: session.model,
                    updated_at: session.updated_at,
                    message_count: session.messages.len(),
                });
            }
        }
    }

    sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(sessions)
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn task_sessions_dir() -> Result<PathBuf, FusionError> {
    let home = dirs::home_dir()
        .ok_or_else(|| FusionError::Config("Cannot find home directory".to_string()))?;
    Ok(home.join(".fusion").join("task_sessions"))
}

fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn generate_task_id() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let pid = std::process::id();
    // Mix time nanos with PID for uniqueness across concurrent spawns
    let hash = now.wrapping_mul(6364136223846793005).wrapping_add(pid as u128);
    format!("task-{:016x}", hash as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_session_create() {
        let ts = TaskSession::new("worker", "fix bug #42", "grok-3", "/tmp", None);
        assert!(ts.task_id.starts_with("task-"));
        assert_eq!(ts.persona, "worker");
        assert_eq!(ts.status, TaskStatus::Running);
        assert_eq!(ts.description, "fix bug #42");
    }

    #[test]
    fn test_task_session_status_transitions() {
        let mut ts = TaskSession::new("scout", "explore", "grok-3", "/tmp", None);
        assert_eq!(ts.status, TaskStatus::Running);

        ts.complete("Found 3 relevant files.".to_string());
        assert_eq!(ts.status, TaskStatus::Completed);
        assert_eq!(ts.summary.as_deref(), Some("Found 3 relevant files."));

        let mut ts2 = TaskSession::new("worker", "write code", "grok-3", "/tmp", None);
        ts2.fail("API rate limited".to_string());
        assert!(matches!(ts2.status, TaskStatus::Failed(_)));

        let mut ts3 = TaskSession::new("worker", "long task", "grok-3", "/tmp", None);
        ts3.timeout();
        assert_eq!(ts3.status, TaskStatus::TimedOut);
    }

    #[test]
    fn test_task_session_save_load_delete() {
        let mut ts = TaskSession::new("reviewer", "review PR", "test-model", "/tmp", None);
        ts.push_message("system", "You are a reviewer.");
        ts.push_message("user", "Review this code.");

        let path = ts.save().unwrap();
        assert!(path.exists());

        let loaded = TaskSession::load(&ts.task_id).unwrap();
        assert_eq!(loaded.task_id, ts.task_id);
        assert_eq!(loaded.persona, "reviewer");
        assert_eq!(loaded.messages.len(), 2);

        TaskSession::delete(&ts.task_id).unwrap();
        assert!(TaskSession::load(&ts.task_id).is_err());
    }

    #[test]
    fn test_task_session_short_id() {
        let ts = TaskSession::new("planner", "plan arch", "grok-3", "/tmp", None);
        assert_eq!(ts.short_id().len(), 8);
    }

    #[test]
    fn test_generate_task_id_unique() {
        let id1 = generate_task_id();
        // Small delay to ensure different nanos
        std::thread::sleep(std::time::Duration::from_millis(1));
        let id2 = generate_task_id();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_task_status_display() {
        assert_eq!(TaskStatus::Running.to_string(), "running");
        assert_eq!(TaskStatus::Completed.to_string(), "completed");
        assert_eq!(
            TaskStatus::Failed("oops".to_string()).to_string(),
            "failed: oops"
        );
        assert_eq!(TaskStatus::TimedOut.to_string(), "timed_out");
    }
}

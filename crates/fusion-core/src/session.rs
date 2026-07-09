use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::FusionError;

/// A saved session — conversation history with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub cwd: String,
    pub model: String,
    pub messages: Vec<SessionMessage>,
}

/// A message stored in a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMessage {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl Session {
    /// Create a new session with a generated ID.
    pub fn new(cwd: &str, model: &str) -> Self {
        let now = now_epoch();
        let id = generate_session_id();
        Self {
            id,
            created_at: now,
            updated_at: now,
            cwd: cwd.to_string(),
            model: model.to_string(),
            messages: Vec::new(),
        }
    }

    /// Add a message to the session.
    pub fn push_message(&mut self, role: &str, content: &str) {
        self.updated_at = now_epoch();
        self.messages.push(SessionMessage {
            role: role.to_string(),
            content: content.to_string(),
            tool_call_id: None,
            name: None,
        });
    }

    /// Save session to disk.
    pub fn save(&self) -> Result<PathBuf, FusionError> {
        let dir = sessions_dir()?;
        fs::create_dir_all(&dir)?;

        let path = dir.join(format!("{}.json", self.id));
        let data = serde_json::to_string_pretty(self)?;
        fs::write(&path, data)?;
        Ok(path)
    }

    /// Load a session by ID.
    pub fn load(id: &str) -> Result<Self, FusionError> {
        let dir = sessions_dir()?;
        let path = dir.join(format!("{}.json", id));

        if !path.exists() {
            return Err(FusionError::Config(format!("Session '{}' not found", id)));
        }

        let data = fs::read_to_string(&path)?;
        let session: Session = serde_json::from_str(&data)?;
        Ok(session)
    }

    /// Load the most recent session.
    pub fn load_last() -> Result<Self, FusionError> {
        let sessions = list_sessions()?;
        if sessions.is_empty() {
            return Err(FusionError::Config("No sessions found".to_string()));
        }
        // Sessions are sorted newest first
        Session::load(&sessions[0].id)
    }

    /// Short display ID (first 8 chars).
    pub fn short_id(&self) -> &str {
        if self.id.len() >= 8 {
            &self.id[..8]
        } else {
            &self.id
        }
    }
}

/// Summary of a session for listing.
#[derive(Debug, Clone)]
pub struct SessionSummary {
    pub id: String,
    pub updated_at: u64,
    pub model: String,
    pub message_count: usize,
    pub cwd: String,
    pub preview: String,
}

/// List all saved sessions, newest first.
pub fn list_sessions() -> Result<Vec<SessionSummary>, FusionError> {
    let dir = sessions_dir()?;
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut sessions: Vec<SessionSummary> = Vec::new();

    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }

        if let Ok(data) = fs::read_to_string(&path) {
            if let Ok(session) = serde_json::from_str::<Session>(&data) {
                // Get a preview from the first user message
                let preview = session
                    .messages
                    .iter()
                    .find(|m| m.role == "user")
                    .map(|m| {
                        if m.content.chars().count() > 60 {
                            let truncated: String = m.content.chars().take(60).collect();
                            format!("{}…", truncated)
                        } else {
                            m.content.clone()
                        }
                    })
                    .unwrap_or_else(|| "(empty)".to_string());

                sessions.push(SessionSummary {
                    id: session.id,
                    updated_at: session.updated_at,
                    model: session.model,
                    message_count: session.messages.len(),
                    cwd: session.cwd,
                    preview,
                });
            }
        }
    }

    // Sort by updated_at, newest first
    sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

    Ok(sessions)
}

/// Delete a session by ID.
pub fn delete_session(id: &str) -> Result<(), FusionError> {
    let dir = sessions_dir()?;
    let path = dir.join(format!("{}.json", id));
    if path.exists() {
        fs::remove_file(&path)?;
    }
    Ok(())
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn sessions_dir() -> Result<PathBuf, FusionError> {
    let home = dirs::home_dir()
        .ok_or_else(|| FusionError::Config("Cannot find home directory".to_string()))?;
    Ok(home.join(".fusion").join("sessions"))
}

fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn generate_session_id() -> String {
    // 16 hex chars from current time + random-ish bits
    let now = now_epoch();
    let pid = std::process::id();
    let hash = now.wrapping_mul(6364136223846793005).wrapping_add(pid as u64);
    format!("{:016x}", hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_create_and_save() {
        let mut session = Session::new("/tmp/test", "grok-3");
        assert_eq!(session.id.len(), 16);
        assert_eq!(session.model, "grok-3");

        session.push_message("user", "hello world");
        session.push_message("assistant", "hi there!");

        assert_eq!(session.messages.len(), 2);
        assert_eq!(session.messages[0].role, "user");
    }

    #[test]
    fn test_session_short_id() {
        let session = Session::new("/tmp", "test");
        assert_eq!(session.short_id().len(), 8);
    }

    #[test]
    fn test_session_save_and_load() {
        let mut session = Session::new("/tmp/test-fusion", "test-model");
        session.push_message("user", "test message");

        let path = session.save().unwrap();
        assert!(path.exists());

        let loaded = Session::load(&session.id).unwrap();
        assert_eq!(loaded.id, session.id);
        assert_eq!(loaded.messages.len(), 1);
        assert_eq!(loaded.messages[0].content, "test message");

        // Cleanup
        let _ = delete_session(&session.id);
    }
}

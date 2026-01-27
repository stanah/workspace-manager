//! Protocol definitions for notify messages

use serde::{Deserialize, Serialize};

/// Notification message types for AI CLI tools
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NotifyMessage {
    /// Register a new workspace session
    Register {
        /// Session ID from the AI CLI tool
        session_id: String,
        /// Project path being worked on
        project_path: String,
        /// AI CLI tool name (claude, kiro, opencode, codex)
        #[serde(default)]
        tool: Option<String>,
    },
    /// Update workspace status
    Status {
        /// Session ID from the AI CLI tool
        session_id: String,
        /// New status (working, idle)
        status: String,
        /// Optional status message
        #[serde(default)]
        message: Option<String>,
    },
    /// Unregister a workspace session
    Unregister {
        /// Session ID from the AI CLI tool
        session_id: String,
    },
    /// Tab focus changed (from Zellij plugin)
    TabFocus {
        /// Tab name that received focus
        tab_name: String,
    },
}

impl NotifyMessage {
    /// Get the session_id from any message type
    pub fn session_id(&self) -> &str {
        match self {
            NotifyMessage::Register { session_id, .. } => session_id,
            NotifyMessage::Status { session_id, .. } => session_id,
            NotifyMessage::Unregister { session_id } => session_id,
            NotifyMessage::TabFocus { tab_name } => tab_name,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_register() {
        let msg = NotifyMessage::Register {
            session_id: "abc123".to_string(),
            project_path: "/path/to/project".to_string(),
            tool: Some("claude".to_string()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"register\""));
        assert!(json.contains("\"session_id\":\"abc123\""));
    }

    #[test]
    fn test_serialize_status() {
        let msg = NotifyMessage::Status {
            session_id: "abc123".to_string(),
            status: "working".to_string(),
            message: Some("Processing...".to_string()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"status\""));
        assert!(json.contains("\"status\":\"working\""));
    }

    #[test]
    fn test_deserialize_register() {
        let json = r#"{"type":"register","session_id":"test","project_path":"/tmp"}"#;
        let msg: NotifyMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.session_id(), "test");
    }
}

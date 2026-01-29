//! Session management for AI CLI tools
//!
//! This module provides the Session structure that represents an active AI CLI session
//! (Claude Code, Kiro, etc.) within a workspace. Each workspace can have multiple sessions.

use serde::{Deserialize, Serialize};
use std::time::SystemTime;
use uuid::Uuid;

/// Unique identifier for a session
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(Uuid);

impl SessionId {
    /// Create a new random session ID
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create from a UUID
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Get the inner UUID
    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// AI CLI tool type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AiTool {
    /// Claude Code (Anthropic)
    #[default]
    Claude,
    /// Kiro CLI (AWS)
    Kiro,
    /// OpenCode
    OpenCode,
    /// Codex (OpenAI)
    Codex,
}

impl AiTool {
    /// Parse from string
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "claude" => AiTool::Claude,
            "kiro" => AiTool::Kiro,
            "opencode" => AiTool::OpenCode,
            "codex" => AiTool::Codex,
            _ => AiTool::Claude,
        }
    }

    /// Get display name
    pub fn name(&self) -> &'static str {
        match self {
            AiTool::Claude => "Claude",
            AiTool::Kiro => "Kiro",
            AiTool::OpenCode => "OpenCode",
            AiTool::Codex => "Codex",
        }
    }

    /// Get short icon/prefix for display
    pub fn icon(&self) -> &'static str {
        match self {
            AiTool::Claude => "✻",
            AiTool::Kiro => "\u{F02A0}",
            AiTool::OpenCode => "[O]",
            AiTool::Codex => "[X]",
        }
    }

    /// Get color for ratatui
    pub fn color(&self) -> ratatui::style::Color {
        use ratatui::style::Color;
        match self {
            AiTool::Claude => Color::Rgb(204, 119, 34), // Orange/brown for Claude
            AiTool::Kiro => Color::Rgb(153, 102, 204),   // Purple for Kiro
            AiTool::OpenCode => Color::Cyan,
            AiTool::Codex => Color::Green,
        }
    }
}

impl std::fmt::Display for AiTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// Session status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    /// Session is idle (waiting for user input)
    #[default]
    Idle,
    /// Session is actively working
    Working,
    /// Session is waiting for user confirmation
    NeedsInput,
    /// Session completed successfully
    Success,
    /// Session encountered an error
    Error,
    /// Session has ended/disconnected
    Disconnected,
}

impl SessionStatus {
    /// Parse from string
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "working" => SessionStatus::Working,
            "idle" => SessionStatus::Idle,
            "needs_input" | "waiting" => SessionStatus::NeedsInput,
            "success" | "completed" => SessionStatus::Success,
            "error" => SessionStatus::Error,
            "disconnected" | "ended" => SessionStatus::Disconnected,
            _ => SessionStatus::Idle,
        }
    }

    /// Get status icon
    pub fn icon(&self) -> &'static str {
        match self {
            SessionStatus::Idle => "○",
            SessionStatus::Working => "●",
            SessionStatus::NeedsInput => "●",
            SessionStatus::Success => "✓",
            SessionStatus::Error => "✗",
            SessionStatus::Disconnected => "◌",
        }
    }

    /// Get color for ratatui
    pub fn color(&self) -> ratatui::style::Color {
        use ratatui::style::Color;
        match self {
            SessionStatus::Idle => Color::Gray,
            SessionStatus::Working => Color::Blue,
            SessionStatus::NeedsInput => Color::Yellow,
            SessionStatus::Success => Color::Green,
            SessionStatus::Error => Color::Red,
            SessionStatus::Disconnected => Color::DarkGray,
        }
    }
}

impl std::fmt::Display for SessionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            SessionStatus::Idle => "idle",
            SessionStatus::Working => "working",
            SessionStatus::NeedsInput => "needs_input",
            SessionStatus::Success => "success",
            SessionStatus::Error => "error",
            SessionStatus::Disconnected => "disconnected",
        };
        write!(f, "{}", s)
    }
}

/// An active AI CLI session within a workspace
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Internal unique identifier
    pub id: SessionId,
    /// External ID from the tool (Claude Code: session_id, Kiro: synthetic)
    /// Format: "claude:{uuid}" or "kiro:{project_path}"
    pub external_id: String,
    /// Index of the parent workspace
    pub workspace_index: usize,
    /// AI tool type
    pub tool: AiTool,
    /// Current status
    pub status: SessionStatus,
    /// Detailed state information
    #[serde(default)]
    pub state_detail: Option<String>,
    /// Brief summary of current work (max 50 chars)
    #[serde(default)]
    pub summary: Option<String>,
    /// Current task description
    #[serde(default)]
    pub current_task: Option<String>,
    /// Last activity timestamp
    #[serde(default)]
    pub last_activity: Option<SystemTime>,
    /// Zellij pane ID (Internal mode)
    #[serde(default)]
    pub pane_id: Option<u32>,
    /// Zellij tab name (External mode)
    #[serde(default)]
    pub tab_name: Option<String>,
    /// Session creation time
    pub created_at: SystemTime,
    /// Last update time
    pub updated_at: SystemTime,
}

impl Session {
    /// Create a new session
    pub fn new(external_id: String, workspace_index: usize, tool: AiTool) -> Self {
        let now = SystemTime::now();
        Self {
            id: SessionId::new(),
            external_id,
            workspace_index,
            tool,
            status: SessionStatus::Idle,
            state_detail: None,
            summary: None,
            current_task: None,
            last_activity: Some(now),
            pane_id: None,
            tab_name: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Update session status
    pub fn update_status(&mut self, status: SessionStatus, message: Option<String>) {
        self.status = status;
        if message.is_some() {
            self.summary = message;
        }
        self.updated_at = SystemTime::now();
        self.last_activity = Some(SystemTime::now());
    }

    /// Update from SessionStatus (logwatch schema)
    pub fn update_from_logwatch_status(&mut self, status: &crate::logwatch::SessionStatus) {
        // Update summary (truncate to 50 chars)
        self.summary = status.display_summary();

        // Update current task
        self.current_task = status.current_task.clone();

        // Update state detail label
        self.state_detail = Some(status.state_detail.label().to_string());

        // Convert StatusState to SessionStatus
        self.status = match status.status {
            crate::logwatch::StatusState::Working => SessionStatus::Working,
            crate::logwatch::StatusState::Waiting => SessionStatus::NeedsInput,
            crate::logwatch::StatusState::Completed => SessionStatus::Success,
            crate::logwatch::StatusState::Error => SessionStatus::Error,
            crate::logwatch::StatusState::Idle => SessionStatus::Idle,
            crate::logwatch::StatusState::Disconnected => SessionStatus::Disconnected,
        };

        // Update timestamps
        if let Some(activity) = status.last_activity {
            self.last_activity = activity
                .timestamp_millis()
                .try_into()
                .ok()
                .map(|millis: u64| {
                    std::time::UNIX_EPOCH + std::time::Duration::from_millis(millis)
                });
        }

        self.updated_at = SystemTime::now();
    }

    /// Get time since last activity as human-readable string
    pub fn time_since_activity(&self) -> Option<String> {
        self.last_activity.and_then(|t| {
            t.elapsed().ok().map(|duration| {
                let secs = duration.as_secs();
                if secs < 60 {
                    format!("{}s ago", secs)
                } else if secs < 3600 {
                    format!("{}m ago", secs / 60)
                } else if secs < 86400 {
                    format!("{}h ago", secs / 3600)
                } else {
                    format!("{}d ago", secs / 86400)
                }
            })
        })
    }

    /// Mark session as disconnected
    pub fn disconnect(&mut self) {
        self.status = SessionStatus::Disconnected;
        self.updated_at = SystemTime::now();
    }

    /// Check if session is active (not disconnected)
    pub fn is_active(&self) -> bool {
        self.status != SessionStatus::Disconnected
    }

    /// Get display summary with state detail
    pub fn display_info(&self) -> String {
        let mut parts = Vec::new();

        if let Some(ref detail) = self.state_detail {
            parts.push(format!("[{}]", detail));
        }

        if let Some(ref summary) = self.summary {
            parts.push(summary.clone());
        }

        if let Some(time) = self.time_since_activity() {
            parts.push(format!("({})", time));
        }

        parts.join(" ")
    }
}

/// Generate external session ID for Claude Code
pub fn claude_external_id(session_id: &str) -> String {
    format!("claude:{}", session_id)
}

/// Generate external session ID for Kiro (with conversation ID)
pub fn kiro_external_id(project_path: &str, conversation_id: &str) -> String {
    format!("kiro:{}:{}", project_path, conversation_id)
}

/// Generate external session ID for Kiro (legacy - without conversation ID)
pub fn kiro_external_id_legacy(project_path: &str) -> String {
    format!("kiro:{}", project_path)
}

/// Parse external session ID to get tool and original ID
/// Returns (tool, raw_id) where raw_id is everything after the tool prefix
pub fn parse_external_id(external_id: &str) -> (AiTool, &str) {
    if let Some(id) = external_id.strip_prefix("claude:") {
        (AiTool::Claude, id)
    } else if let Some(id) = external_id.strip_prefix("kiro:") {
        (AiTool::Kiro, id)
    } else if let Some(id) = external_id.strip_prefix("opencode:") {
        (AiTool::OpenCode, id)
    } else if let Some(id) = external_id.strip_prefix("codex:") {
        (AiTool::Codex, id)
    } else {
        // Default to Claude for legacy compatibility
        (AiTool::Claude, external_id)
    }
}

/// Parse Kiro external ID to get project path and conversation ID
/// Format: kiro:{project_path}:{conversation_id}
pub fn parse_kiro_external_id(external_id: &str) -> Option<(&str, &str)> {
    let id = external_id.strip_prefix("kiro:")?;
    // Find the last colon to split project_path and conversation_id
    let last_colon = id.rfind(':')?;
    let project_path = &id[..last_colon];
    let conversation_id = &id[last_colon + 1..];
    Some((project_path, conversation_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_id() {
        let id1 = SessionId::new();
        let id2 = SessionId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_ai_tool_parsing() {
        assert_eq!(AiTool::from_str("claude"), AiTool::Claude);
        assert_eq!(AiTool::from_str("kiro"), AiTool::Kiro);
        assert_eq!(AiTool::from_str("CLAUDE"), AiTool::Claude);
        assert_eq!(AiTool::from_str("unknown"), AiTool::Claude);
    }

    #[test]
    fn test_session_status_parsing() {
        assert_eq!(SessionStatus::from_str("working"), SessionStatus::Working);
        assert_eq!(SessionStatus::from_str("idle"), SessionStatus::Idle);
        assert_eq!(SessionStatus::from_str("waiting"), SessionStatus::NeedsInput);
    }

    #[test]
    fn test_external_id_generation() {
        assert_eq!(claude_external_id("abc-123"), "claude:abc-123");
        assert_eq!(kiro_external_id("/path/to/project", "conv-123"), "kiro:/path/to/project:conv-123");
        assert_eq!(kiro_external_id_legacy("/path/to/project"), "kiro:/path/to/project");
    }

    #[test]
    fn test_external_id_parsing() {
        let (tool, id) = parse_external_id("claude:abc-123");
        assert_eq!(tool, AiTool::Claude);
        assert_eq!(id, "abc-123");

        // New format: kiro:{project_path}:{conversation_id}
        let (tool, id) = parse_external_id("kiro:/path/to/project:conv-123");
        assert_eq!(tool, AiTool::Kiro);
        assert_eq!(id, "/path/to/project:conv-123");

        // Parse kiro external id to get project path and conversation id
        let (project_path, conv_id) = parse_kiro_external_id("kiro:/path/to/project:conv-123").unwrap();
        assert_eq!(project_path, "/path/to/project");
        assert_eq!(conv_id, "conv-123");
    }

    #[test]
    fn test_session_creation() {
        let session = Session::new("claude:test-123".to_string(), 0, AiTool::Claude);
        assert_eq!(session.external_id, "claude:test-123");
        assert_eq!(session.workspace_index, 0);
        assert_eq!(session.tool, AiTool::Claude);
        assert_eq!(session.status, SessionStatus::Idle);
        assert!(session.is_active());
    }

    #[test]
    fn test_session_disconnect() {
        let mut session = Session::new("claude:test-123".to_string(), 0, AiTool::Claude);
        assert!(session.is_active());

        session.disconnect();
        assert!(!session.is_active());
        assert_eq!(session.status, SessionStatus::Disconnected);
    }
}

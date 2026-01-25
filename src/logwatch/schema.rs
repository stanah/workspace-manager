//! Schema definitions for log analysis results

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Main status states
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum StatusState {
    /// AI is actively working on a task
    Working,
    /// Waiting for user input or confirmation
    Waiting,
    /// Task completed successfully
    Completed,
    /// An error occurred
    Error,
    /// Session is inactive
    #[default]
    Idle,
    /// Session has ended
    Disconnected,
}

impl StatusState {
    /// Convert to display string
    pub fn as_str(&self) -> &'static str {
        match self {
            StatusState::Working => "working",
            StatusState::Waiting => "waiting",
            StatusState::Completed => "completed",
            StatusState::Error => "error",
            StatusState::Idle => "idle",
            StatusState::Disconnected => "disconnected",
        }
    }

    /// Get status icon
    pub fn icon(&self) -> &'static str {
        match self {
            StatusState::Working => "●",
            StatusState::Waiting => "●",
            StatusState::Completed => "✓",
            StatusState::Error => "✗",
            StatusState::Idle => "○",
            StatusState::Disconnected => "◌",
        }
    }

    /// Get color for ratatui
    pub fn color(&self) -> ratatui::style::Color {
        use ratatui::style::Color;
        match self {
            StatusState::Working => Color::Blue,
            StatusState::Waiting => Color::Yellow,
            StatusState::Completed => Color::Green,
            StatusState::Error => Color::Red,
            StatusState::Idle => Color::Gray,
            StatusState::Disconnected => Color::DarkGray,
        }
    }
}

/// Detailed status information
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum StatusDetail {
    // Working states
    Thinking,
    ExecutingTool,
    WritingCode,

    // Waiting states
    UserInput,
    Confirmation,

    // Completed states
    Success,
    Partial,

    // Error states
    ApiError,
    ToolError,

    // Idle/Disconnected states
    #[default]
    Inactive,
    SessionEnded,
}

impl StatusDetail {
    pub fn as_str(&self) -> &'static str {
        match self {
            StatusDetail::Thinking => "thinking",
            StatusDetail::ExecutingTool => "executing_tool",
            StatusDetail::WritingCode => "writing_code",
            StatusDetail::UserInput => "user_input",
            StatusDetail::Confirmation => "confirmation",
            StatusDetail::Success => "success",
            StatusDetail::Partial => "partial",
            StatusDetail::ApiError => "api_error",
            StatusDetail::ToolError => "tool_error",
            StatusDetail::Inactive => "inactive",
            StatusDetail::SessionEnded => "session_ended",
        }
    }

    /// Human-readable label for display
    pub fn label(&self) -> &'static str {
        match self {
            StatusDetail::Thinking => "thinking",
            StatusDetail::ExecutingTool => "running tool",
            StatusDetail::WritingCode => "writing code",
            StatusDetail::UserInput => "needs input",
            StatusDetail::Confirmation => "confirm?",
            StatusDetail::Success => "done",
            StatusDetail::Partial => "partial",
            StatusDetail::ApiError => "API error",
            StatusDetail::ToolError => "tool error",
            StatusDetail::Inactive => "inactive",
            StatusDetail::SessionEnded => "ended",
        }
    }
}

/// Progress tracking for multi-step tasks
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AnalysisProgress {
    /// Steps that have been completed
    #[serde(default)]
    pub completed_steps: Vec<String>,
    /// Current step being worked on
    #[serde(default)]
    pub current_step: Option<String>,
    /// Steps remaining to be done
    #[serde(default)]
    pub pending_steps: Vec<String>,
}

/// Additional context from analysis
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AnalysisContext {
    /// Files that have been modified
    #[serde(default)]
    pub files_modified: Vec<String>,
    /// Approximate tokens used
    #[serde(default)]
    pub tokens_used: Option<u64>,
    /// Model being used
    #[serde(default)]
    pub model: Option<String>,
}

/// Complete session status from AI analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStatus {
    /// Session identifier
    #[serde(default)]
    pub session_id: Option<String>,
    /// Project path being worked on
    #[serde(default)]
    pub project_path: Option<String>,
    /// Tool name (claude, kiro)
    #[serde(default)]
    pub tool: Option<String>,
    /// Main status
    pub status: StatusState,
    /// Detailed state
    #[serde(default)]
    pub state_detail: StatusDetail,
    /// Brief summary of current work (max 50 chars)
    #[serde(default)]
    pub summary: Option<String>,
    /// Current task description
    #[serde(default)]
    pub current_task: Option<String>,
    /// Timestamp of last activity
    #[serde(default)]
    pub last_activity: Option<DateTime<Utc>>,
    /// Progress tracking
    #[serde(default)]
    pub progress: Option<AnalysisProgress>,
    /// Error message if status is error
    #[serde(default)]
    pub error: Option<String>,
    /// Additional context
    #[serde(default)]
    pub context: Option<AnalysisContext>,
}

impl Default for SessionStatus {
    fn default() -> Self {
        Self {
            session_id: None,
            project_path: None,
            tool: None,
            status: StatusState::Idle,
            state_detail: StatusDetail::Inactive,
            summary: None,
            current_task: None,
            last_activity: None,
            progress: None,
            error: None,
            context: None,
        }
    }
}

impl SessionStatus {
    /// Create a new idle session status
    pub fn new_idle() -> Self {
        Self::default()
    }

    /// Create an error status
    pub fn new_error(error: String) -> Self {
        Self {
            status: StatusState::Error,
            state_detail: StatusDetail::ApiError,
            error: Some(error),
            last_activity: Some(Utc::now()),
            ..Default::default()
        }
    }

    /// Create a disconnected status
    pub fn new_disconnected() -> Self {
        Self {
            status: StatusState::Disconnected,
            state_detail: StatusDetail::SessionEnded,
            last_activity: Some(Utc::now()),
            ..Default::default()
        }
    }

    /// Get a truncated summary for display (max 50 chars)
    pub fn display_summary(&self) -> Option<String> {
        self.summary.as_ref().map(|s| {
            if s.len() > 50 {
                format!("{}...", &s[..47])
            } else {
                s.clone()
            }
        })
    }

    /// Get time since last activity as human-readable string
    pub fn time_since_activity(&self) -> Option<String> {
        self.last_activity.map(|t| {
            let now = Utc::now();
            let duration = now.signed_duration_since(t);

            if duration.num_seconds() < 60 {
                format!("{}s ago", duration.num_seconds())
            } else if duration.num_minutes() < 60 {
                format!("{}m ago", duration.num_minutes())
            } else if duration.num_hours() < 24 {
                format!("{}h ago", duration.num_hours())
            } else {
                format!("{}d ago", duration.num_days())
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_state_serialization() {
        let status = StatusState::Working;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"working\"");

        let parsed: StatusState = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, StatusState::Working);
    }

    #[test]
    fn test_session_status_parsing() {
        let json = r#"{
            "status": "working",
            "state_detail": "writing_code",
            "summary": "Implementing feature X",
            "current_task": "Writing login handler"
        }"#;

        let status: SessionStatus = serde_json::from_str(json).unwrap();
        assert_eq!(status.status, StatusState::Working);
        assert_eq!(status.state_detail, StatusDetail::WritingCode);
        assert_eq!(status.summary.as_deref(), Some("Implementing feature X"));
    }

    #[test]
    fn test_display_summary_truncation() {
        let status = SessionStatus {
            summary: Some("This is a very long summary that exceeds the fifty character limit for display".to_string()),
            ..Default::default()
        };

        let display = status.display_summary().unwrap();
        assert!(display.len() <= 50);
        assert!(display.ends_with("..."));
    }
}

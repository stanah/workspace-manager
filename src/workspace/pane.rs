//! Pane tracking for terminal multiplexer integration

use std::path::PathBuf;
use std::time::SystemTime;

use super::session::{AiTool, SessionStatus};

/// AI セッション情報（ペイン内で AI ツールが動作している場合）
#[derive(Debug, Clone)]
pub struct AiSessionInfo {
    pub tool: AiTool,
    pub status: SessionStatus,
    pub state_detail: Option<String>,
    pub summary: Option<String>,
    pub current_task: Option<String>,
    pub last_activity: Option<SystemTime>,
    pub external_id: Option<String>,
}

/// マルチプレクサのペイン（ワークスペースに紐付く）
#[derive(Debug, Clone)]
pub struct Pane {
    pub pane_id: String,
    pub workspace_index: usize,
    pub window_name: String,
    pub window_index: u32,
    pub cwd: PathBuf,
    pub command: String,
    pub is_active: bool,
    pub session_name: String,
    pub pid: u32,
    pub ai_session: Option<AiSessionInfo>,
}

impl Pane {
    pub fn detect_ai_tool(command: &str) -> Option<AiTool> {
        match command {
            "claude" => Some(AiTool::Claude),
            "kiro" => Some(AiTool::Kiro),
            "opencode" => Some(AiTool::OpenCode),
            "codex" => Some(AiTool::Codex),
            _ => None,
        }
    }

    pub fn is_ai_pane(&self) -> bool {
        self.ai_session.is_some()
    }

    pub fn ai_status(&self) -> Option<SessionStatus> {
        self.ai_session.as_ref().map(|s| s.status)
    }

    pub fn display_info(&self) -> String {
        if let Some(ref ai) = self.ai_session {
            let has_detail = ai.state_detail.is_some() || ai.summary.is_some();
            let mut parts = Vec::new();
            if !has_detail {
                // メッセージがない場合はツール名を表示
                parts.push(ai.tool.name().to_string());
            }
            if let Some(ref detail) = ai.state_detail {
                parts.push(format!("[{}]", detail));
            }
            if let Some(ref summary) = ai.summary {
                parts.push(summary.clone());
            }
            if let Some(ref activity) = ai.last_activity {
                if let Ok(duration) = activity.elapsed() {
                    let secs = duration.as_secs();
                    let time_str = if secs < 60 {
                        format!("{}s ago", secs)
                    } else if secs < 3600 {
                        format!("{}m ago", secs / 60)
                    } else if secs < 86400 {
                        format!("{}h ago", secs / 3600)
                    } else {
                        format!("{}d ago", secs / 86400)
                    };
                    parts.push(format!("({})", time_str));
                }
            }
            parts.join(" ")
        } else {
            self.command.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_ai_tool() {
        assert_eq!(Pane::detect_ai_tool("claude"), Some(AiTool::Claude));
        assert_eq!(Pane::detect_ai_tool("kiro"), Some(AiTool::Kiro));
        assert_eq!(Pane::detect_ai_tool("zsh"), None);
        assert_eq!(Pane::detect_ai_tool("vim"), None);
    }

    #[test]
    fn test_display_info_normal_pane() {
        let pane = Pane {
            pane_id: "%1".to_string(),
            workspace_index: 0,
            window_name: "main".to_string(),
            window_index: 0,
            cwd: PathBuf::from("/tmp"),
            command: "zsh".to_string(),
            is_active: true,
            session_name: "main".to_string(),
            pid: 1234,
            ai_session: None,
        };
        assert_eq!(pane.display_info(), "zsh");
        assert!(!pane.is_ai_pane());
    }

    #[test]
    fn test_display_info_ai_pane() {
        let pane = Pane {
            pane_id: "%2".to_string(),
            workspace_index: 0,
            window_name: "main".to_string(),
            window_index: 0,
            cwd: PathBuf::from("/tmp"),
            command: "claude".to_string(),
            is_active: false,
            session_name: "main".to_string(),
            pid: 5678,
            ai_session: Some(AiSessionInfo {
                tool: AiTool::Claude,
                status: SessionStatus::Working,
                state_detail: Some("processing".to_string()),
                summary: Some("Fix bug".to_string()),
                current_task: None,
                last_activity: None,
                external_id: None,
            }),
        };
        assert!(pane.is_ai_pane());
        assert_eq!(pane.display_info(), "[processing] Fix bug");
    }

    #[test]
    fn test_display_info_ai_pane_no_detail() {
        let pane = Pane {
            pane_id: "%3".to_string(),
            workspace_index: 0,
            window_name: "main".to_string(),
            window_index: 0,
            cwd: PathBuf::from("/tmp"),
            command: "claude".to_string(),
            is_active: false,
            session_name: "main".to_string(),
            pid: 9999,
            ai_session: Some(AiSessionInfo {
                tool: AiTool::Claude,
                status: SessionStatus::Idle,
                state_detail: None,
                summary: None,
                current_task: None,
                last_activity: None,
                external_id: None,
            }),
        };
        assert_eq!(pane.display_info(), "Claude");
    }
}

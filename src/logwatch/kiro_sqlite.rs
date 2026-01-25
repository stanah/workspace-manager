//! Kiro CLI status fetcher via SQLite database
//!
//! Reads Kiro CLI session status directly from its SQLite database instead of
//! parsing log files. This provides faster and more accurate status detection.

use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags};
use serde::Deserialize;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::{debug, warn};

use super::schema::{SessionStatus, StatusDetail, StatusState};

/// Kiro SQLite database path on macOS
const KIRO_DB_PATH_MACOS: &str = "Library/Application Support/kiro-cli/data.sqlite3";

/// Configuration for Kiro SQLite fetcher
#[derive(Debug, Clone)]
pub struct KiroSqliteConfig {
    /// Path to the Kiro SQLite database
    pub db_path: PathBuf,
    /// Connection timeout in seconds
    pub timeout_secs: u64,
}

impl Default for KiroSqliteConfig {
    fn default() -> Self {
        let db_path = dirs::home_dir()
            .map(|h| h.join(KIRO_DB_PATH_MACOS))
            .unwrap_or_else(|| PathBuf::from("/tmp/kiro-data.sqlite3"));

        Self {
            db_path,
            timeout_secs: 5,
        }
    }
}

/// Kiro CLI status from SQLite
#[derive(Debug, Clone)]
pub struct KiroStatus {
    /// Main status state
    pub state: StatusState,
    /// Detailed status
    pub state_detail: StatusDetail,
    /// Brief summary
    pub summary: Option<String>,
    /// Last update time
    pub updated_at: SystemTime,
}

impl KiroStatus {
    /// Convert to SessionStatus for unified handling
    pub fn to_session_status(&self, project_path: &str) -> SessionStatus {
        // Convert SystemTime to chrono::DateTime
        let last_activity = self.updated_at
            .duration_since(UNIX_EPOCH)
            .ok()
            .map(|d| chrono::DateTime::from_timestamp(d.as_secs() as i64, d.subsec_nanos()))
            .flatten();

        SessionStatus {
            project_path: Some(project_path.to_string()),
            tool: Some("kiro".to_string()),
            status: self.state,
            state_detail: self.state_detail.clone(),
            summary: self.summary.clone(),
            last_activity,
            ..Default::default()
        }
    }
}

/// Kiro conversation structure (from SQLite JSON)
#[derive(Debug, Deserialize)]
struct ConversationValue {
    history: Option<Vec<HistoryEntry>>,
}

/// History entry in Kiro conversation
/// Each entry has a user request and assistant response
#[derive(Debug, Deserialize)]
struct HistoryEntry {
    user: Option<UserEntry>,
    assistant: Option<serde_json::Value>,
}

/// User entry in Kiro history
#[derive(Debug, Deserialize)]
struct UserEntry {
    content: Option<UserContent>,
}

/// User content types
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum UserContent {
    Structured(UserContentStructured),
    Other(serde_json::Value),
}

/// Structured user content
#[derive(Debug, Deserialize)]
struct UserContentStructured {
    #[serde(rename = "Prompt")]
    prompt: Option<PromptContent>,
    #[serde(rename = "ToolUseResults")]
    tool_use_results: Option<ToolUseResultsContent>,
}

#[derive(Debug, Deserialize)]
struct PromptContent {
    prompt: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ToolUseResultsContent {
    tool_use_results: Option<Vec<serde_json::Value>>,
}

/// Fetches Kiro CLI status from SQLite database
pub struct KiroSqliteFetcher {
    config: KiroSqliteConfig,
}

impl KiroSqliteFetcher {
    /// Create a new fetcher with default configuration
    pub fn new() -> Self {
        Self {
            config: KiroSqliteConfig::default(),
        }
    }

    /// Create a new fetcher with custom configuration
    pub fn with_config(config: KiroSqliteConfig) -> Self {
        Self { config }
    }

    /// Check if the Kiro database exists and is accessible
    pub fn is_available(&self) -> bool {
        self.config.db_path.exists()
    }

    /// Get the database path
    pub fn db_path(&self) -> &PathBuf {
        &self.config.db_path
    }

    /// Open a read-only connection to the database
    fn open_connection(&self) -> Result<Connection> {
        let conn = Connection::open_with_flags(
            &self.config.db_path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .context("Failed to open Kiro database")?;

        conn.busy_timeout(std::time::Duration::from_secs(self.config.timeout_secs))?;

        Ok(conn)
    }

    /// Check if a Kiro CLI process is running for the given workspace
    fn is_kiro_running(&self, workspace_path: &str) -> bool {
        // Find kiro processes, check they're running (not stopped), and get their cwd
        // Process states: R=running, S=sleeping (normal), T=stopped/suspended
        let script = r#"
            for pid in $(pgrep -f 'kiro-cli-chat|kiro-cli$' 2>/dev/null); do
                state=$(ps -p $pid -o state= 2>/dev/null | tr -d ' ')
                # Only check processes that are running or sleeping (not T=stopped)
                if [ "$state" != "T" ] && [ -n "$state" ]; then
                    lsof -p $pid 2>/dev/null | grep cwd | awk '{print $NF}'
                fi
            done
        "#;

        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg(script)
            .output();

        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                for line in stdout.lines() {
                    let cwd = line.trim();
                    if cwd == workspace_path {
                        return true;
                    }
                }
                false
            }
            Err(_) => false,
        }
    }

    /// Get status for a specific workspace path
    pub fn get_status(&self, workspace_path: &str) -> Result<Option<KiroStatus>> {
        if !self.is_available() {
            debug!("Kiro database not available at {:?}", self.config.db_path);
            return Ok(None);
        }

        // First check if Kiro is actually running for this workspace
        if !self.is_kiro_running(workspace_path) {
            debug!("No Kiro process running for: {}", workspace_path);
            // Return "stopped" status (green) instead of None
            return Ok(Some(KiroStatus {
                state: StatusState::Completed,
                state_detail: StatusDetail::SessionEnded,
                summary: Some("Stopped".to_string()),
                updated_at: SystemTime::now(),
            }));
        }

        let conn = self.open_connection()?;
        self.get_status_with_conn(&conn, workspace_path)
    }

    /// Get statuses for multiple workspaces
    pub fn get_statuses(&self, workspaces: &[String]) -> Vec<(String, KiroStatus)> {
        if !self.is_available() {
            return Vec::new();
        }

        let conn = match self.open_connection() {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to open Kiro database: {}", e);
                return Vec::new();
            }
        };

        let mut results = Vec::new();

        for workspace in workspaces {
            match self.get_status_with_conn(&conn, workspace) {
                Ok(Some(status)) => {
                    results.push((workspace.clone(), status));
                }
                Ok(None) => {
                    // No status for this workspace
                }
                Err(e) => {
                    debug!("Failed to get status for {}: {}", workspace, e);
                }
            }
        }

        results
    }

    /// Get status using an existing connection
    fn get_status_with_conn(&self, conn: &Connection, workspace_path: &str) -> Result<Option<KiroStatus>> {
        // Query the conversations_v2 table
        // updated_at is INTEGER (Unix timestamp in milliseconds)
        let mut stmt = conn.prepare_cached(
            "SELECT value, updated_at FROM conversations_v2 WHERE key = ? ORDER BY updated_at DESC LIMIT 1"
        )?;

        let result: Option<(String, i64)> = stmt
            .query_row([workspace_path], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .ok();

        match result {
            Some((value, updated_at_ms)) => {
                let status = self.parse_conversation_value(&value)?;
                let updated_at = UNIX_EPOCH + Duration::from_millis(updated_at_ms as u64);

                Ok(Some(KiroStatus {
                    state: status.0,
                    state_detail: status.1,
                    summary: status.2,
                    updated_at,
                }))
            }
            None => {
                debug!("No conversation found for workspace: {}", workspace_path);
                Ok(None)
            }
        }
    }

    /// Parse conversation value JSON and determine status
    fn parse_conversation_value(&self, value: &str) -> Result<(StatusState, StatusDetail, Option<String>)> {
        let conv: ConversationValue = serde_json::from_str(value)
            .context("Failed to parse conversation JSON")?;

        let history = match conv.history {
            Some(h) if !h.is_empty() => h,
            _ => {
                return Ok((StatusState::Idle, StatusDetail::Inactive, None));
            }
        };

        // Get the last entry to determine current state
        let last_entry = &history[history.len() - 1];

        self.determine_status_from_entry(last_entry)
    }

    /// Determine status from the last history entry
    ///
    /// Kiro conversation pattern:
    /// - User: Prompt, Assistant: ToolUse -> Waiting for y/n confirmation (yellow)
    /// - User: ToolUseResults, Assistant: Response -> Completed, waiting for next prompt (green)
    /// - User: Prompt, Assistant: Response -> Completed, waiting for next prompt (green)
    /// - User: *, Assistant: None -> Working (processing) (blue)
    fn determine_status_from_entry(&self, entry: &HistoryEntry) -> Result<(StatusState, StatusDetail, Option<String>)> {
        // First, check the assistant response type - this is the primary indicator
        if let Some(assistant) = &entry.assistant {
            if let Some(obj) = assistant.as_object() {
                // ToolUse -> Waiting for y/n confirmation (yellow)
                if let Some(tool_use) = obj.get("ToolUse") {
                    let summary = self.extract_tool_use_summary(tool_use);
                    return Ok((
                        StatusState::Waiting,
                        StatusDetail::Confirmation,
                        Some(summary),
                    ));
                }

                // Response -> Completed, ready for next prompt (green)
                if let Some(response) = obj.get("Response") {
                    let summary = self.extract_response_summary(response);
                    return Ok((
                        StatusState::Completed,
                        StatusDetail::Success,
                        Some(summary),
                    ));
                }
            }
        }

        // No assistant response yet -> Working (processing) (blue)
        // Check what the user sent to provide context
        if let Some(user) = &entry.user {
            if let Some(content) = &user.content {
                match content {
                    UserContent::Structured(s) => {
                        if s.tool_use_results.is_some() {
                            // Processing tool results
                            return Ok((
                                StatusState::Working,
                                StatusDetail::ExecutingTool,
                                Some("Running tools...".to_string()),
                            ));
                        }
                        if let Some(prompt) = &s.prompt {
                            // Processing user prompt
                            let summary = prompt.prompt.as_ref()
                                .map(|p| truncate_str(p, 30))
                                .unwrap_or_else(|| "Thinking...".to_string());
                            return Ok((
                                StatusState::Working,
                                StatusDetail::Thinking,
                                Some(summary),
                            ));
                        }
                    }
                    UserContent::Other(_) => {}
                }
            }
        }

        // Fallback
        Ok((StatusState::Working, StatusDetail::Thinking, Some("Processing...".to_string())))
    }

    /// Extract summary from ToolUse (shows the message and tool names)
    fn extract_tool_use_summary(&self, tool_use: &serde_json::Value) -> String {
        // Try to get the content message first
        if let Some(content) = tool_use.get("content").and_then(|c| c.as_str()) {
            if !content.is_empty() {
                return truncate_str(content, 40);
            }
        }

        // Fallback to tool names
        let mut names = Vec::new();
        if let Some(tool_uses) = tool_use.get("tool_uses").and_then(|t| t.as_array()) {
            for tool in tool_uses.iter().take(3) {
                if let Some(name) = tool.get("name").and_then(|n| n.as_str()) {
                    names.push(name.to_string());
                }
            }
            if tool_uses.len() > 3 {
                names.push(format!("+{}", tool_uses.len() - 3));
            }
        }

        if names.is_empty() {
            "Confirm?".to_string()
        } else {
            format!("Confirm: {}", names.join(", "))
        }
    }

    /// Extract summary from Response (shows truncated response content)
    fn extract_response_summary(&self, response: &serde_json::Value) -> String {
        if let Some(content) = response.get("content").and_then(|c| c.as_str()) {
            if !content.is_empty() {
                // Get first line and truncate
                let first_line = content.lines().next().unwrap_or(content);
                return truncate_str(first_line, 40);
            }
        }
        "Done".to_string()
    }
}

impl Default for KiroSqliteFetcher {
    fn default() -> Self {
        Self::new()
    }
}

/// Truncate string to max characters (not bytes) with ellipsis
fn truncate_str(s: &str, max_chars: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars.saturating_sub(3)).collect();
        format!("{}...", truncated)
    }
}

/// Helper module for home directory
mod dirs {
    pub fn home_dir() -> Option<std::path::PathBuf> {
        std::env::var_os("HOME").map(std::path::PathBuf::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = KiroSqliteConfig::default();
        assert!(config.db_path.to_string_lossy().contains("kiro-cli"));
    }

    #[test]
    fn test_fetcher_creation() {
        let fetcher = KiroSqliteFetcher::new();
        assert!(!fetcher.db_path().as_os_str().is_empty());
    }

    #[test]
    fn test_truncate_str() {
        assert_eq!(truncate_str("hello", 10), "hello");
        assert_eq!(truncate_str("hello world", 8), "hello...");
    }

    #[test]
    fn test_kiro_status_to_session_status() {
        let kiro_status = KiroStatus {
            state: StatusState::Working,
            state_detail: StatusDetail::Thinking,
            summary: Some("Test".to_string()),
            updated_at: SystemTime::now(),
        };

        let session_status = kiro_status.to_session_status("/path/to/project");
        assert_eq!(session_status.status, StatusState::Working);
        assert_eq!(session_status.tool, Some("kiro".to_string()));
        assert_eq!(session_status.project_path, Some("/path/to/project".to_string()));
    }

    #[test]
    fn test_determine_status_with_assistant_response() {
        let fetcher = KiroSqliteFetcher::new();

        // Entry with assistant Response -> Completed (green)
        let entry = HistoryEntry {
            user: Some(UserEntry {
                content: Some(UserContent::Structured(UserContentStructured {
                    prompt: Some(PromptContent { prompt: Some("test".to_string()) }),
                    tool_use_results: None,
                })),
            }),
            assistant: Some(serde_json::json!({"Response": {"content": "Task completed successfully"}})),
        };

        let (state, detail, summary) = fetcher.determine_status_from_entry(&entry).unwrap();
        assert_eq!(state, StatusState::Completed);
        assert_eq!(detail, StatusDetail::Success);
        assert!(summary.unwrap().contains("Task completed"));
    }

    #[test]
    fn test_determine_status_with_tool_use() {
        let fetcher = KiroSqliteFetcher::new();

        // Entry with assistant ToolUse -> Waiting for confirmation (y/n)
        let entry = HistoryEntry {
            user: Some(UserEntry {
                content: Some(UserContent::Structured(UserContentStructured {
                    prompt: Some(PromptContent { prompt: Some("test".to_string()) }),
                    tool_use_results: None,
                })),
            }),
            assistant: Some(serde_json::json!({
                "ToolUse": {
                    "tool_uses": [
                        {"name": "fs_read"},
                        {"name": "execute_bash"}
                    ]
                }
            })),
        };

        let (state, detail, summary) = fetcher.determine_status_from_entry(&entry).unwrap();
        assert_eq!(state, StatusState::Waiting);
        assert_eq!(detail, StatusDetail::Confirmation);
        assert!(summary.unwrap().contains("fs_read"));
    }

    #[test]
    fn test_determine_status_processing_prompt() {
        let fetcher = KiroSqliteFetcher::new();

        // Entry without assistant response -> Working
        let entry = HistoryEntry {
            user: Some(UserEntry {
                content: Some(UserContent::Structured(UserContentStructured {
                    prompt: Some(PromptContent { prompt: Some("test prompt".to_string()) }),
                    tool_use_results: None,
                })),
            }),
            assistant: None,
        };

        let (state, detail, _) = fetcher.determine_status_from_entry(&entry).unwrap();
        assert_eq!(state, StatusState::Working);
        assert_eq!(detail, StatusDetail::Thinking);
    }
}

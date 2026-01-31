//! Claude Code session fetcher via sessions-index.json
//!
//! Reads Claude Code session information from ~/.claude/projects/*/sessions-index.json
//! and provides session status based on file modification times.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};
use tracing::{debug, warn};

use crate::workspace::claude_external_id;

/// Default inactivity threshold in seconds
const DEFAULT_INACTIVITY_THRESHOLD_SECS: u64 = 60;

/// Configuration for Claude sessions fetcher
#[derive(Debug, Clone)]
pub struct ClaudeSessionsConfig {
    /// Path to the .claude directory
    pub claude_dir: PathBuf,
    /// Inactivity threshold in seconds (sessions modified after this are considered active)
    pub inactivity_threshold_secs: u64,
}

impl Default for ClaudeSessionsConfig {
    fn default() -> Self {
        let claude_dir = directories::BaseDirs::new()
            .map(|d| d.home_dir().join(".claude"))
            .unwrap_or_else(|| PathBuf::from("~/.claude"));

        Self {
            claude_dir,
            inactivity_threshold_secs: DEFAULT_INACTIVITY_THRESHOLD_SECS,
        }
    }
}

/// Sessions index file structure
#[derive(Debug, Deserialize)]
struct SessionsIndex {
    /// Version number
    #[allow(dead_code)]
    version: u32,
    /// Session entries
    entries: Vec<SessionEntry>,
    /// Original project path
    #[serde(rename = "originalPath")]
    original_path: String,
}

/// Individual session entry in the index
#[derive(Debug, Deserialize)]
struct SessionEntry {
    /// Session ID (UUID)
    #[serde(rename = "sessionId")]
    session_id: String,
    /// Full path to the JSONL file
    #[serde(rename = "fullPath")]
    #[allow(dead_code)]
    full_path: String,
    /// File modification time (Unix timestamp in milliseconds) - may be stale
    #[serde(rename = "fileMtime")]
    file_mtime: i64,
    /// First user prompt (truncated)
    #[serde(rename = "firstPrompt")]
    #[allow(dead_code)]
    first_prompt: Option<String>,
    /// Session summary
    summary: Option<String>,
    /// Message count
    #[serde(rename = "messageCount")]
    message_count: u32,
    /// Created timestamp (ISO 8601)
    created: String,
    /// Modified timestamp (ISO 8601)
    modified: String,
    /// Git branch name
    #[serde(rename = "gitBranch")]
    git_branch: Option<String>,
    /// Project path
    #[serde(rename = "projectPath")]
    project_path: String,
    /// Is sidechain session
    #[serde(rename = "isSidechain")]
    #[allow(dead_code)]
    is_sidechain: bool,
}

/// Claude Code session information
#[derive(Debug, Clone)]
pub struct ClaudeSession {
    /// Session ID (UUID)
    pub session_id: String,
    /// External ID (claude:{session_id})
    pub external_id: String,
    /// Project path
    pub project_path: String,
    /// Session summary
    pub summary: Option<String>,
    /// Message count
    pub message_count: u32,
    /// Created timestamp
    pub created: DateTime<Utc>,
    /// Modified timestamp
    pub modified: DateTime<Utc>,
    /// Git branch name
    pub git_branch: Option<String>,
    /// Is session currently active (modified within threshold)
    pub is_active: bool,
}

impl ClaudeSession {
    /// Convert to SessionStatus for unified handling
    pub fn to_session_status(&self) -> super::SessionStatus {
        let status = if self.is_active {
            // If modified recently, assume working
            super::StatusState::Working
        } else {
            super::StatusState::Idle
        };

        let state_detail = if self.is_active {
            super::StatusDetail::Thinking
        } else {
            super::StatusDetail::Inactive
        };

        let modified_chrono = self.modified;

        super::SessionStatus {
            session_id: Some(self.session_id.clone()),
            project_path: Some(self.project_path.clone()),
            tool: Some("claude".to_string()),
            status,
            state_detail,
            summary: self.summary.clone(),
            current_task: None,
            last_activity: Some(modified_chrono),
            progress: None,
            error: None,
            context: None,
        }
    }
}

/// Running Claude process info
#[derive(Debug, Clone)]
pub struct ClaudeProcessInfo {
    pub cwd: String,
    pub session_id: Option<String>,
}

/// Fetches Claude Code sessions from sessions-index.json files
pub struct ClaudeSessionsFetcher {
    config: ClaudeSessionsConfig,
}

impl ClaudeSessionsFetcher {
    /// Create a new fetcher with default configuration
    pub fn new() -> Self {
        Self {
            config: ClaudeSessionsConfig::default(),
        }
    }

    /// Create a new fetcher with custom configuration
    pub fn with_config(config: ClaudeSessionsConfig) -> Self {
        Self { config }
    }

    /// Check if the Claude directory exists and is accessible
    pub fn is_available(&self) -> bool {
        self.config.claude_dir.join("projects").exists()
    }

    /// Get the Claude directory path
    pub fn claude_dir(&self) -> &PathBuf {
        &self.config.claude_dir
    }

    /// Get all running Claude processes with their session IDs
    /// Returns a list of (cwd, session_id) pairs
    pub fn get_running_processes(&self) -> Vec<ClaudeProcessInfo> {
        let mut processes = Vec::new();

        // Find claude processes, get their cwd and --resume argument
        // Only include processes with a TTY (not background/subprocess with tty=??)
        let script = r#"
            for pid in $(pgrep -x 'claude' 2>/dev/null); do
                tty=$(ps -p $pid -o tty= 2>/dev/null | tr -d ' ')
                # Skip background processes (tty is ?? or empty)
                if [ "$tty" = "??" ] || [ -z "$tty" ]; then
                    continue
                fi
                state=$(ps -p $pid -o state= 2>/dev/null | tr -d ' ')
                if [ "$state" != "T" ] && [ -n "$state" ]; then
                    cwd=$(lsof -p $pid 2>/dev/null | grep cwd | awk '{print $NF}')
                    args=$(ps -p $pid -o args= 2>/dev/null)
                    # Extract session ID from --resume argument
                    session_id=$(echo "$args" | grep -oE '\-\-resume [a-f0-9-]+' | awk '{print $2}')
                    if [ -n "$cwd" ]; then
                        echo "${cwd}|${session_id}"
                    fi
                fi
            done
        "#;

        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg(script)
            .output();

        match output {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    let line = line.trim();
                    if line.is_empty() {
                        continue;
                    }
                    let parts: Vec<&str> = line.splitn(2, '|').collect();
                    let cwd = normalize_path(parts.first().unwrap_or(&""));
                    let session_id = parts.get(1).and_then(|s| {
                        let s = s.trim();
                        if s.is_empty() { None } else { Some(s.to_string()) }
                    });
                    if !cwd.is_empty() {
                        processes.push(ClaudeProcessInfo { cwd, session_id });
                    }
                }
            }
            Err(e) => {
                debug!("Failed to check Claude processes: {}", e);
            }
        }

        processes
    }

    /// Get all workspaces where Claude Code is currently running
    /// Returns a map of workspace path -> process count
    pub fn get_running_workspaces(&self) -> std::collections::HashMap<String, usize> {
        let processes = self.get_running_processes();
        let mut running: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for proc in processes {
            *running.entry(proc.cwd).or_insert(0) += 1;
        }
        running
    }

    /// Get session IDs for running processes in a workspace
    pub fn get_running_session_ids(&self, workspace_path: &str) -> Vec<String> {
        let processes = self.get_running_processes();
        let normalized = normalize_path(workspace_path);
        processes.into_iter()
            .filter(|p| p.cwd == normalized)
            .filter_map(|p| p.session_id)
            .collect()
    }

    /// Check if Claude Code is running for a specific workspace
    pub fn is_claude_running(&self, workspace_path: &str) -> bool {
        let running = self.get_running_workspaces();
        let normalized = normalize_path(workspace_path);
        running.contains_key(&normalized)
    }

    /// Get process count for a specific workspace
    pub fn get_process_count(&self, workspace_path: &str) -> usize {
        let running = self.get_running_workspaces();
        let normalized = normalize_path(workspace_path);
        running.get(&normalized).copied().unwrap_or(0)
    }

    /// Get sessions for specific workspace paths
    ///
    /// Returns a map of project_path -> Vec<ClaudeSession>
    pub fn get_sessions(&self, workspace_paths: &[String]) -> HashMap<String, Vec<ClaudeSession>> {
        if !self.is_available() {
            debug!("Claude projects directory not available");
            return HashMap::new();
        }

        let projects_dir = self.config.claude_dir.join("projects");
        let mut results: HashMap<String, Vec<ClaudeSession>> = HashMap::new();
        let now = SystemTime::now();

        // Read all sessions-index.json files
        match std::fs::read_dir(&projects_dir) {
            Ok(entries) => {
                for entry in entries.filter_map(|e| e.ok()) {
                    let path = entry.path();
                    if !path.is_dir() {
                        continue;
                    }

                    let index_path = path.join("sessions-index.json");
                    if !index_path.exists() {
                        continue;
                    }

                    match self.read_sessions_index(&index_path) {
                        Ok(index) => {
                            // Check if this project matches any workspace path
                            let normalized_project_path = normalize_path(&index.original_path);
                            let is_tracked = workspace_paths.iter().any(|wp| {
                                normalize_path(wp) == normalized_project_path
                            });

                            if !is_tracked {
                                continue;
                            }

                            // Convert entries to ClaudeSession
                            // Only include active sessions (modified within inactivity threshold)
                            let mut sessions: Vec<ClaudeSession> = index.entries
                                .iter()
                                .filter_map(|entry| self.entry_to_session(entry, now))
                                .filter(|s| s.is_active)
                                .collect();
                            sessions.sort_by(|a, b| b.modified.cmp(&a.modified));

                            if !sessions.is_empty() {
                                results.insert(index.original_path.clone(), sessions);
                            }
                        }
                        Err(e) => {
                            debug!("Failed to read sessions index at {:?}: {}", index_path, e);
                        }
                    }
                }
            }
            Err(e) => {
                warn!("Failed to read Claude projects directory: {}", e);
            }
        }

        results
    }

    /// Get all sessions (no filtering by workspace paths)
    pub fn get_all_sessions(&self) -> Vec<ClaudeSession> {
        if !self.is_available() {
            return Vec::new();
        }

        let projects_dir = self.config.claude_dir.join("projects");
        let mut results = Vec::new();
        let now = SystemTime::now();

        if let Ok(entries) = std::fs::read_dir(&projects_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }

                let index_path = path.join("sessions-index.json");
                if !index_path.exists() {
                    continue;
                }

                if let Ok(index) = self.read_sessions_index(&index_path) {
                    for entry in index.entries {
                        if let Some(session) = self.entry_to_session(&entry, now) {
                            results.push(session);
                        }
                    }
                }
            }
        }

        results
    }

    /// Read and parse a sessions-index.json file
    fn read_sessions_index(&self, path: &PathBuf) -> Result<SessionsIndex> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read {:?}", path))?;
        let index: SessionsIndex = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse {:?}", path))?;
        Ok(index)
    }

    /// Convert a SessionEntry to ClaudeSession
    fn entry_to_session(&self, entry: &SessionEntry, now: SystemTime) -> Option<ClaudeSession> {
        // Parse timestamps
        let created = DateTime::parse_from_rfc3339(&entry.created)
            .ok()?
            .with_timezone(&Utc);
        let modified = DateTime::parse_from_rfc3339(&entry.modified)
            .ok()?
            .with_timezone(&Utc);

        // Check if session is active (modified within threshold)
        let modified_system_time = std::time::UNIX_EPOCH
            + Duration::from_millis(entry.file_mtime as u64);
        let is_active = now
            .duration_since(modified_system_time)
            .map(|d| d.as_secs() < self.config.inactivity_threshold_secs)
            .unwrap_or(false);

        let external_id = claude_external_id(&entry.session_id);

        Some(ClaudeSession {
            session_id: entry.session_id.clone(),
            external_id,
            project_path: entry.project_path.clone(),
            summary: entry.summary.clone(),
            message_count: entry.message_count,
            created,
            modified,
            git_branch: entry.git_branch.clone(),
            is_active,
        })
    }
}

impl Default for ClaudeSessionsFetcher {
    fn default() -> Self {
        Self::new()
    }
}

/// Normalize a path by expanding ~ to home directory
fn normalize_path(path: &str) -> String {
    if path.starts_with("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return format!("{}{}", home.to_string_lossy(), &path[1..]);
        }
    }
    path.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = ClaudeSessionsConfig::default();
        assert!(config.claude_dir.to_string_lossy().contains(".claude"));
        assert_eq!(config.inactivity_threshold_secs, 60);
    }

    #[test]
    fn test_fetcher_creation() {
        let fetcher = ClaudeSessionsFetcher::new();
        assert!(!fetcher.claude_dir().as_os_str().is_empty());
    }

    #[test]
    fn test_normalize_path() {
        let path = normalize_path("/Users/test/project");
        assert_eq!(path, "/Users/test/project");

        // ~ expansion depends on HOME env var
        std::env::set_var("HOME", "/home/testuser");
        let path = normalize_path("~/project");
        assert_eq!(path, "/home/testuser/project");
    }

    #[test]
    fn test_session_entry_parsing() {
        let json = r#"{
            "sessionId": "test-123",
            "fullPath": "/path/to/session.jsonl",
            "fileMtime": 1769336818852,
            "firstPrompt": "test prompt",
            "summary": "Test summary",
            "messageCount": 10,
            "created": "2026-01-25T07:37:30.698Z",
            "modified": "2026-01-25T07:49:46.343Z",
            "gitBranch": "main",
            "projectPath": "/path/to/project",
            "isSidechain": false
        }"#;

        let entry: SessionEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.session_id, "test-123");
        assert_eq!(entry.message_count, 10);
        assert_eq!(entry.project_path, "/path/to/project");
    }

    #[test]
    fn test_sessions_index_parsing() {
        let json = r#"{
            "version": 1,
            "entries": [
                {
                    "sessionId": "test-123",
                    "fullPath": "/path/to/session.jsonl",
                    "fileMtime": 1769336818852,
                    "summary": "Test summary",
                    "messageCount": 10,
                    "created": "2026-01-25T07:37:30.698Z",
                    "modified": "2026-01-25T07:49:46.343Z",
                    "projectPath": "/path/to/project",
                    "isSidechain": false
                }
            ],
            "originalPath": "/path/to/project"
        }"#;

        let index: SessionsIndex = serde_json::from_str(json).unwrap();
        assert_eq!(index.version, 1);
        assert_eq!(index.entries.len(), 1);
        assert_eq!(index.original_path, "/path/to/project");
    }
}

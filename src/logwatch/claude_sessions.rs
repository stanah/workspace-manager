//! Claude Code session fetcher via sessions-index.json
//!
//! Reads Claude Code session information from ~/.claude/projects/*/sessions-index.json
//! and provides session status based on file modification times.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};
use tracing::debug;

use crate::workspace::claude_external_id;
use super::collector::encode_project_path;

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
    #[allow(dead_code)]
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
    pub pid: u32,
    pub cwd: String,
    pub session_id: Option<String>,
    pub ppid: Option<u32>,
}

/// Filter out subagent processes (child claude processes spawned by parent claude processes).
/// A process is considered a subagent if its parent PID (ppid) belongs to another claude process,
/// or if any ancestor up to `max_depth` levels is a claude process.
pub fn filter_subagents(processes: Vec<ClaudeProcessInfo>) -> Vec<ClaudeProcessInfo> {
    let claude_pids: HashSet<u32> = processes.iter().map(|p| p.pid).collect();

    // Build pid -> ppid map for ancestor checking
    let pid_to_ppid: HashMap<u32, u32> = processes
        .iter()
        .filter_map(|p| p.ppid.map(|ppid| (p.pid, ppid)))
        .collect();

    let max_depth = 3;

    processes
        .into_iter()
        .filter(|p| {
            // Check if any ancestor (up to max_depth) is a claude process
            let mut current_ppid = p.ppid;
            for _ in 0..max_depth {
                match current_ppid {
                    Some(ppid) if claude_pids.contains(&ppid) => return false,
                    Some(ppid) => {
                        // Walk up the process tree
                        current_ppid = pid_to_ppid.get(&ppid).copied();
                    }
                    None => break,
                }
            }
            true
        })
        .collect()
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
    /// Returns a list of ClaudeProcessInfo with pid, cwd, session_id, and ppid.
    /// Subagent processes (child claude processes) are filtered out.
    pub fn get_running_processes(&self) -> Vec<ClaudeProcessInfo> {
        let raw = self.get_running_processes_raw();
        filter_subagents(raw)
    }

    /// Get raw running Claude processes without subagent filtering
    fn get_running_processes_raw(&self) -> Vec<ClaudeProcessInfo> {
        let mut processes = Vec::new();

        // Find claude processes, get their pid, cwd, --resume argument, and ppid
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
                    ppid=$(ps -p $pid -o ppid= 2>/dev/null | tr -d ' ')
                    if [ -n "$cwd" ]; then
                        echo "${pid}|${cwd}|${session_id}|${ppid}"
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
                    let parts: Vec<&str> = line.splitn(4, '|').collect();
                    let pid = parts.first().and_then(|s| s.parse::<u32>().ok());
                    let cwd = normalize_path(parts.get(1).unwrap_or(&""));
                    let session_id = parts.get(2).and_then(|s| {
                        let s = s.trim();
                        if s.is_empty() { None } else { Some(s.to_string()) }
                    });
                    let ppid = parts.get(3).and_then(|s| {
                        let s = s.trim();
                        if s.is_empty() { None } else { s.parse::<u32>().ok() }
                    });
                    if let Some(pid) = pid {
                        if !cwd.is_empty() {
                            processes.push(ClaudeProcessInfo { pid, cwd, session_id, ppid });
                        }
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
    /// Uses a hybrid approach:
    /// 1. Reads sessions-index.json for metadata (summary, branch, etc.)
    /// 2. Scans JSONL files directly for recent activity (sessions-index.json may be stale)
    /// 3. Merges both sources, preferring JSONL file scan for activity detection
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

        for workspace_path in workspace_paths {
            let normalized_wp = normalize_path(workspace_path);
            let encoded = encode_project_path(&normalized_wp);
            let project_dir = projects_dir.join(&encoded);

            if !project_dir.is_dir() {
                continue;
            }

            // Build session metadata from sessions-index.json (if available)
            let mut index_sessions: HashMap<String, ClaudeSession> = HashMap::new();
            let index_path = project_dir.join("sessions-index.json");
            if index_path.exists() {
                if let Ok(index) = self.read_sessions_index(&index_path) {
                    for entry in &index.entries {
                        if let Some(session) = self.entry_to_session(entry, now) {
                            index_sessions.insert(session.session_id.clone(), session);
                        }
                    }
                }
            }

            // Scan JSONL files directly for recent sessions
            // This catches sessions not yet in sessions-index.json
            let mut sessions: Vec<ClaudeSession> = Vec::new();
            let mut seen_ids: HashSet<String> = HashSet::new();

            if let Ok(dir_entries) = std::fs::read_dir(&project_dir) {
                for entry in dir_entries.filter_map(|e| e.ok()) {
                    let file_path = entry.path();
                    // Only root-level .jsonl files (not subagent files in subdirectories)
                    if !file_path.is_file() {
                        continue;
                    }
                    let file_name = match file_path.file_name().and_then(|n| n.to_str()) {
                        Some(name) if name.ends_with(".jsonl") && name != "sessions-index.json" => name,
                        _ => continue,
                    };

                    // Extract session ID from filename (UUID.jsonl)
                    let session_id = match file_name.strip_suffix(".jsonl") {
                        Some(id) if id.len() >= 36 => id.to_string(),
                        _ => continue,
                    };

                    if seen_ids.contains(&session_id) {
                        continue;
                    }
                    seen_ids.insert(session_id.clone());

                    // Check actual file modification time
                    let file_mtime = match std::fs::metadata(&file_path).and_then(|m| m.modified()) {
                        Ok(mtime) => mtime,
                        Err(_) => continue,
                    };

                    let is_active = now
                        .duration_since(file_mtime)
                        .map(|d| d.as_secs() < self.config.inactivity_threshold_secs)
                        .unwrap_or(false);

                    // Use metadata from index if available, otherwise create minimal entry
                    if let Some(mut indexed) = index_sessions.remove(&session_id) {
                        // Update is_active based on actual file mtime (more reliable)
                        indexed.is_active = is_active;
                        sessions.push(indexed);
                    } else {
                        // Session not in index - create minimal entry from file info
                        let modified_chrono = chrono::DateTime::<Utc>::from(file_mtime);
                        let external_id = crate::workspace::claude_external_id(&session_id);
                        sessions.push(ClaudeSession {
                            session_id,
                            external_id,
                            project_path: normalized_wp.clone(),
                            summary: None,
                            message_count: 0,
                            created: modified_chrono,
                            modified: modified_chrono,
                            git_branch: None,
                            is_active,
                        });
                    }
                }
            }

            // Sort by modified time (newest first)
            sessions.sort_by(|a, b| b.modified.cmp(&a.modified));

            if !sessions.is_empty() {
                results.insert(normalized_wp, sessions);
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

    fn make_process(pid: u32, cwd: &str, session_id: Option<&str>, ppid: Option<u32>) -> ClaudeProcessInfo {
        ClaudeProcessInfo {
            pid,
            cwd: cwd.to_string(),
            session_id: session_id.map(|s| s.to_string()),
            ppid,
        }
    }

    #[test]
    fn test_filter_subagents_removes_child_processes() {
        let processes = vec![
            make_process(100, "/work/project", Some("sess-a"), Some(1)),   // parent claude
            make_process(200, "/work/project", Some("sess-a"), Some(100)), // subagent (child of 100)
            make_process(300, "/work/project", Some("sess-a"), Some(200)), // nested subagent (child of 200)
        ];
        let filtered = filter_subagents(processes);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].pid, 100);
    }

    #[test]
    fn test_filter_subagents_keeps_independent_processes() {
        let processes = vec![
            make_process(100, "/work/project-a", Some("sess-a"), Some(1)), // independent
            make_process(200, "/work/project-b", Some("sess-b"), Some(1)), // independent
        ];
        let filtered = filter_subagents(processes);
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_filter_subagents_mixed_scenario() {
        let processes = vec![
            make_process(100, "/work/proj", Some("sess-a"), Some(1)),     // parent 1
            make_process(200, "/work/proj", Some("sess-a"), Some(100)),   // subagent of 100
            make_process(300, "/work/proj", Some("sess-b"), Some(1)),     // parent 2 (different session)
            make_process(400, "/work/proj", None, Some(300)),             // subagent of 300
        ];
        let filtered = filter_subagents(processes);
        assert_eq!(filtered.len(), 2);
        let pids: Vec<u32> = filtered.iter().map(|p| p.pid).collect();
        assert!(pids.contains(&100));
        assert!(pids.contains(&300));
    }

    #[test]
    fn test_filter_subagents_no_ppid() {
        let processes = vec![
            make_process(100, "/work/proj", Some("sess-a"), None), // no ppid info
            make_process(200, "/work/proj", Some("sess-b"), None), // no ppid info
        ];
        let filtered = filter_subagents(processes);
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_filter_subagents_deep_nesting() {
        // Ancestor check goes up to 3 levels
        let processes = vec![
            make_process(100, "/work/proj", Some("sess-a"), Some(1)),   // root claude
            make_process(200, "/work/proj", None, Some(100)),            // depth 1
            make_process(300, "/work/proj", None, Some(200)),            // depth 2
            make_process(400, "/work/proj", None, Some(300)),            // depth 3
        ];
        let filtered = filter_subagents(processes);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].pid, 100);
    }
}

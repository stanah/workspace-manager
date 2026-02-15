//! Claude Code session fetcher via sessions-index.json
//!
//! Reads Claude Code session information from ~/.claude/projects/*/sessions-index.json
//! and provides session status based on file modification times.
//! Also parses JSONL session files for rich status display (tool usage, thinking state, etc.).

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::io::{Read as _, Seek, SeekFrom};
use std::path::{Path, PathBuf};
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

/// State extracted from JSONL tail parsing
#[derive(Debug, Clone, Default)]
pub struct JsonlSessionState {
    /// Last assistant text (for summary)
    pub last_assistant_text: Option<String>,
    /// Last user text input
    pub last_user_input: Option<String>,
    /// Last tool name used
    pub last_tool_name: Option<String>,
    /// Inferred state detail
    pub state_detail: super::StatusDetail,
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
    /// Rich state from JSONL parsing
    pub jsonl_state: Option<JsonlSessionState>,
}

impl ClaudeSession {
    /// Convert to SessionStatus for unified handling
    pub fn to_session_status(&self) -> super::SessionStatus {
        // Use JSONL state if available for richer status
        if let Some(ref jsonl) = self.jsonl_state {
            let (status, state_detail) = if self.is_active {
                match jsonl.state_detail {
                    super::StatusDetail::UserInput => {
                        (super::StatusState::Waiting, super::StatusDetail::UserInput)
                    }
                    ref detail => (super::StatusState::Working, detail.clone()),
                }
            } else {
                (super::StatusState::Idle, super::StatusDetail::Inactive)
            };

            // Build summary from JSONL data
            let summary = if self.is_active {
                match jsonl.state_detail {
                    super::StatusDetail::ExecutingTool => {
                        jsonl.last_tool_name.as_ref().map(|t| format!("Running {}", t))
                    }
                    _ => jsonl
                        .last_assistant_text
                        .clone()
                        .or_else(|| self.summary.clone()),
                }
            } else {
                self.summary.clone()
            };

            let current_task = if self.is_active {
                jsonl.last_user_input.clone()
            } else {
                None
            };

            return super::SessionStatus {
                session_id: Some(self.session_id.clone()),
                project_path: Some(self.project_path.clone()),
                tool: Some("claude".to_string()),
                status,
                state_detail,
                summary,
                current_task,
                last_activity: Some(self.modified),
                progress: None,
                error: None,
                context: None,
            };
        }

        // Fallback: no JSONL data available
        let status = if self.is_active {
            super::StatusState::Working
        } else {
            super::StatusState::Idle
        };

        let state_detail = if self.is_active {
            super::StatusDetail::Thinking
        } else {
            super::StatusDetail::Inactive
        };

        super::SessionStatus {
            session_id: Some(self.session_id.clone()),
            project_path: Some(self.project_path.clone()),
            tool: Some("claude".to_string()),
            status,
            state_detail,
            summary: self.summary.clone(),
            current_task: None,
            last_activity: Some(self.modified),
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

/// Maximum bytes to read from the tail of a JSONL file
const JSONL_TAIL_MAX_BYTES: u64 = 32768;

/// Parse the tail of a JSONL session file to extract rich status information.
///
/// Reads up to `max_bytes` from the end of the file, splits into JSON lines,
/// and walks backward to find the latest assistant/user entries.
fn parse_jsonl_tail(path: &Path, max_bytes: u64) -> Option<JsonlSessionState> {
    let mut file = std::fs::File::open(path).ok()?;
    let file_len = file.metadata().ok()?.len();

    // Read tail bytes
    let read_start = file_len.saturating_sub(max_bytes);
    file.seek(SeekFrom::Start(read_start)).ok()?;

    let mut buf = Vec::with_capacity((file_len - read_start) as usize);
    file.read_to_end(&mut buf).ok()?;
    let text = String::from_utf8_lossy(&buf);

    // Split into lines, skip first partial line if we seeked into the middle
    let lines: Vec<&str> = text.lines().collect();
    let start_idx = if read_start > 0 { 1 } else { 0 };

    let mut last_assistant_text: Option<String> = None;
    let mut last_user_input: Option<String> = None;
    let mut last_tool_name: Option<String> = None;
    let mut last_entry_type: Option<String> = None;
    let mut last_content_kind: Option<String> = None; // "tool_use", "text", "thinking"

    // Walk lines backward to find relevant entries
    for line in lines[start_idx..].iter().rev() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let value: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let entry_type = value.get("type").and_then(|v| v.as_str()).unwrap_or("");

        match entry_type {
            "assistant" => {
                let content = value
                    .get("message")
                    .and_then(|m| m.get("content"))
                    .and_then(|c| c.as_array());

                if let Some(items) = content {
                    for item in items.iter().rev() {
                        let item_type = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
                        match item_type {
                            "tool_use" => {
                                if last_tool_name.is_none() {
                                    last_tool_name = item
                                        .get("name")
                                        .and_then(|v| v.as_str())
                                        .map(|s| s.to_string());
                                }
                                if last_content_kind.is_none() {
                                    last_content_kind = Some("tool_use".to_string());
                                    last_entry_type = Some("assistant".to_string());
                                }
                            }
                            "text" => {
                                if last_assistant_text.is_none() {
                                    let text = item
                                        .get("text")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("");
                                    let trimmed = text.trim();
                                    if !trimmed.is_empty() {
                                        last_assistant_text =
                                            Some(truncate_text(trimmed, 50));
                                    }
                                }
                                if last_content_kind.is_none() {
                                    last_content_kind = Some("text".to_string());
                                    last_entry_type = Some("assistant".to_string());
                                }
                            }
                            "thinking" => {
                                if last_content_kind.is_none() {
                                    last_content_kind = Some("thinking".to_string());
                                    last_entry_type = Some("assistant".to_string());
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            "user" => {
                let content = value
                    .get("message")
                    .and_then(|m| m.get("content"))
                    .and_then(|c| c.as_array());

                if let Some(items) = content {
                    for item in items {
                        let item_type = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
                        if item_type == "text" && last_user_input.is_none() {
                            let text = item
                                .get("text")
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            let trimmed = text.trim();
                            // Skip internal messages
                            if !trimmed.is_empty()
                                && !trimmed.starts_with("[Request interrupted")
                            {
                                last_user_input = Some(truncate_text(trimmed, 80));
                            }
                        }
                    }
                }

                if last_content_kind.is_none() {
                    // Check if user entry is a tool_result (assistant called a tool)
                    let has_tool_result = content.map_or(false, |items| {
                        items
                            .iter()
                            .any(|i| i.get("type").and_then(|v| v.as_str()) == Some("tool_result"))
                    });
                    if has_tool_result {
                        // The most recent entry is a tool_result from user,
                        // meaning the assistant is processing the tool result (thinking)
                        last_content_kind = Some("tool_result".to_string());
                        last_entry_type = Some("user".to_string());
                    } else {
                        last_content_kind = Some("user_text".to_string());
                        last_entry_type = Some("user".to_string());
                    }
                }
            }
            _ => continue,
        }

        // Stop once we have all the info we need
        if last_assistant_text.is_some()
            && last_user_input.is_some()
            && last_tool_name.is_some()
            && last_entry_type.is_some()
        {
            break;
        }
    }

    // Determine state_detail from the most recent entry
    let state_detail = match (
        last_entry_type.as_deref(),
        last_content_kind.as_deref(),
    ) {
        // Assistant is calling a tool
        (Some("assistant"), Some("tool_use")) => super::StatusDetail::ExecutingTool,
        // Assistant is writing text (thinking/responding)
        (Some("assistant"), Some("text")) => super::StatusDetail::Thinking,
        // Assistant is in extended thinking
        (Some("assistant"), Some("thinking")) => super::StatusDetail::Thinking,
        // User provided tool result → assistant is processing it
        (Some("user"), Some("tool_result")) => super::StatusDetail::Thinking,
        // User typed text → waiting for assistant (or assistant will start)
        (Some("user"), Some("user_text")) => super::StatusDetail::Thinking,
        // Default
        _ => super::StatusDetail::Inactive,
    };

    Some(JsonlSessionState {
        last_assistant_text,
        last_user_input,
        last_tool_name,
        state_detail,
    })
}

/// Truncate text to max characters, appending "..." if truncated
fn truncate_text(s: &str, max_chars: usize) -> String {
    let char_count = s.chars().count();
    if char_count > max_chars {
        let truncated: String = s.chars().take(max_chars.saturating_sub(3)).collect();
        format!("{}...", truncated)
    } else {
        s.to_string()
    }
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

                    // Parse JSONL tail for active sessions to get rich status
                    let jsonl_state = if is_active {
                        let state = parse_jsonl_tail(&file_path, JSONL_TAIL_MAX_BYTES);
                        if let Some(ref s) = state {
                            debug!(
                                session_id = %session_id,
                                state_detail = ?s.state_detail,
                                last_tool = ?s.last_tool_name,
                                last_text = ?s.last_assistant_text,
                                "Parsed JSONL tail"
                            );
                        }
                        state
                    } else {
                        None
                    };

                    // Use metadata from index if available, otherwise create minimal entry
                    if let Some(mut indexed) = index_sessions.remove(&session_id) {
                        // Update is_active based on actual file mtime (more reliable)
                        indexed.is_active = is_active;
                        indexed.jsonl_state = jsonl_state;
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
                            jsonl_state,
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
            jsonl_state: None,
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

    // --- JSONL tail parser tests ---

    fn write_jsonl(lines: &[&str]) -> tempfile::NamedTempFile {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        for line in lines {
            writeln!(f, "{}", line).unwrap();
        }
        f.flush().unwrap();
        f
    }

    #[test]
    fn test_parse_jsonl_tail_tool_use() {
        let f = write_jsonl(&[
            r#"{"type":"user","message":{"role":"user","content":[{"type":"text","text":"Add authentication"}]}}"#,
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"I will add auth."}]}}"#,
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","name":"Bash","id":"tool1","input":{}}]}}"#,
        ]);
        let state = parse_jsonl_tail(f.path(), 32768).unwrap();
        assert_eq!(state.state_detail, super::super::StatusDetail::ExecutingTool);
        assert_eq!(state.last_tool_name.as_deref(), Some("Bash"));
        assert_eq!(state.last_assistant_text.as_deref(), Some("I will add auth."));
        assert_eq!(state.last_user_input.as_deref(), Some("Add authentication"));
    }

    #[test]
    fn test_parse_jsonl_tail_assistant_text() {
        let f = write_jsonl(&[
            r#"{"type":"user","message":{"role":"user","content":[{"type":"text","text":"Fix the bug"}]}}"#,
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"The bug has been fixed successfully."}]}}"#,
        ]);
        let state = parse_jsonl_tail(f.path(), 32768).unwrap();
        assert_eq!(state.state_detail, super::super::StatusDetail::Thinking);
        assert!(state.last_assistant_text.as_deref().unwrap().contains("bug has been fixed"));
        assert_eq!(state.last_user_input.as_deref(), Some("Fix the bug"));
        assert!(state.last_tool_name.is_none());
    }

    #[test]
    fn test_parse_jsonl_tail_user_tool_result() {
        // After assistant calls a tool, user sends tool_result back
        let f = write_jsonl(&[
            r#"{"type":"user","message":{"role":"user","content":[{"type":"text","text":"Build the project"}]}}"#,
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","name":"Read","id":"t1","input":{}}]}}"#,
            r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":"file contents"}]}}"#,
        ]);
        let state = parse_jsonl_tail(f.path(), 32768).unwrap();
        // Last entry is tool_result → assistant is processing (thinking)
        assert_eq!(state.state_detail, super::super::StatusDetail::Thinking);
        assert_eq!(state.last_tool_name.as_deref(), Some("Read"));
        assert_eq!(state.last_user_input.as_deref(), Some("Build the project"));
    }

    #[test]
    fn test_parse_jsonl_tail_thinking_entry() {
        let f = write_jsonl(&[
            r#"{"type":"user","message":{"role":"user","content":[{"type":"text","text":"Explain this code"}]}}"#,
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"thinking","thinking":"Let me analyze..."}]}}"#,
        ]);
        let state = parse_jsonl_tail(f.path(), 32768).unwrap();
        assert_eq!(state.state_detail, super::super::StatusDetail::Thinking);
        assert_eq!(state.last_user_input.as_deref(), Some("Explain this code"));
        // thinking entries don't populate last_assistant_text
        assert!(state.last_assistant_text.is_none());
    }

    #[test]
    fn test_parse_jsonl_tail_empty_file() {
        let f = write_jsonl(&[]);
        let state = parse_jsonl_tail(f.path(), 32768);
        // Empty file should return Some with default Inactive state
        assert!(state.is_some());
        let s = state.unwrap();
        assert_eq!(s.state_detail, super::super::StatusDetail::Inactive);
    }

    #[test]
    fn test_parse_jsonl_tail_invalid_json() {
        let f = write_jsonl(&[
            "not valid json",
            "{also invalid",
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Valid line"}]}}"#,
        ]);
        let state = parse_jsonl_tail(f.path(), 32768).unwrap();
        // Should skip invalid lines and parse the valid one
        assert_eq!(state.last_assistant_text.as_deref(), Some("Valid line"));
    }

    #[test]
    fn test_parse_jsonl_tail_skips_interrupted_messages() {
        let f = write_jsonl(&[
            r#"{"type":"user","message":{"role":"user","content":[{"type":"text","text":"Real user input"}]}}"#,
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Working on it"}]}}"#,
            r#"{"type":"user","message":{"role":"user","content":[{"type":"text","text":"[Request interrupted by user for tool use]"}]}}"#,
        ]);
        let state = parse_jsonl_tail(f.path(), 32768).unwrap();
        // Should skip the interrupted message and find the real user input
        assert_eq!(state.last_user_input.as_deref(), Some("Real user input"));
    }

    #[test]
    fn test_parse_jsonl_tail_progress_entries_ignored() {
        let f = write_jsonl(&[
            r#"{"type":"user","message":{"role":"user","content":[{"type":"text","text":"Do something"}]}}"#,
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","name":"Grep","id":"t1","input":{}}]}}"#,
            r#"{"type":"progress","data":{"type":"hook_progress","hookEvent":"PreToolUse"}}"#,
        ]);
        let state = parse_jsonl_tail(f.path(), 32768).unwrap();
        // Progress entries should be skipped; last meaningful entry is tool_use
        assert_eq!(state.state_detail, super::super::StatusDetail::ExecutingTool);
        assert_eq!(state.last_tool_name.as_deref(), Some("Grep"));
    }

    #[test]
    fn test_parse_jsonl_tail_nonexistent_file() {
        let result = parse_jsonl_tail(Path::new("/nonexistent/path.jsonl"), 32768);
        assert!(result.is_none());
    }

    #[test]
    fn test_truncate_text() {
        assert_eq!(truncate_text("short", 50), "short");
        let long = "a".repeat(60);
        let truncated = truncate_text(&long, 50);
        assert!(truncated.ends_with("..."));
        assert!(truncated.chars().count() <= 50);
    }

    #[test]
    fn test_to_session_status_with_jsonl_tool_use() {
        let session = ClaudeSession {
            session_id: "test-id".to_string(),
            external_id: "claude:test-id".to_string(),
            project_path: "/test".to_string(),
            summary: Some("Index summary".to_string()),
            message_count: 5,
            created: Utc::now(),
            modified: Utc::now(),
            git_branch: None,
            is_active: true,
            jsonl_state: Some(JsonlSessionState {
                last_assistant_text: Some("Working on auth".to_string()),
                last_user_input: Some("Add login".to_string()),
                last_tool_name: Some("Bash".to_string()),
                state_detail: super::super::StatusDetail::ExecutingTool,
            }),
        };
        let status = session.to_session_status();
        assert_eq!(status.state_detail, super::super::StatusDetail::ExecutingTool);
        assert_eq!(status.summary.as_deref(), Some("Running Bash"));
        assert_eq!(status.current_task.as_deref(), Some("Add login"));
        assert_eq!(status.status, super::super::StatusState::Working);
    }

    #[test]
    fn test_to_session_status_with_jsonl_thinking() {
        let session = ClaudeSession {
            session_id: "test-id".to_string(),
            external_id: "claude:test-id".to_string(),
            project_path: "/test".to_string(),
            summary: Some("Index summary".to_string()),
            message_count: 5,
            created: Utc::now(),
            modified: Utc::now(),
            git_branch: None,
            is_active: true,
            jsonl_state: Some(JsonlSessionState {
                last_assistant_text: Some("Adding authentication module".to_string()),
                last_user_input: Some("Add auth".to_string()),
                last_tool_name: None,
                state_detail: super::super::StatusDetail::Thinking,
            }),
        };
        let status = session.to_session_status();
        assert_eq!(status.state_detail, super::super::StatusDetail::Thinking);
        assert_eq!(status.summary.as_deref(), Some("Adding authentication module"));
        assert_eq!(status.status, super::super::StatusState::Working);
    }

    #[test]
    fn test_to_session_status_inactive_ignores_jsonl() {
        let session = ClaudeSession {
            session_id: "test-id".to_string(),
            external_id: "claude:test-id".to_string(),
            project_path: "/test".to_string(),
            summary: Some("Index summary".to_string()),
            message_count: 5,
            created: Utc::now(),
            modified: Utc::now(),
            git_branch: None,
            is_active: false,
            jsonl_state: Some(JsonlSessionState {
                last_assistant_text: Some("Old text".to_string()),
                last_user_input: None,
                last_tool_name: None,
                state_detail: super::super::StatusDetail::Thinking,
            }),
        };
        let status = session.to_session_status();
        // Inactive session should use Inactive state regardless of JSONL
        assert_eq!(status.state_detail, super::super::StatusDetail::Inactive);
        assert_eq!(status.status, super::super::StatusState::Idle);
        assert_eq!(status.summary.as_deref(), Some("Index summary"));
    }

    #[test]
    fn test_to_session_status_without_jsonl() {
        let session = ClaudeSession {
            session_id: "test-id".to_string(),
            external_id: "claude:test-id".to_string(),
            project_path: "/test".to_string(),
            summary: Some("Fallback summary".to_string()),
            message_count: 5,
            created: Utc::now(),
            modified: Utc::now(),
            git_branch: None,
            is_active: true,
            jsonl_state: None,
        };
        let status = session.to_session_status();
        // Without JSONL data, should fall back to Thinking
        assert_eq!(status.state_detail, super::super::StatusDetail::Thinking);
        assert_eq!(status.summary.as_deref(), Some("Fallback summary"));
    }
}

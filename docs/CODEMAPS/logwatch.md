# Logwatch Module Codemap

**Last Updated:** 2026-01-30
**Location:** `src/logwatch/`

## Overview

The logwatch module provides log-based CLI status tracking for AI tools (Claude Code, Kiro CLI). It monitors session status via file polling (sessions-index.json for Claude, SQLite for Kiro) and optionally uses AI analysis for structured status extraction from log files.

## Architecture

- **Claude Code**: Polls `~/.claude/projects/*/sessions-index.json` + process detection via `pgrep`/`lsof`
- **Kiro CLI**: Reads status from SQLite database (`kiro-cli/data.sqlite3`) + process detection

## Structure

```
src/logwatch/
├── mod.rs              # Module exports
├── schema.rs           # StatusState, StatusDetail, SessionStatus schema
├── analyzer.rs         # AI-powered log analysis (Claude CLI invocation)
├── claude_sessions.rs  # Claude Code session fetcher via sessions-index.json
├── collector.rs        # Log file monitoring and collection
└── kiro_sqlite.rs      # Kiro CLI status fetcher via SQLite database
```

## Key Types

### StatusState (schema.rs)

Main status states for session tracking.

```rust
pub enum StatusState {
    Working,       // AI is actively working
    Waiting,       // Waiting for user input/confirmation
    Completed,     // Task completed
    Error,         // Error occurred
    Idle,          // Session inactive (default)
    Disconnected,  // Session ended
}
```

**Methods:** `as_str()`, `icon()`, `color()` (ratatui Color)

### StatusDetail (schema.rs)

Detailed status information.

```rust
pub enum StatusDetail {
    // Working
    Thinking, ExecutingTool, WritingCode,
    // Waiting
    UserInput, Confirmation,
    // Completed
    Success, Partial,
    // Error
    ApiError, ToolError,
    // Idle/Disconnected
    Inactive, SessionEnded,
}
```

**Methods:** `as_str()`, `label()` (human-readable display)

### SessionStatus (schema.rs)

Complete session status from AI analysis or polling.

```rust
pub struct SessionStatus {
    pub session_id: Option<String>,
    pub project_path: Option<String>,
    pub tool: Option<String>,
    pub status: StatusState,
    pub state_detail: StatusDetail,
    pub summary: Option<String>,       // Max 50 chars
    pub current_task: Option<String>,
    pub last_activity: Option<DateTime<Utc>>,
    pub progress: Option<AnalysisProgress>,
    pub error: Option<String>,
    pub context: Option<AnalysisContext>,
}
```

**Methods:**
| Method | Description |
|--------|-------------|
| `new_idle()` | Create idle status |
| `new_error(msg)` | Create error status |
| `new_disconnected()` | Create disconnected status |
| `display_summary()` | Truncated summary (max 50 chars) |
| `time_since_activity()` | Human-readable time ago string |

### AnalysisProgress (schema.rs)

```rust
pub struct AnalysisProgress {
    pub completed_steps: Vec<String>,
    pub current_step: Option<String>,
    pub pending_steps: Vec<String>,
}
```

### AnalysisContext (schema.rs)

```rust
pub struct AnalysisContext {
    pub files_modified: Vec<String>,
    pub tokens_used: Option<u64>,
    pub model: Option<String>,
}
```

### ClaudeSession (claude_sessions.rs)

```rust
pub struct ClaudeSession {
    pub session_id: String,
    pub external_id: String,       // "claude:{session_id}"
    pub project_path: String,
    pub summary: Option<String>,
    pub message_count: u32,
    pub created: DateTime<Utc>,
    pub modified: DateTime<Utc>,
    pub git_branch: Option<String>,
    pub is_active: bool,
}
```

**Methods:** `to_session_status()` -> SessionStatus

### ClaudeProcessInfo (claude_sessions.rs)

```rust
pub struct ClaudeProcessInfo {
    pub cwd: String,
    pub session_id: Option<String>,
}
```

### ClaudeSessionsFetcher (claude_sessions.rs)

```rust
pub struct ClaudeSessionsFetcher { config: ClaudeSessionsConfig }
```

**Methods:**
| Method | Description |
|--------|-------------|
| `new()` | Create with default config |
| `with_config(config)` | Create with custom config |
| `is_available()` | Check if ~/.claude/projects exists |
| `get_sessions(paths)` | Get sessions for workspace paths |
| `get_all_sessions()` | Get all sessions (unfiltered) |
| `get_running_processes()` | Detect running Claude processes via pgrep/lsof |
| `get_running_workspaces()` | Map of workspace path -> process count |
| `get_running_session_ids(path)` | Session IDs for running processes |
| `is_claude_running(path)` | Check if Claude runs for workspace |
| `get_process_count(path)` | Process count for workspace |

### KiroStatus (kiro_sqlite.rs)

```rust
pub struct KiroStatus {
    pub conversation_id: String,
    pub state: StatusState,
    pub state_detail: StatusDetail,
    pub summary: Option<String>,
    pub updated_at: SystemTime,
}
```

**Methods:** `to_session_status(path)`, `external_id(path)`

### KiroSqliteFetcher (kiro_sqlite.rs)

```rust
pub struct KiroSqliteFetcher { config: KiroSqliteConfig }
```

**Methods:**
| Method | Description |
|--------|-------------|
| `new()` | Create with default config |
| `with_config(config)` | Create with custom config |
| `is_available()` | Check if SQLite database exists |
| `get_status(path)` | Get first active session for workspace |
| `get_all_statuses(path)` | Get all active sessions for workspace |
| `get_statuses(workspaces)` | Get statuses for multiple workspaces |
| `get_running_kiro_workspaces()` | Map of workspace path -> process count |
| `get_kiro_process_count(path)` | Process count for workspace |

### LogAnalyzer (analyzer.rs)

AI-powered log analysis using Claude CLI in non-interactive mode.

```rust
pub struct LogAnalyzer { config: AnalyzerConfig }
```

**Methods:**
| Method | Description |
|--------|-------------|
| `new(config)` | Create with config |
| `analyze(log)` | Analyze log content (async) |
| `is_available()` | Check if CLI tool exists (async) |

**Free Function:** `extract_status_heuristic(log)` - Fallback heuristic without AI

### LogCollector (collector.rs)

Monitors and collects log files from Claude/Kiro directories.

```rust
pub struct LogCollector {
    config: CollectorConfig,
    tracked_files: HashMap<PathBuf, LogFileInfo>,
}
```

**Methods:**
| Method | Description |
|--------|-------------|
| `new(config)` | Create with config |
| `scan()` | Scan for new/updated logs |
| `read_for_project(path)` | Force read logs for specific project |
| `spawn(tx)` | Start as background tokio task |

### LogContent (collector.rs)

```rust
pub struct LogContent {
    pub source: PathBuf,
    pub project_path: Option<String>,
    pub tool: String,
    pub lines: Vec<String>,
    pub collected_at: SystemTime,
}
```

## Data Flow

```
Process Detection (pgrep/lsof)
        │
        ├── Claude: get_running_processes() -> ClaudeProcessInfo
        └── Kiro:   get_running_kiro_workspaces() -> HashMap

Session Status Fetching
        │
        ├── Claude: sessions-index.json -> ClaudeSession -> SessionStatus
        │           ClaudeSessionsFetcher.get_sessions()
        │
        └── Kiro:   SQLite (conversations_v2) -> KiroStatus -> SessionStatus
                    KiroSqliteFetcher.get_statuses()

Log Analysis (Optional)
        │
        ├── LogCollector.scan() -> Vec<LogContent>
        └── LogAnalyzer.analyze(log) -> SessionStatus
            │
            └── Fallback: extract_status_heuristic(log)
```

## Configuration

### ClaudeSessionsConfig

```rust
pub struct ClaudeSessionsConfig {
    pub claude_dir: PathBuf,                // ~/.claude
    pub inactivity_threshold_secs: u64,     // 60 (default)
}
```

### KiroSqliteConfig

```rust
pub struct KiroSqliteConfig {
    pub db_path: PathBuf,       // ~/Library/Application Support/kiro-cli/data.sqlite3
    pub timeout_secs: u64,      // 5 (default)
}
```

### AnalyzerConfig

```rust
pub struct AnalyzerConfig {
    pub analyzer_tool: String,       // "claude" (default)
    pub timeout_secs: u64,           // 30 (default)
    pub max_content_length: usize,   // 50000 (default)
}
```

### CollectorConfig

```rust
pub struct CollectorConfig {
    pub claude_home: PathBuf,
    pub kiro_logs_dir: Option<PathBuf>,
    pub max_lines: usize,           // 500 (default)
    pub scan_interval_secs: u64,    // 5 (default)
    pub min_file_age_secs: u64,     // 1 (default)
}
```

## Exports

```rust
pub use analyzer::LogAnalyzer;
pub use claude_sessions::{ClaudeSession, ClaudeSessionsConfig, ClaudeSessionsFetcher};
pub use collector::LogCollector;
pub use kiro_sqlite::{KiroSqliteConfig, KiroSqliteFetcher, KiroStatus};
pub use schema::{AnalysisProgress, SessionStatus, StatusDetail, StatusState};
```

## Related Modules

- [workspace](workspace.md) - Session struct uses logwatch types for status updates
- [app](app.md) - AppState integrates logwatch data into session tracking
- main.rs - Orchestrates polling and status updates

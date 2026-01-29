# Workspace Module Codemap

**Last Updated:** 2026-01-30
**Location:** `src/workspace/`

## Overview

The workspace module handles git repository detection, worktree scanning, worktree lifecycle management, and AI CLI session tracking.

## Structure

```
src/workspace/
├── mod.rs       # Module exports
├── state.rs     # Workspace struct, WorkspaceStatus enum
├── worktree.rs  # WorktreeInfo, detection functions
├── manager.rs   # WorktreeManager for create/delete operations
└── session.rs   # Session, SessionId, AiTool, SessionStatus
```

## Key Types

### Workspace (state.rs)

Represents a single workspace (git repository or worktree).

```rust
pub struct Workspace {
    pub project_path: String,       // Absolute path to workspace
    pub repo_name: String,          // Repository/worktree name
    pub branch: String,             // Current branch
    pub status: WorkspaceStatus,    // Current status
    pub session_id: Option<String>, // Connected AI CLI session
    pub pane_id: Option<u32>,       // Zellij pane ID (internal mode)
    pub message: Option<String>,    // Status message
    pub updated_at: SystemTime,     // Last update time
}
```

**Methods:**
| Method | Description |
|--------|-------------|
| `new()` | Create new workspace |
| `update_status()` | Update status and timestamp |
| `display_path()` | Get shortened display path (~/ prefix) |

### WorkspaceStatus (state.rs)

```rust
pub enum WorkspaceStatus {
    Disconnected,  // No AI CLI session
    Idle,          // Session active, waiting for input
    Working,       // AI is processing
    NeedsInput,    // Waiting for user input
    Error,         // Error state
}
```

### Session (session.rs)

An active AI CLI session within a workspace. Each workspace can have multiple sessions.

```rust
pub struct Session {
    pub id: SessionId,                  // Internal UUID
    pub external_id: String,            // "claude:{uuid}" or "kiro:{path}:{conv_id}"
    pub workspace_index: usize,         // Parent workspace index
    pub tool: AiTool,                   // AI tool type
    pub status: SessionStatus,          // Current status
    pub state_detail: Option<String>,   // Detailed state label
    pub summary: Option<String>,        // Brief summary (max 50 chars)
    pub current_task: Option<String>,   // Current task description
    pub last_activity: Option<SystemTime>,
    pub pane_id: Option<u32>,           // Zellij pane ID
    pub tab_name: Option<String>,       // Zellij tab name
    pub created_at: SystemTime,
    pub updated_at: SystemTime,
}
```

**Methods:**
| Method | Description |
|--------|-------------|
| `new(external_id, workspace_index, tool)` | Create new session |
| `update_status(status, message)` | Update status and summary |
| `update_from_logwatch_status(status)` | Update from logwatch schema |
| `disconnect()` | Mark as disconnected |
| `is_active()` | Check if not disconnected |
| `display_info()` | Format display string with detail + summary + time |
| `time_since_activity()` | Human-readable time ago |

### SessionId (session.rs)

```rust
pub struct SessionId(Uuid);
```

**Methods:** `new()`, `from_uuid(uuid)`, `as_uuid()`

### AiTool (session.rs)

```rust
pub enum AiTool {
    Claude,    // Anthropic
    Kiro,      // AWS
    OpenCode,
    Codex,     // OpenAI
}
```

**Methods:** `from_str(s)`, `name()`, `icon()` ([C], [K], [O], [X]), `color()`

### SessionStatus (session.rs)

```rust
pub enum SessionStatus {
    Idle,          // Waiting for user input
    Working,       // Actively working
    NeedsInput,    // Waiting for confirmation
    Success,       // Completed successfully
    Error,         // Error state
    Disconnected,  // Session ended
}
```

**Methods:** `from_str(s)`, `icon()`, `color()`

### External ID Functions (session.rs)

| Function | Description |
|----------|-------------|
| `claude_external_id(session_id)` | `"claude:{uuid}"` |
| `kiro_external_id(path, conv_id)` | `"kiro:{path}:{conv_id}"` |
| `kiro_external_id_legacy(path)` | `"kiro:{path}"` |
| `parse_external_id(id)` | Returns `(AiTool, raw_id)` |
| `parse_kiro_external_id(id)` | Returns `(project_path, conv_id)` |

### WorktreeInfo (worktree.rs)

```rust
pub struct WorktreeInfo {
    pub path: PathBuf,
    pub name: String,
    pub branch: Option<String>,
    pub is_worktree: bool,
    pub is_bare: bool,
}
```

Implements `From<WorktreeInfo> for Workspace`.

### WorktreeListInfo (manager.rs)

```rust
pub struct WorktreeListInfo {
    pub path: PathBuf,
    pub branch: Option<String>,
    pub commit: Option<String>,
    pub is_bare: bool,
    pub is_detached: bool,
}
```

### WorktreeManager (manager.rs)

```rust
pub struct WorktreeManager { config: WorktreeConfig }
```

**Methods:**
| Method | Description |
|--------|-------------|
| `new(config)` | Create with configuration |
| `create_worktree(repo, branch, create_branch)` | Create new worktree |
| `remove_worktree(repo, worktree, force)` | Remove worktree |
| `list_worktrees(repo)` | List all worktrees |
| `list_local_branches(repo)` | List local branches |
| `list_remote_branches(repo)` | List remote branches |

## Key Functions (worktree.rs)

### scan_for_repositories

```rust
pub fn scan_for_repositories(root: &Path, max_depth: usize) -> Vec<WorktreeInfo>
```

Recursively scans directory for git repositories and worktrees.

### detect_worktrees

```rust
pub fn detect_worktrees(repo_path: &Path) -> Vec<WorktreeInfo>
```

### get_default_search_paths

```rust
pub fn get_default_search_paths() -> Vec<PathBuf>
```

Returns: `~/work`, `~/ghq` (or `$GHQ_ROOT`)

## Worktree Path Generation

| Style | Example |
|-------|---------|
| Parallel (default) | `~/work/myrepo=feature-branch` |
| Ghq | `~/ghq/github.com/owner/repo=feature-branch` |
| Subdirectory | `~/work/myrepo/.worktrees/feature-branch` |
| Custom | User-defined template with `{repo}`, `{branch}`, `{repo_path}` |

## Git Operations

Uses both:
- **git2 crate** for read operations (Repository::open, branch listing)
- **git CLI** for write operations (worktree add/remove)

## Exports

```rust
pub use manager::WorktreeManager;
pub use state::{Workspace, WorkspaceStatus};
pub use worktree::{detect_worktrees, get_default_search_paths, scan_for_repositories, WorktreeInfo};
pub use session::*;  // SessionId, Session, AiTool, SessionStatus, helpers
```

## Related Modules

- [app](app.md) - Uses Workspace and Session in AppState
- [logwatch](logwatch.md) - Provides SessionStatus for session updates
- Uses git2 crate for repository operations

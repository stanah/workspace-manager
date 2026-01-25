# Workspace Module Codemap

**Last Updated:** 2025-01-26
**Location:** `src/workspace/`

## Overview

The workspace module handles git repository detection, worktree scanning, and worktree lifecycle management.

## Structure

```
src/workspace/
├── mod.rs       # Module exports
├── state.rs     # Workspace struct, WorkspaceStatus enum
├── worktree.rs  # WorktreeInfo, detection functions
└── manager.rs   # WorktreeManager for create/delete operations
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

### WorktreeInfo (worktree.rs)

Information about a detected git repository or worktree.

```rust
pub struct WorktreeInfo {
    pub path: PathBuf,           // Absolute path
    pub name: String,            // Directory name
    pub branch: Option<String>,  // Current branch
    pub is_worktree: bool,       // true if worktree, false if main repo
    pub is_bare: bool,           // true if bare repository
}
```

Implements `From<WorktreeInfo> for Workspace`.

### WorktreeListInfo (manager.rs)

Detailed worktree information from `git worktree list`.

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

Handles worktree creation and deletion.

```rust
pub struct WorktreeManager {
    config: WorktreeConfig,
}
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

**Algorithm:**
1. Walk directories up to max_depth
2. Check for `.git` file/directory
3. If file: parse gitdir reference (worktree)
4. If directory: check if bare or regular repo
5. Extract branch from HEAD reference

### detect_worktrees

```rust
pub fn detect_worktrees(repo_path: &Path) -> Vec<WorktreeInfo>
```

Detects all worktrees for a given repository.

### get_default_search_paths

```rust
pub fn get_default_search_paths() -> Vec<PathBuf>
```

Returns default paths to scan:
- `~/work`
- `~/ghq` (if GHQ_ROOT not set)
- `$GHQ_ROOT` (if set)

## Worktree Path Generation

The `WorktreeConfig::generate_worktree_path()` method supports multiple styles:

### Parallel (default)
```
Parent directory: ~/work/myrepo
New worktree:     ~/work/myrepo=feature-branch
```

### Ghq
```
GHQ root:     ~/ghq
Remote URL:   git@github.com:owner/repo.git
New worktree: ~/ghq/github.com/owner/repo=feature-branch
```

### Subdirectory
```
Repository:   ~/work/myrepo
New worktree: ~/work/myrepo/.worktrees/feature-branch
```

### Custom
User-defined template with placeholders:
- `{repo}` - repository name
- `{branch}` - branch name (/ replaced with -)
- `{repo_path}` - full repository path

## Data Flow

```
scan_workspaces() in AppState
        │
        ▼
get_default_search_paths()
        │
        ▼
scan_for_repositories(path, depth)
        │
        ├── Walk directories
        ├── Detect .git file/directory
        └── Extract WorktreeInfo
                │
                ▼
        Vec<WorktreeInfo>
                │
                ▼
        into() -> Vec<Workspace>
                │
                ▼
        AppState.workspaces
```

## Git Operations

The module uses both:
- **git2 crate** for read operations (Repository::open, branch listing)
- **git CLI** for write operations (worktree add/remove)

This hybrid approach provides:
- Fast read access via libgit2
- Reliable write operations via git CLI
- Proper handling of hooks and refs

## Exports

```rust
pub use manager::WorktreeManager;
pub use state::{Workspace, WorkspaceStatus};
pub use worktree::{detect_worktrees, get_default_search_paths, scan_for_repositories, WorktreeInfo};
```

## Related Modules

- [app](app.md) - Uses Workspace in AppState
- Uses git2 crate for repository operations

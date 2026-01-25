# App Module Codemap

**Last Updated:** 2025-01-26
**Location:** `src/app/`

## Overview

The app module contains the core application state, configuration, and event handling logic.

## Structure

```
src/app/
├── mod.rs      # Module exports
├── state.rs    # AppState, ViewMode, TreeItem, ListDisplayMode
├── config.rs   # Config, ZellijConfig, WorktreeConfig
└── events.rs   # Action, AppEvent, poll_event, mouse_action
```

## Key Types

### AppState (state.rs)

Central application state container.

```rust
pub struct AppState {
    // Data
    pub workspaces: Vec<Workspace>,      // Detected workspaces
    pub tree_items: Vec<TreeItem>,       // Flattened tree for display
    collapsed_repos: HashSet<String>,     // Collapsed repository groups
    session_map: HashMap<String, usize>,  // session_id -> workspace index
    open_tabs: HashSet<String>,           // Zellij tab names cache

    // UI State
    pub selected_index: usize,
    pub view_mode: ViewMode,
    pub list_display_mode: ListDisplayMode,
    pub input_dialog: Option<InputDialog>,
    pub selection_dialog: Option<SelectionDialog>,
    pub should_quit: bool,
    pub status_message: Option<String>,
}
```

**Key Methods:**
| Method | Description |
|--------|-------------|
| `scan_workspaces()` | Scan configured paths for git repositories |
| `rebuild_tree()` | Rebuild tree_items from workspaces |
| `rebuild_tree_with_manager()` | Rebuild including branch information |
| `selected_workspace()` | Get currently selected workspace |
| `selected_branch_info()` | Get branch info if branch is selected |
| `toggle_expand()` | Expand/collapse repository group |
| `update_open_tabs()` | Update Zellij tab cache |
| `is_workspace_open()` | Check if workspace has open Zellij tab |

### ViewMode (state.rs)

```rust
pub enum ViewMode {
    List,       // Main workspace list
    Help,       // Help overlay
    Detail,     // Workspace detail overlay
    Input,      // Text input dialog
    Selection,  // Selection dialog (session/layout)
}
```

### ListDisplayMode (state.rs)

```rust
pub enum ListDisplayMode {
    Worktrees,           // Show only existing worktrees
    WithLocalBranches,   // + local branches without worktrees
    WithAllBranches,     // + remote branches
}
```

### TreeItem (state.rs)

```rust
pub enum TreeItem {
    RepoGroup { name, path, expanded, worktree_count },
    Worktree { workspace_index, is_last },
    Branch { name, is_local, repo_path, is_last },
}
```

### Config (config.rs)

```rust
pub struct Config {
    pub search_paths: Vec<PathBuf>,
    pub max_scan_depth: usize,
    pub socket_path: PathBuf,
    pub log_level: String,
    pub editor: String,           // Editor command (code, cursor, vim)
    pub zellij: ZellijConfig,
    pub worktree: WorktreeConfig,
}
```

**Methods:**
| Method | Description |
|--------|-------------|
| `load()` | Load from file or create default |
| `save()` | Save current config to file |
| `config_path()` | Get config file path |
| `save_zellij_session()` | Update and save Zellij session |
| `save_zellij_layout()` | Update and save default layout |

### ZellijConfig (config.rs)

```rust
pub struct ZellijConfig {
    pub enabled: bool,
    pub session_name: Option<String>,
    pub default_layout: Option<PathBuf>,
    pub layout_dir: Option<PathBuf>,
    pub tab_name_template: String,  // "{repo}/{branch}"
    pub ai_command: String,          // "claude", "kiro-cli", etc.
}
```

**Methods:**
| Method | Description |
|--------|-------------|
| `generate_tab_name()` | Generate tab name from template |
| `ensure_layout_dir()` | Create layout directory if needed |
| `generate_builtin_layouts()` | Generate layouts from templates |

### WorktreeConfig (config.rs)

```rust
pub struct WorktreeConfig {
    pub path_style: WorktreePathStyle,
    pub ghq_root: Option<PathBuf>,
    pub default_remote: String,
}

pub enum WorktreePathStyle {
    Parallel,              // {repo_parent}/{repo}={branch}
    Ghq,                   // {ghq_root}/{host}/{owner}/{repo}={branch}
    Subdirectory,          // {repo}/.worktrees/{branch}
    Custom(String),        // User template
}
```

### Action (events.rs)

```rust
pub enum Action {
    Quit, MoveUp, MoveDown, Select, SelectWithLayout,
    ToggleExpand, ToggleDisplayMode, ToggleHelp, Back, Refresh,
    CreateWorktree, DeleteWorktree, OpenInEditor,
    LaunchLazygit, LaunchShell, LaunchYazi, NewSession, CloseWorkspace,
    MouseSelect(u16), MouseDoubleClick(u16), MouseMiddleClick(u16),
    ScrollUp, ScrollDown, None,
}
```

### AppEvent (events.rs)

```rust
pub enum AppEvent {
    Key(KeyEvent),
    Mouse(MouseEvent),
    Resize(u16, u16),
    WorkspaceRegister { session_id, project_path, pane_id },
    WorkspaceUpdate { session_id, status, message },
    WorkspaceUnregister { session_id },
}
```

## Data Flow

```
User Input (Key/Mouse)
        │
        ▼
poll_event() -> AppEvent
        │
        ▼
Action::from(KeyEvent)
        │
        ▼
handle_action() in main.rs
        │
        ├── Modifies AppState
        ├── Calls ZellijActions
        └── Calls WorktreeManager

Notification (UDS)
        │
        ▼
notify_rx.try_recv() -> AppEvent
        │
        ▼
handle_notify_event() in main.rs
        │
        ▼
Updates AppState.workspaces
```

## Configuration File Format

`~/.config/workspace-manager/config.toml`:

```toml
search_paths = ["~/work", "~/ghq"]
max_scan_depth = 3
socket_path = "/tmp/workspace-manager.sock"
log_level = "info"
editor = "code"

[zellij]
enabled = true
session_name = "main"
tab_name_template = "{repo}/{branch}"
ai_command = "claude"

[worktree]
path_style = "Parallel"
default_remote = "origin"
```

## Related Modules

- [ui](ui.md) - Renders AppState to terminal
- [workspace](workspace.md) - Provides Workspace data
- [notify](notify.md) - Sends AppEvents for status updates

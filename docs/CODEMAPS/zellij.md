# Zellij Module Codemap

**Last Updated:** 2026-01-30
**Location:** `src/zellij/`

## Overview

The zellij module provides integration with the Zellij terminal multiplexer, supporting both internal (running inside Zellij) and external (controlling a remote session) modes.

## Structure

```
src/zellij/
├── mod.rs      # Module exports
└── actions.rs  # ZellijActions, ZellijMode, TabActionResult
```

## Key Types

### ZellijMode

```rust
pub enum ZellijMode {
    /// Running inside Zellij (ZELLIJ env var set)
    Internal,
    /// Controlling external session
    External { session_name: String },
}
```

### TabActionResult

```rust
pub enum TabActionResult {
    SwitchedToExisting(String),  // Switched to existing tab
    CreatedNew(String),          // Created new tab
    SessionNotFound(String),     // Target session not found
}
```

### ZellijActions

Main interface for Zellij operations.

```rust
pub struct ZellijActions {
    mode: ZellijMode,
}
```

## Constructors

| Constructor | Description |
|-------------|-------------|
| `new_internal()` | Create for internal mode |
| `new_external(session)` | Create for external mode |
| `auto_detect(config_session)` | Auto-detect based on environment |
| `new()` | Alias for `auto_detect(None)` |

**Auto-detection logic:**
1. If `ZELLIJ` env var exists -> Internal mode
2. If config has session_name -> External mode with that session
3. Otherwise -> External mode with empty session

## Mode Detection Methods

| Method | Description |
|--------|-------------|
| `is_available()` | True if operations can be performed |
| `is_internal()` | True if running inside Zellij |
| `session_name()` | Get session name (External only) |
| `set_session_name()` | Set session name (External only) |

## Session/Tab Management (External Mode)

| Method | Description |
|--------|-------------|
| `list_sessions()` | List all Zellij sessions |
| `session_exists(name)` | Check if session exists |
| `query_tab_names(session)` | Get tab names in session |
| `go_to_tab(session, name)` | Switch to named tab |
| `close_tab(session, name)` | Close named tab |
| `new_tab(session, name, cwd, layout)` | Create new tab |
| `open_workspace_tab(name, cwd, layout)` | High-level: open or switch to tab |
| `list_layouts(dir)` | List .kdl files in directory |

### open_workspace_tab Flow

```
open_workspace_tab(tab_name, cwd, layout)
        │
        ├── Check mode is External with session
        │
        ├── session_exists()?
        │   └── No -> SessionNotFound
        │
        ├── query_tab_names()
        │   └── Tab exists? -> go_to_tab() -> SwitchedToExisting
        │
        └── new_tab() -> CreatedNew
```

## Pane Operations (Internal Mode)

| Method | Description |
|--------|-------------|
| `focus_pane(pane_id)` | Focus specific pane |
| `new_pane(cwd, command)` | Create new pane with command |
| `close_pane(pane_id)` | Close specific pane |

## Tool Launchers (Internal Mode)

| Method | Command | Description |
|--------|---------|-------------|
| `launch_shell(cwd)` | `zsh` | Open shell in directory |
| `launch_lazygit(cwd)` | `lazygit` | Open lazygit |
| `launch_yazi(cwd)` | `yazi` | Open yazi file manager |
| `launch_claude(cwd)` | `claude` | Start Claude Code session |

All launchers use `zellij run --cwd <path> -- <command>`.

## CLI Commands Used

### Session Management
```bash
zellij list-sessions --no-formatting
zellij --session <name> action query-tab-names
zellij --session <name> action go-to-tab-name <tab>
zellij --session <name> action close-tab
zellij --session <name> action new-tab --name <name> --cwd <path> [--layout <file>]
```

### Pane Operations (Internal)
```bash
zellij action focus-pane --pane-id <id>
zellij action close-pane --pane-id <id>
zellij run --cwd <path> -- <command> [args...]
```

## Layout Files

Layout files are Zellij KDL configuration files stored in:
- `~/.config/workspace-manager/layouts/` (default)
- Custom path via `zellij.layout_dir` config

Built-in templates (generated from `layouts/*.kdl.template`):
- `simple.kdl` - Single pane with AI CLI
- `with-shell.kdl` - AI CLI + shell pane
- `dev.kdl` - AI CLI + shell + file browser

Template variable: `{{AI_COMMAND}}` replaced with configured command.

## Exports

```rust
pub use actions::{TabActionResult, ZellijActions, ZellijMode};
```

## Related Modules

- [app](app.md) - ZellijConfig configuration
- [zellij-tab-sync](zellij-tab-sync.md) - Companion Zellij plugin for tab focus tracking
- Layout templates in `layouts/` directory

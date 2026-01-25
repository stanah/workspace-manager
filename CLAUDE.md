# CLAUDE.md

This file provides guidance to Claude Code when working with this repository.

## Overview

Workspace Manager is a Rust TUI application for managing multiple Claude Code/Kiro-CLI workspaces. It provides:

- Git worktree detection and management
- Real-time status updates via Unix Domain Socket notifications
- Zellij terminal multiplexer integration (internal and external modes)
- Tree-based workspace organization with branch browsing

## Key Commands

```bash
# Build
cargo build

# Run TUI (default)
cargo run

# Run with debug logging
cargo run -- --log-level debug

# Send notification to running TUI
cargo run -- notify register --session-id $CLAUDE_SESSION_ID --project-path .
cargo run -- notify status $CLAUDE_SESSION_ID working
cargo run -- notify unregister --session-id $CLAUDE_SESSION_ID

# Run tests
cargo test
```

## Architecture

### Module Structure

```
src/
├── main.rs              # Entry point, CLI parsing, TUI loop, event handling
├── lib.rs               # Library exports
├── app/
│   ├── mod.rs           # Module exports
│   ├── state.rs         # AppState, ViewMode, TreeItem, ListDisplayMode
│   ├── config.rs        # Config, ZellijConfig, WorktreeConfig, WorktreePathStyle
│   └── events.rs        # Action enum, AppEvent, poll_event, mouse_action
├── ui/
│   ├── mod.rs           # UI render entry, centered_rect utility
│   ├── workspace_list.rs # Main workspace tree view
│   ├── detail_view.rs   # Workspace detail overlay
│   ├── help_view.rs     # Help overlay (keyboard shortcuts)
│   ├── status_bar.rs    # Bottom status bar
│   ├── input_dialog.rs  # Text input dialog (create/delete worktree)
│   └── selection_dialog.rs # Selection dialog (session/layout picker)
├── workspace/
│   ├── mod.rs           # Module exports
│   ├── state.rs         # Workspace struct, WorkspaceStatus enum
│   ├── worktree.rs      # WorktreeInfo, detect_worktrees, scan_for_repositories
│   └── manager.rs       # WorktreeManager (create/remove worktrees, list branches)
├── zellij/
│   ├── mod.rs           # Module exports
│   └── actions.rs       # ZellijActions, ZellijMode, TabActionResult
└── notify/
    ├── mod.rs           # Module exports, socket_path()
    ├── protocol.rs      # NotifyMessage enum (Register/Status/Unregister)
    ├── client.rs        # send_notification()
    └── server.rs        # run_listener() - async UDS server
```

### Key Dependencies

- **ratatui** + **crossterm**: TUI framework
- **git2**: Git repository and worktree detection
- **clap**: CLI argument parsing with subcommands
- **tokio**: Async runtime for notification listener
- **tracing**: Structured logging to file
- **directories**: Cross-platform config/data paths
- **serde** + **toml**: Configuration serialization

### Data Flow

```
                    ┌─────────────────────┐
                    │   AI CLI Tools      │
                    │ (Claude/Kiro/etc)   │
                    └──────────┬──────────┘
                               │ notify command
                               ▼
┌─────────────────────────────────────────────────────────┐
│                    workspace-manager                     │
│                                                         │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐ │
│  │   notify/   │───▶│    app/     │───▶│     ui/     │ │
│  │  (UDS rx)   │    │   state.rs  │    │  (render)   │ │
│  └─────────────┘    └──────┬──────┘    └─────────────┘ │
│                            │                            │
│  ┌─────────────┐    ┌──────▼──────┐    ┌─────────────┐ │
│  │ workspace/  │◀───│   main.rs   │───▶│   zellij/   │ │
│  │  (scan)     │    │ (event loop)│    │  (actions)  │ │
│  └─────────────┘    └─────────────┘    └─────────────┘ │
└─────────────────────────────────────────────────────────┘
```

## Configuration

Config file: `~/.config/workspace-manager/config.toml`

```toml
search_paths = ["~/work", "~/ghq"]
max_scan_depth = 3
log_level = "info"
editor = "code"  # or "cursor", "vim", etc.

[zellij]
enabled = true
session_name = "main"  # Target session for external mode
tab_name_template = "{repo}/{branch}"
ai_command = "claude"  # Command used in layout templates

[worktree]
path_style = "Parallel"  # or "Ghq", "Subdirectory", "Custom"
default_remote = "origin"
```

## Code Conventions

- Use `anyhow::Result` for error handling in application code
- Prefer `tracing` macros over `println!` for logging
- Use `directories` crate for platform-specific paths
- Follow Rust 2021 edition idioms

## Testing

```bash
# Run all tests
cargo test

# Run specific test
cargo test test_name
```

## Integration with Claude Code

Add hooks to `~/.claude/settings.json`:

```json
{
  "hooks": {
    "SessionStart": [{
      "type": "command",
      "command": "workspace-manager notify register --project-path . --tool claude"
    }],
    "Stop": [{
      "type": "command",
      "command": "workspace-manager notify status $CLAUDE_SESSION_ID idle"
    }],
    "UserPromptSubmit": [{
      "type": "command",
      "command": "workspace-manager notify status $CLAUDE_SESSION_ID working"
    }],
    "SessionEnd": [{
      "type": "command",
      "command": "workspace-manager notify unregister"
    }]
  }
}
```

## Related Documentation

See [docs/CODEMAPS/](docs/CODEMAPS/) for detailed architecture documentation.

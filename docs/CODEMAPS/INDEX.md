# Workspace Manager - Architecture Index

**Last Updated:** 2025-01-26

## Overview

Workspace Manager is a Rust TUI application for managing multiple AI CLI workspaces (Claude Code, Kiro-CLI, OpenCode, Codex) with git worktree support and Zellij terminal multiplexer integration.

## Module Map

```
workspace-manager
├── app/           # Application core (state, config, events)
├── ui/            # Terminal UI components (ratatui)
├── workspace/     # Git repository and worktree management
├── zellij/        # Zellij terminal multiplexer integration
└── notify/        # Status notification system (UDS)
```

## Codemaps

| Module | Description | Key Types |
|--------|-------------|-----------|
| [app](app.md) | Application state and configuration | `AppState`, `Config`, `Action` |
| [ui](ui.md) | Terminal UI rendering | `render()`, `centered_rect()` |
| [workspace](workspace.md) | Git worktree management | `Workspace`, `WorktreeManager` |
| [zellij](zellij.md) | Zellij integration | `ZellijActions`, `ZellijMode` |
| [notify](notify.md) | Notification system | `NotifyMessage`, `run_listener()` |

## Data Flow

```
┌──────────────────────────────────────────────────────────────────┐
│                         main.rs                                   │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │                      Event Loop                              │ │
│  │  1. poll_event() -> AppEvent (Key/Mouse/Resize)             │ │
│  │  2. notify_rx.try_recv() -> AppEvent (Workspace updates)    │ │
│  │  3. Action::from(event) -> handle_action()                  │ │
│  │  4. terminal.draw() -> ui::render()                         │ │
│  └─────────────────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────────────┘
         │                    │                    │
         ▼                    ▼                    ▼
┌─────────────┐      ┌─────────────┐      ┌─────────────┐
│   app/      │      │ workspace/  │      │   zellij/   │
│             │      │             │      │             │
│ AppState    │◀────▶│ Workspace   │      │ ZellijActions│
│ Config      │      │ Manager     │      │ TabActionResult│
│ Action      │      │ WorktreeInfo│      │             │
└─────────────┘      └─────────────┘      └─────────────┘
         │
         ▼
┌─────────────┐
│   ui/       │
│             │
│ render()    │
│ dialogs     │
│ views       │
└─────────────┘
```

## Key Entry Points

### Binary: `workspace-manager`

```
workspace-manager [OPTIONS] [COMMAND]

COMMANDS:
  tui      Start the TUI (default)
  daemon   Start the MCP daemon server (Phase 2 - not implemented)
  notify   Send a notification to the daemon
    register   Register a new workspace session
    status     Update workspace status
    unregister Unregister a workspace session

OPTIONS:
  -l, --log-level <LEVEL>  Log level [default: info]
```

### Configuration Files

| File | Purpose |
|------|---------|
| `~/.config/workspace-manager/config.toml` | Main configuration |
| `~/.config/workspace-manager/layouts/*.kdl` | Zellij layout templates |
| `~/{data_dir}/workspace-manager/workspace-manager.log` | Log file |
| `~/{runtime_dir}/workspace-manager/notify.sock` | UDS notification socket |

## External Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| ratatui | 0.29 | Terminal UI framework |
| crossterm | 0.28 | Terminal manipulation |
| tokio | 1.36 | Async runtime |
| git2 | 0.19 | Git operations |
| clap | 4.5 | CLI argument parsing |
| serde | 1.0 | Serialization |
| toml | 0.8 | Config file format |
| directories | 5.0 | Platform-specific paths |
| tracing | 0.1 | Structured logging |
| anyhow | 1.0 | Error handling |
| uuid | 1.7 | Session IDs |

## Development Status

- [x] Phase 1: Basic TUI with git worktree scanning
- [x] Phase 1.5: Notification system via UDS
- [x] Phase 2: Worktree management (create/delete)
- [x] Phase 3: Zellij integration (internal + external modes)
- [ ] Phase 4: SQLite persistence (planned)

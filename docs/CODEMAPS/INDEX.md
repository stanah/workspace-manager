# Workspace Manager - Architecture Index

**Last Updated:** 2026-01-30

## Overview

Workspace Manager is a Rust TUI application for managing multiple AI CLI workspaces (Claude Code, Kiro-CLI, OpenCode, Codex) with git worktree support, Zellij terminal multiplexer integration, and real-time session status tracking.

## Module Map

```
workspace-manager
├── app/              # Application core (state, config, events)
├── ui/               # Terminal UI components (ratatui)
├── workspace/        # Git repository, worktree, and session management
├── zellij/           # Zellij terminal multiplexer integration
├── notify/           # Status notification system (UDS)
└── logwatch/         # AI CLI status tracking (polling + AI analysis)

zellij-tab-sync/      # Zellij plugin (separate wasm32-wasip1 crate)
```

## Codemaps

| Module | Description | Key Types |
|--------|-------------|-----------|
| [app](app.md) | Application state and configuration | `AppState`, `Config`, `Action` |
| [ui](ui.md) | Terminal UI rendering | `render()`, `centered_rect()` |
| [workspace](workspace.md) | Git worktree and session management | `Workspace`, `WorktreeManager`, `Session` |
| [zellij](zellij.md) | Zellij integration | `ZellijActions`, `ZellijMode` |
| [notify](notify.md) | Notification system | `NotifyMessage`, `run_listener()` |
| [logwatch](logwatch.md) | AI CLI status tracking | `ClaudeSessionsFetcher`, `KiroSqliteFetcher` |
| [zellij-tab-sync](zellij-tab-sync.md) | Zellij tab focus plugin | `ZellijTabSync` |

## Data Flow

```
┌──────────────────────────────────────────────────────────────────┐
│                         main.rs                                   │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │                      Event Loop                              │ │
│  │  1. poll_event() -> AppEvent (Key/Mouse/Resize)             │ │
│  │  2. notify_rx.try_recv() -> AppEvent (Workspace/Tab focus)  │ │
│  │  3. Action::from(event) -> handle_action()                  │ │
│  │  4. Periodic: poll Claude/Kiro sessions                     │ │
│  │  5. terminal.draw() -> ui::render()                         │ │
│  └─────────────────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────────────┘
         │                    │                    │
         ▼                    ▼                    ▼
┌─────────────┐      ┌─────────────┐      ┌─────────────┐
│   app/      │      │ workspace/  │      │   zellij/   │
│             │      │             │      │             │
│ AppState    │◀────▶│ Workspace   │      │ ZellijActions│
│ Config      │      │ Session     │      │ TabActionResult│
│ Action      │      │ Manager     │      │             │
└─────────────┘      └─────────────┘      └─────────────┘
         │                    ▲
         ▼                    │
┌─────────────┐      ┌─────────────┐
│   ui/       │      │  logwatch/  │
│             │      │             │
│ render()    │      │ Claude poll │
│ dialogs     │      │ Kiro SQLite │
│ views       │      │ LogAnalyzer │
└─────────────┘      └─────────────┘
```

## Key Entry Points

### Binary: `workspace-manager`

```
workspace-manager [OPTIONS] [COMMAND]

COMMANDS:
  tui            Start the TUI (default)
  notify         Send a notification to the TUI
    register       Register a new workspace session
    status         Update workspace status
    unregister     Unregister a workspace session
    tab-focus      Notify tab focus change (from Zellij plugin)
  setup-plugin   Build and install zellij-tab-sync plugin

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
| serde_json | 1.0 | JSON parsing |
| toml | 0.8 | Config file format |
| directories | 5.0 | Platform-specific paths |
| tracing | 0.1 | Structured logging |
| anyhow | 1.0 | Error handling |
| uuid | 1.7 | Session IDs |
| notify | 7.0 | File watching (logwatch) |
| chrono | 0.4 | DateTime handling (logwatch) |
| rusqlite | 0.32 | SQLite access (Kiro status) |

## Development Status

- [x] Phase 1: Basic TUI with git worktree scanning
- [x] Phase 1.5: Notification system via UDS
- [x] Phase 2: Worktree management (create/delete)
- [x] Phase 3: Zellij integration (internal + external modes)
- [x] Phase 3.5: Zellij tab-sync plugin
- [x] Phase 4: Multi-session tracking (Claude + Kiro)
- [x] Phase 4.5: Process detection and logwatch polling
- [ ] Phase 5: SQLite persistence (planned)

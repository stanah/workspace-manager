# Workspace Manager

TUI application for managing multiple Claude Code/Kiro-CLI workspaces.

## Features

- **Git worktree detection**: Automatically scans and displays git repositories and worktrees
- **Real-time status tracking**: Shows workspace status (idle, working, needs input, etc.)
- **Zellij integration**: Focus panes, launch tools (lazygit, yazi, shell)
- **MCP server**: Receives status updates from Claude Code via hooks

## Installation

```bash
cargo install --path .
```

## Usage

```bash
# Start TUI (default)
workspace-manager

# With debug logging
workspace-manager --log-level debug
```

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `j` / `↓` | Move down |
| `k` / `↑` | Move up |
| `Enter` | Focus workspace / Show details |
| `r` | Refresh workspace list |
| `?` | Toggle help |
| `q` | Quit |

### Zellij Actions (when running inside Zellij)

| Key | Action |
|-----|--------|
| `l` | Launch lazygit |
| `g` | Launch shell |
| `y` | Launch yazi |
| `n` | New Claude Code session |
| `x` | Close workspace |

## Development Phases

- [x] **Phase 1**: Basic TUI with git worktree detection
- [ ] **Phase 2**: MCP server for Claude Code integration
- [ ] **Phase 3**: Zellij pane management
- [ ] **Phase 4**: SQLite persistence and configuration

## Architecture

```
workspace-manager/
├── src/
│   ├── main.rs          # Entry point, CLI
│   ├── app/             # Application state and events
│   ├── ui/              # TUI components
│   ├── workspace/       # Workspace and worktree management
│   └── zellij/          # Zellij CLI wrapper
└── hooks/               # Claude Code hook scripts
```

## License

MIT

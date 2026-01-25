# Workspace Manager

TUI application for managing multiple Claude Code/Kiro-CLI workspaces with git worktree support and Zellij integration.

## Features

- **Git worktree detection**: Automatically scans and displays git repositories and worktrees
- **Worktree management**: Create and delete worktrees directly from the TUI
- **Branch browsing**: View local and remote branches, create worktrees from branches
- **Real-time status tracking**: Shows workspace status (idle, working, needs input, etc.)
- **Zellij integration**: Two modes supported:
  - **Internal mode**: Run inside Zellij to focus panes and launch tools
  - **External mode**: Run outside Zellij to manage tabs in a target session
- **Notification system**: Receives status updates from AI CLI tools via Unix Domain Socket

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

# Send notifications (for AI CLI integration)
workspace-manager notify register --session-id $SESSION_ID --project-path .
workspace-manager notify status $SESSION_ID working
workspace-manager notify unregister --session-id $SESSION_ID
```

## Keyboard Shortcuts

### Navigation

| Key | Action |
|-----|--------|
| `j` / `Down` | Move down |
| `k` / `Up` | Move up |
| `Enter` | Open workspace tab / Focus pane |
| `Space` | Expand/collapse repository group |
| `v` | Cycle display mode (Worktrees / +Local / +All branches) |
| `Tab` | Open with layout selection |
| `r` | Refresh workspace list |
| `Esc` | Close overlay / Go back |
| `?` | Toggle help |
| `q` / `Ctrl+c` | Quit |

### Worktree Management

| Key | Action |
|-----|--------|
| `c` / `a` | Create new worktree (from branch or new) |
| `d` | Delete selected worktree |

### Zellij Actions

| Key | Action |
|-----|--------|
| `l` | Launch lazygit |
| `g` | Launch shell |
| `y` | Launch yazi |
| `n` | New AI CLI session |
| `x` / `Backspace` | Close workspace (tab or pane) |

### Other

| Key | Action |
|-----|--------|
| `e` | Open workspace in editor |

### Mouse Support

- **Click**: Select item
- **Double-click**: Open workspace
- **Middle-click**: Close workspace
- **Scroll**: Navigate list

## Configuration

Configuration file: `~/.config/workspace-manager/config.toml`

```toml
# Directories to scan for git repositories
search_paths = ["~/work", "~/ghq"]
max_scan_depth = 3
log_level = "info"

# Editor command for 'e' key
editor = "code"

[zellij]
enabled = true
# Target session name (required for external mode)
session_name = "main"
# Tab naming template
tab_name_template = "{repo}/{branch}"
# AI command for layouts (claude, kiro-cli, opencode, codex)
ai_command = "claude"

[worktree]
# Path style: "Parallel", "Ghq", "Subdirectory", or Custom("template")
path_style = "Parallel"
default_remote = "origin"
```

### Worktree Path Styles

- **Parallel**: `{repo_parent}/{repo}={branch}` (e.g., `~/work/myrepo=feature`)
- **Ghq**: `{ghq_root}/{host}/{owner}/{repo}={branch}`
- **Subdirectory**: `{repo}/.worktrees/{branch}`
- **Custom**: User-defined template with `{repo}`, `{branch}`, `{repo_path}` placeholders

## AI CLI Integration

To receive status updates from Claude Code, add hooks to `~/.claude/settings.json`:

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

## Architecture

```
workspace-manager/
├── src/
│   ├── main.rs          # Entry point, CLI, event loop
│   ├── app/             # Application state, config, events
│   ├── ui/              # Ratatui TUI components
│   ├── workspace/       # Git worktree scanning and management
│   ├── zellij/          # Zellij CLI wrapper
│   └── notify/          # UDS notification server/client
├── layouts/             # Zellij layout templates
└── docs/CODEMAPS/       # Architecture documentation
```

See [CLAUDE.md](CLAUDE.md) for detailed development documentation.

## License

MIT

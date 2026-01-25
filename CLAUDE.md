# CLAUDE.md

This file provides guidance to Claude Code when working with this repository.

## Overview

Workspace Manager is a Rust TUI application for managing multiple Claude Code/Kiro-CLI workspaces. It uses MCP (Model Context Protocol) to receive status updates from Claude Code instances and displays them in a unified dashboard.

## Key Commands

```bash
# Build
cargo build

# Run
cargo run

# Run with debug logging
cargo run -- --log-level debug

# Run tests
cargo test
```

## Architecture

### Module Structure

- `src/main.rs` - Entry point, CLI parsing, TUI loop
- `src/app/` - Application state, configuration, event handling
- `src/ui/` - Ratatui-based UI components
- `src/workspace/` - Workspace state and git worktree detection
- `src/zellij/` - Zellij CLI wrapper for pane management

### Key Dependencies

- **ratatui** + **crossterm**: TUI framework
- **git2**: Git repository and worktree detection
- **clap**: CLI argument parsing
- **tokio**: Async runtime (for Phase 2 MCP server)
- **tracing**: Structured logging

### Development Phases

1. **Phase 1** (Current): Basic TUI with git worktree scanning
2. **Phase 2**: MCP server via Unix Domain Socket
3. **Phase 3**: Zellij pane integration
4. **Phase 4**: SQLite persistence, configuration

## Code Conventions

- Use `anyhow::Result` for error handling in application code
- Use `thiserror` for library-level error types
- Prefer `tracing` macros over `println!` for logging
- Follow Rust 2021 edition idioms

## Testing

```bash
# Run all tests
cargo test

# Run specific test
cargo test test_name
```

## Integration with Claude Code

Phase 2 will add Claude Code hooks to `~/.claude/settings.json`:

```json
{
  "hooks": {
    "SessionStart": [...],
    "Notification": [...],
    "UserPromptSubmit": [...],
    "Stop": [...],
    "SessionEnd": [...]
  }
}
```

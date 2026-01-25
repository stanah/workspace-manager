# Notify Module Codemap

**Last Updated:** 2025-01-26
**Location:** `src/notify/`

## Overview

The notify module implements a Unix Domain Socket (UDS) based notification system for receiving real-time status updates from AI CLI tools (Claude Code, Kiro-CLI, etc.).

## Structure

```
src/notify/
├── mod.rs       # Module exports, socket_path()
├── protocol.rs  # NotifyMessage enum
├── client.rs    # send_notification()
└── server.rs    # run_listener()
```

## Key Types

### NotifyMessage (protocol.rs)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NotifyMessage {
    /// Register a new workspace session
    Register {
        session_id: String,
        project_path: String,
        tool: Option<String>,  // "claude", "kiro", etc.
    },
    /// Update workspace status
    Status {
        session_id: String,
        status: String,        // "working", "idle"
        message: Option<String>,
    },
    /// Unregister a workspace session
    Unregister {
        session_id: String,
    },
}
```

## Key Functions

### socket_path (mod.rs)

```rust
pub fn socket_path() -> PathBuf
```

Returns the UDS path for notifications:
- Primary: `{runtime_dir}/workspace-manager/notify.sock`
- Fallback: `{data_dir}/workspace-manager/notify.sock`
- Last resort: `/tmp/workspace-manager/notify.sock`

Uses `directories::ProjectDirs` for platform-specific paths.

### send_notification (client.rs)

```rust
pub fn send_notification(socket_path: &Path, message: &NotifyMessage) -> Result<()>
```

Sends a notification to the running TUI:
1. Connect to UDS
2. Serialize message as JSON
3. Write with newline delimiter
4. Close connection

### run_listener (server.rs)

```rust
pub async fn run_listener(
    socket_path: &Path,
    tx: Sender<AppEvent>,
) -> Result<()>
```

Async listener that:
1. Creates/binds UDS at socket_path
2. Accepts connections
3. Reads JSON messages (newline-delimited)
4. Converts to AppEvent and sends via channel
5. Handles multiple concurrent connections

## Message to Event Conversion

| NotifyMessage | AppEvent |
|---------------|----------|
| `Register { session_id, project_path, tool }` | `WorkspaceRegister { session_id, project_path, pane_id: None }` |
| `Status { session_id, status: "working", .. }` | `WorkspaceUpdate { status: Working, .. }` |
| `Status { session_id, status: "idle", .. }` | `WorkspaceUpdate { status: Idle, .. }` |
| `Status { session_id, status: "needs_input", .. }` | `WorkspaceUpdate { status: NeedsInput, .. }` |
| `Unregister { session_id }` | `WorkspaceUnregister { session_id }` |

## Data Flow

```
AI CLI Tool                     workspace-manager TUI
     │                                   │
     │  workspace-manager notify         │
     │  register --session-id X          │
     │  --project-path /path             │
     │         │                         │
     │         ▼                         │
     │  send_notification()              │
     │         │                         │
     │         │    UDS Connection       │
     │         └────────────────────────▶│
     │                                   │ run_listener()
     │                                   │      │
     │                                   │      ▼
     │                                   │ Parse JSON
     │                                   │      │
     │                                   │      ▼
     │                                   │ AppEvent::WorkspaceRegister
     │                                   │      │
     │                                   │      ▼
     │                                   │ tx.send(event)
     │                                   │      │
     │                                   │      ▼
     │                                   │ main loop receives
     │                                   │ via notify_rx
     │                                   │      │
     │                                   │      ▼
     │                                   │ handle_notify_event()
     │                                   │      │
     │                                   │      ▼
     │                                   │ Updates AppState
```

## CLI Usage

```bash
# Register workspace (typically on session start)
workspace-manager notify register \
    --session-id $CLAUDE_SESSION_ID \
    --project-path . \
    --tool claude

# Update status to working
workspace-manager notify status \
    $CLAUDE_SESSION_ID working

# Update status to idle with message
workspace-manager notify status \
    $CLAUDE_SESSION_ID idle \
    --message "Task completed"

# Unregister on session end
workspace-manager notify unregister \
    --session-id $CLAUDE_SESSION_ID
```

## Integration Example

Claude Code hooks (`~/.claude/settings.json`):

```json
{
  "hooks": {
    "SessionStart": [{
      "type": "command",
      "command": "workspace-manager notify register --project-path . --tool claude"
    }],
    "UserPromptSubmit": [{
      "type": "command",
      "command": "workspace-manager notify status $CLAUDE_SESSION_ID working"
    }],
    "Stop": [{
      "type": "command",
      "command": "workspace-manager notify status $CLAUDE_SESSION_ID idle"
    }],
    "SessionEnd": [{
      "type": "command",
      "command": "workspace-manager notify unregister"
    }]
  }
}
```

## Error Handling

- **Socket not found**: Client silently succeeds (TUI not running)
- **Connection failed**: Warning printed, operation continues
- **Parse error**: Logged, connection continues
- **Socket cleanup**: Removed on TUI exit

## Exports

```rust
pub use client::send_notification;
pub use protocol::NotifyMessage;
pub use server::run_listener;
pub fn socket_path() -> PathBuf;
```

## Related Modules

- [app](app.md) - Defines AppEvent variants for notifications
- main.rs - Spawns listener and handles events

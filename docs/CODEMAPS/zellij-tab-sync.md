# Zellij Tab Sync Plugin Codemap

**Last Updated:** 2026-01-30
**Location:** `zellij-tab-sync/`

## Overview

A Zellij WebAssembly plugin (wasm32-wasip1) that monitors tab focus changes and notifies the workspace-manager TUI via the notify system. This enables auto-selection of the focused workspace when switching Zellij tabs.

## Structure

```
zellij-tab-sync/
├── Cargo.toml    # Separate crate targeting wasm32-wasip1
└── src/
    └── main.rs   # ZellijTabSync plugin implementation
```

## Key Types

### ZellijTabSync

```rust
#[derive(Default)]
struct ZellijTabSync {
    prev_active_tab: Option<String>,
}
```

Implements `ZellijPlugin` trait:

| Method | Description |
|--------|-------------|
| `load()` | Subscribe to `TabUpdate` events, request permissions |
| `update(event)` | On tab change, run `workspace-manager notify tab-focus <name>` |

## Plugin Lifecycle

```
Zellij Session
    │
    ├── load()
    │   ├── subscribe(&[EventType::TabUpdate])
    │   └── request_permission(&[ReadApplicationState, RunCommands])
    │
    └── update(Event::TabUpdate(tabs))
        │
        ├── Find active tab
        ├── Compare with prev_active_tab
        │   └── If changed:
        │       ├── Update prev_active_tab
        │       └── run_command(&["workspace-manager", "notify", "tab-focus", &tab_name])
        │
        └── Returns false (no UI render needed)
```

## Integration with workspace-manager

```
Zellij Tab Switch
        │
        ▼
ZellijTabSync plugin
        │
        ▼
workspace-manager notify tab-focus <tab_name>
        │
        ▼
NotifyMessage::TabFocus { tab_name }
        │
        ▼
AppEvent (via UDS)
        │
        ▼
main.rs: select_by_tab_name()
```

## Build & Installation

```bash
# Build plugin (requires wasm32-wasip1 target)
cargo build -p zellij-tab-sync --target wasm32-wasip1 --release

# Auto-setup (build, install, load)
workspace-manager setup-plugin
```

The `setup-plugin` command:
1. Builds the plugin for `wasm32-wasip1`
2. Copies to `~/.config/zellij/plugins/`
3. Loads the plugin in the current Zellij session

## Dependencies

| Crate | Purpose |
|-------|---------|
| zellij-tile | Zellij plugin SDK |

## Permissions

| Permission | Purpose |
|------------|---------|
| `ReadApplicationState` | Access tab information |
| `RunCommands` | Execute workspace-manager notify |

## Related Modules

- [notify](notify.md) - Receives `TabFocus` messages from this plugin
- [zellij](zellij.md) - Main Zellij integration in workspace-manager
- [app](app.md) - `select_by_tab_name()` handles the focus event

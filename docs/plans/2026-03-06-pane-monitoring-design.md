# Pane Monitoring Design

## Overview

Extend workspace-manager from session-oriented (AI CLI only) to pane-oriented monitoring. Each pane in tmux/zellij is tracked per-workspace, with AI sessions displayed as enriched pane entries.

## Requirements

- Monitor all panes across tmux sessions via polling
- Map panes to workspaces by CWD (longest-prefix match)
- Replace `TreeItem::Session` with `TreeItem::Pane`
- AI panes retain existing rich status display (tool icon, status, summary, elapsed time)
- Non-AI panes show command name in DarkGray
- Double-click on pane: switch to window + focus pane
- Panes with CWD outside known workspaces are hidden
- tmux first; zellij deferred

## Data Model

### Pane struct (replaces Session)

```rust
pub struct Pane {
    pub pane_id: String,              // tmux: "%12"
    pub workspace_index: usize,       // parent workspace reference
    pub window_name: String,          // tab/window name
    pub window_index: u32,            // tab/window index
    pub cwd: PathBuf,                 // current working directory
    pub command: String,              // running command (zsh, claude, vim)
    pub is_active: bool,              // active pane flag
    pub session_name: String,         // tmux session name
    pub ai_session: Option<AiSessionInfo>,
}

pub struct AiSessionInfo {
    pub tool: AiTool,
    pub status: SessionStatus,
    pub state_detail: Option<String>,
    pub summary: Option<String>,
    pub current_task: Option<String>,
    pub last_activity: Option<SystemTime>,
    pub external_id: Option<String>,  // from notify registration
}
```

### AppState changes

```
sessions: Vec<Session>                          -> panes: Vec<Pane>
session_map: HashMap<String, usize>             -> pane_map: HashMap<String, usize>
sessions_by_workspace: HashMap<usize, Vec<usize>> -> panes_by_workspace: HashMap<usize, Vec<usize>>
```

### TreeItem changes

```
TreeItem::Session { session_index, is_last } -> TreeItem::Pane { pane_index, is_last }
```

### Workspace mapping

Pane CWD is matched against workspace `project_path` using longest-prefix match. If CWD is a subdirectory of a workspace path, the pane belongs to that workspace.

## Pane Information Retrieval

### tmux list-panes

```bash
tmux list-panes -a -F "#{session_name}\t#{window_index}\t#{window_name}\t#{pane_id}\t#{pane_current_path}\t#{pane_current_command}\t#{pane_active}\t#{pane_pid}"
```

Polled every 1-2 seconds, riding on the existing `tick_rx` interval.

### Multiplexer trait addition

```rust
fn list_all_panes(&self) -> Result<Vec<PaneInfo>>;

pub struct PaneInfo {
    pub session_name: String,
    pub window_index: u32,
    pub window_name: String,
    pub pane_id: String,
    pub cwd: PathBuf,
    pub command: String,
    pub is_active: bool,
    pub pid: u32,
}
```

### AI session detection

Pane `command` field determines AI tool type:
- `claude` -> Claude Code
- `kiro` -> Kiro
- `opencode` -> OpenCode
- `codex` -> Codex

Detected AI panes use existing logwatch (JSONL parsing) for detailed status. `pane_pid` can trace child process tree to identify AI session IDs.

### Polling flow

1. `tick_rx` fires (1-2 sec interval)
2. Call `mux.list_all_panes()` in background
3. Send `AppEvent::PaneScan(Vec<PaneInfo>)` to main loop
4. Diff against current panes: add new, remove gone, update CWD changes
5. Re-map panes to workspaces on CWD change

## UI Display

### Tree structure

```
workspace-manager (2)
  +-- * main [3 panes]
  |  +-- zsh                          <- normal shell (DarkGray)
  |  +-- * [working] Fix bug...       <- AI pane (rich display)
  |  \-- vim                          <- editor pane (DarkGray)
  \-- o feature-x [1 pane]
     \-- zsh
```

- Normal panes: command name only, DarkGray
- AI panes: status icon + state_detail + summary + elapsed time (same as current Session display)
- Pane count: `[N panes]` on worktree row (replaces `[N sessions]`)
- Aggregate status: AI pane statuses reflected on worktree row icon

### Double-click on Pane

1. `tmux select-window -t session:window_index`
2. `tmux select-pane -t %pane_id`
3. Run `post_select_command` if configured

### Double-click on Worktree (unchanged)

Switch to workspace tab/window as before.

## notify Compatibility

Existing notify Register/Status/Unregister messages continue to work. When a notify registration arrives with a `pane_id`, it updates the corresponding `Pane.ai_session` field. No changes required to AI CLI tool hook configurations.

## Error Handling

- `list_all_panes()` failure: retain previous pane data, show warning in status bar
- Pane disappears: immediately remove from pane list (no Disconnected state)
- AI detection failure: display as normal pane (graceful degradation)
- tmux not connected: skip polling, fall back to notify-only mode

## Implementation Phases

1. Add `list_all_panes()` to Multiplexer trait (tmux implementation only)
2. Add `Pane` struct, `PaneInfo`, and `TreeItem::Pane`; implement polling loop
3. Replace `Session` / `TreeItem::Session` with `Pane`-based display
4. Implement double-click pane focus (window switch + pane select)
5. Integrate with notify (map notify sessions to panes by pane_id)

## Future: zellij Support

- Investigate `zellij action dump-layout` for pane info extraction
- Add zellij implementation of `list_all_panes()` once API is understood
- No changes to Multiplexer trait interface needed

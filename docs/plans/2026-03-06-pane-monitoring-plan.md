# Pane Monitoring Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace session-oriented monitoring with pane-oriented monitoring, tracking all tmux panes per workspace and displaying AI session info as enriched pane entries.

**Architecture:** Add `PaneInfo` to Multiplexer trait and `Pane` struct to workspace module. Poll tmux `list-panes` on the existing 1-second tick. Replace `TreeItem::Session` with `TreeItem::Pane` for display. Existing notify/logwatch flows attach AI session info to matching panes.

**Tech Stack:** Rust, ratatui, tmux CLI, crossterm

---

### Task 1: Add `PaneInfo` struct and `list_all_panes()` to Multiplexer trait

**Files:**
- Modify: `src/multiplexer/mod.rs:1-10` (add PaneInfo struct)
- Modify: `src/multiplexer/mod.rs:32-95` (add trait method)

**Step 1: Add PaneInfo struct to multiplexer/mod.rs**

Add after the `use` block at the top of the file:

```rust
use std::path::PathBuf;

/// ペイン情報（マルチプレクサから取得）
#[derive(Debug, Clone)]
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

**Step 2: Add `list_all_panes()` to Multiplexer trait**

Add after the `new_pane()` method in the trait:

```rust
/// 全セッションの全ペイン情報を取得（ポーリング用）
fn list_all_panes(&self) -> Result<Vec<PaneInfo>> {
    Ok(Vec::new()) // デフォルト: 空リスト
}
```

**Step 3: Verify it compiles**

Run: `cargo build 2>&1`
Expected: Compiles with no errors (default impl means no breakage).

**Step 4: Commit**

```bash
git add src/multiplexer/mod.rs
git commit -m "feat: add PaneInfo struct and list_all_panes() to Multiplexer trait"
```

---

### Task 2: Implement `list_all_panes()` for tmux

**Files:**
- Modify: `src/multiplexer/tmux.rs:126-413` (add implementation)
- Test: manual `cargo run` verification

**Step 1: Add tmux implementation of `list_all_panes()`**

Add to the `impl Multiplexer for TmuxMultiplexer` block, after the `new_pane()` method:

```rust
fn list_all_panes(&self) -> Result<Vec<super::PaneInfo>> {
    let session = match self.resolve_session() {
        Ok(s) => s,
        Err(_) => return Ok(Vec::new()),
    };

    let output = Command::new("tmux")
        .args([
            "list-panes",
            "-s",          // all windows in session
            "-t", &session,
            "-F",
            "#{session_name}\t#{window_index}\t#{window_name}\t#{pane_id}\t#{pane_current_path}\t#{pane_current_command}\t#{pane_active}\t#{pane_pid}",
        ])
        .output()
        .context("Failed to list tmux panes")?;

    if !output.status.success() {
        return Ok(Vec::new());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let panes = stdout
        .lines()
        .filter(|line| !line.is_empty())
        .filter_map(|line| {
            let fields: Vec<&str> = line.split('\t').collect();
            if fields.len() < 8 {
                return None;
            }
            Some(super::PaneInfo {
                session_name: fields[0].to_string(),
                window_index: fields[1].parse().unwrap_or(0),
                window_name: fields[2].to_string(),
                pane_id: fields[3].to_string(), // e.g., "%12"
                cwd: PathBuf::from(fields[4]),
                command: fields[5].to_string(),
                is_active: fields[6] == "1",
                pid: fields[7].parse().unwrap_or(0),
            })
        })
        .collect();

    Ok(panes)
}
```

**Step 2: Add PathBuf import to tmux.rs if not present**

Verify `use std::path::Path;` is already imported. Add `PathBuf` usage if needed — note that `Path` is already imported; `PathBuf` is used via the full path `std::path::PathBuf` or add to import.

**Step 3: Verify it compiles**

Run: `cargo build 2>&1`
Expected: Compiles with no errors.

**Step 4: Commit**

```bash
git add src/multiplexer/tmux.rs
git commit -m "feat: implement list_all_panes() for tmux backend"
```

---

### Task 3: Add `Pane` and `AiSessionInfo` structs

**Files:**
- Create: `src/workspace/pane.rs`
- Modify: `src/workspace/mod.rs` (add module export)

**Step 1: Create `src/workspace/pane.rs`**

```rust
//! Pane tracking for terminal multiplexer integration
//!
//! A Pane represents a terminal pane tracked via multiplexer polling.
//! If the pane runs an AI CLI tool, it carries AiSessionInfo.

use std::path::PathBuf;
use std::time::SystemTime;

use super::session::{AiTool, SessionStatus};

/// AI セッション情報（ペイン内で AI ツールが動作している場合）
#[derive(Debug, Clone)]
pub struct AiSessionInfo {
    pub tool: AiTool,
    pub status: SessionStatus,
    pub state_detail: Option<String>,
    pub summary: Option<String>,
    pub current_task: Option<String>,
    pub last_activity: Option<SystemTime>,
    /// notify 経由の外部ID（logwatch連携用）
    pub external_id: Option<String>,
}

/// マルチプレクサのペイン（ワークスペースに紐付く）
#[derive(Debug, Clone)]
pub struct Pane {
    /// ペインID（tmux: "%12"）
    pub pane_id: String,
    /// 親ワークスペースへのインデックス
    pub workspace_index: usize,
    /// ウィンドウ/タブ名
    pub window_name: String,
    /// ウィンドウ/タブインデックス
    pub window_index: u32,
    /// 現在の作業ディレクトリ
    pub cwd: PathBuf,
    /// 実行中コマンド（zsh, claude, vim 等）
    pub command: String,
    /// アクティブペインフラグ
    pub is_active: bool,
    /// マルチプレクサセッション名
    pub session_name: String,
    /// プロセスID
    pub pid: u32,
    /// AI セッション情報（AI ツール動作中の場合）
    pub ai_session: Option<AiSessionInfo>,
}

impl Pane {
    /// コマンド名から AI ツールを検出
    pub fn detect_ai_tool(command: &str) -> Option<AiTool> {
        match command {
            "claude" => Some(AiTool::Claude),
            "kiro" => Some(AiTool::Kiro),
            "opencode" => Some(AiTool::OpenCode),
            "codex" => Some(AiTool::Codex),
            _ => None,
        }
    }

    /// AI ペインかどうか
    pub fn is_ai_pane(&self) -> bool {
        self.ai_session.is_some()
    }

    /// アクティブな AI セッションステータスを取得
    pub fn ai_status(&self) -> Option<SessionStatus> {
        self.ai_session.as_ref().map(|s| s.status)
    }

    /// 表示用情報文字列を取得
    pub fn display_info(&self) -> String {
        if let Some(ref ai) = self.ai_session {
            let mut parts = Vec::new();
            if let Some(ref detail) = ai.state_detail {
                parts.push(format!("[{}]", detail));
            }
            if let Some(ref summary) = ai.summary {
                parts.push(summary.clone());
            }
            if let Some(ref activity) = ai.last_activity {
                if let Ok(duration) = activity.elapsed() {
                    let secs = duration.as_secs();
                    let time_str = if secs < 60 {
                        format!("{}s ago", secs)
                    } else if secs < 3600 {
                        format!("{}m ago", secs / 60)
                    } else if secs < 86400 {
                        format!("{}h ago", secs / 3600)
                    } else {
                        format!("{}d ago", secs / 86400)
                    };
                    parts.push(format!("({})", time_str));
                }
            }
            parts.join(" ")
        } else {
            self.command.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_ai_tool() {
        assert_eq!(Pane::detect_ai_tool("claude"), Some(AiTool::Claude));
        assert_eq!(Pane::detect_ai_tool("kiro"), Some(AiTool::Kiro));
        assert_eq!(Pane::detect_ai_tool("zsh"), None);
        assert_eq!(Pane::detect_ai_tool("vim"), None);
    }

    #[test]
    fn test_display_info_normal_pane() {
        let pane = Pane {
            pane_id: "%1".to_string(),
            workspace_index: 0,
            window_name: "main".to_string(),
            window_index: 0,
            cwd: PathBuf::from("/tmp"),
            command: "zsh".to_string(),
            is_active: true,
            session_name: "main".to_string(),
            pid: 1234,
            ai_session: None,
        };
        assert_eq!(pane.display_info(), "zsh");
        assert!(!pane.is_ai_pane());
    }

    #[test]
    fn test_display_info_ai_pane() {
        let pane = Pane {
            pane_id: "%2".to_string(),
            workspace_index: 0,
            window_name: "main".to_string(),
            window_index: 0,
            cwd: PathBuf::from("/tmp"),
            command: "claude".to_string(),
            is_active: false,
            session_name: "main".to_string(),
            pid: 5678,
            ai_session: Some(AiSessionInfo {
                tool: AiTool::Claude,
                status: SessionStatus::Working,
                state_detail: Some("processing".to_string()),
                summary: Some("Fix bug".to_string()),
                current_task: None,
                last_activity: None,
                external_id: None,
            }),
        };
        assert!(pane.is_ai_pane());
        assert_eq!(pane.display_info(), "[processing] Fix bug");
    }
}
```

**Step 2: Add module to `src/workspace/mod.rs`**

Add after the existing module declarations:

```rust
pub mod pane;
```

And add to the `pub use` block:

```rust
pub use pane::{AiSessionInfo, Pane};
```

**Step 3: Run tests**

Run: `cargo test workspace::pane -v`
Expected: All 3 tests pass.

**Step 4: Commit**

```bash
git add src/workspace/pane.rs src/workspace/mod.rs
git commit -m "feat: add Pane and AiSessionInfo structs with AI tool detection"
```

---

### Task 4: Add `TreeItem::Pane` variant and pane state management to AppState

**Files:**
- Modify: `src/app/state.rs:56-91` (add TreeItem::Pane variant)
- Modify: `src/app/state.rs:94-133` (add pane fields to AppState)
- Modify: `src/app/state.rs:135-160` (update new() for pane fields)

**Step 1: Add `TreeItem::Pane` variant**

In the `TreeItem` enum, add after the `Session` variant:

```rust
/// ペイン（マルチプレクサのペイン）
Pane {
    pane_index: usize,
    is_last: bool,
    parent_is_last: bool,
},
```

**Step 2: Add pane fields to `AppState`**

Add after the `sessions_by_workspace` field:

```rust
/// 検出されたペイン一覧
pub panes: Vec<crate::workspace::Pane>,
/// pane_id -> pane index のマッピング
pane_map: HashMap<String, usize>,
/// workspace_index -> pane indices のマッピング
panes_by_workspace: HashMap<usize, Vec<usize>>,
```

**Step 3: Update `AppState::new()`**

Add the new fields to the constructor:

```rust
panes: Vec::new(),
pane_map: HashMap::new(),
panes_by_workspace: HashMap::new(),
```

**Step 4: Add pane management methods to AppState**

Add these methods to the `impl AppState` block (before the `// ===== Navigation =====` section):

```rust
// ===== Pane management =====

/// PaneInfo リストからペイン状態を更新（差分処理）
pub fn update_panes(&mut self, pane_infos: &[crate::multiplexer::PaneInfo]) {
    use crate::workspace::pane::{Pane, AiSessionInfo};

    let mut new_panes: Vec<Pane> = Vec::new();
    let mut new_pane_map: HashMap<String, usize> = HashMap::new();
    let mut new_panes_by_workspace: HashMap<usize, Vec<usize>> = HashMap::new();

    for info in pane_infos {
        // CWD からワークスペースを最長一致で検索
        let workspace_index = self.find_workspace_by_cwd(&info.cwd);
        let Some(workspace_index) = workspace_index else {
            continue; // 該当ワークスペースなし → 非表示
        };

        let pane_index = new_panes.len();

        // 既存ペインの AI セッション情報を引き継ぐ
        let prev_ai_session = self.pane_map
            .get(&info.pane_id)
            .and_then(|&idx| self.panes.get(idx))
            .and_then(|p| p.ai_session.clone());

        // AI ツール検出
        let ai_session = if let Some(tool) = Pane::detect_ai_tool(&info.command) {
            // 既存の AI セッション情報があれば引き継ぐ、なければ新規作成
            prev_ai_session.unwrap_or_else(|| AiSessionInfo {
                tool,
                status: crate::workspace::SessionStatus::Idle,
                state_detail: None,
                summary: None,
                current_task: None,
                last_activity: Some(std::time::SystemTime::now()),
                external_id: None,
            }).into()
        } else {
            None
        };

        new_panes.push(Pane {
            pane_id: info.pane_id.clone(),
            workspace_index,
            window_name: info.window_name.clone(),
            window_index: info.window_index,
            cwd: info.cwd.clone(),
            command: info.command.clone(),
            is_active: info.is_active,
            session_name: info.session_name.clone(),
            pid: info.pid,
            ai_session,
        });

        new_pane_map.insert(info.pane_id.clone(), pane_index);
        new_panes_by_workspace
            .entry(workspace_index)
            .or_default()
            .push(pane_index);
    }

    self.panes = new_panes;
    self.pane_map = new_pane_map;
    self.panes_by_workspace = new_panes_by_workspace;
}

/// CWD からワークスペースを最長一致で検索
fn find_workspace_by_cwd(&self, cwd: &std::path::Path) -> Option<usize> {
    let cwd_str = cwd.to_string_lossy();
    let mut best_match: Option<(usize, usize)> = None; // (index, path_len)

    for (idx, ws) in self.workspaces.iter().enumerate() {
        let ws_path = &ws.project_path;
        if cwd_str.starts_with(ws_path) {
            let len = ws_path.len();
            if best_match.map_or(true, |(_, best_len)| len > best_len) {
                best_match = Some((idx, len));
            }
        }
    }

    best_match.map(|(idx, _)| idx)
}

/// ワークスペースのペイン一覧を取得
pub fn panes_for_workspace(&self, workspace_index: usize) -> Vec<usize> {
    self.panes_by_workspace
        .get(&workspace_index)
        .cloned()
        .unwrap_or_default()
}

/// ワークスペースの集約ステータスを取得（ペインベース）
/// AI ペインがあればそのステータスを集約、なければ Disconnected
pub fn workspace_aggregate_status_from_panes(&self, workspace_index: usize) -> crate::workspace::SessionStatus {
    use crate::workspace::SessionStatus;

    let pane_indices = self.panes_for_workspace(workspace_index);
    if pane_indices.is_empty() {
        return SessionStatus::Disconnected;
    }

    let mut has_working = false;
    let mut has_needs_input = false;
    let mut has_idle = false;
    let mut has_any_ai = false;

    for &idx in &pane_indices {
        if let Some(pane) = self.panes.get(idx) {
            if let Some(status) = pane.ai_status() {
                has_any_ai = true;
                match status {
                    SessionStatus::Working => has_working = true,
                    SessionStatus::NeedsInput => has_needs_input = true,
                    SessionStatus::Idle | SessionStatus::Success => has_idle = true,
                    _ => {}
                }
            }
        }
    }

    if has_working {
        SessionStatus::Working
    } else if has_needs_input {
        SessionStatus::NeedsInput
    } else if has_idle {
        SessionStatus::Idle
    } else if has_any_ai {
        SessionStatus::Disconnected
    } else {
        SessionStatus::Disconnected
    }
}

/// ペインの AI セッション情報を外部IDで更新
pub fn update_pane_ai_session_by_external_id(
    &mut self,
    external_id: &str,
    updater: impl FnOnce(&mut crate::workspace::pane::AiSessionInfo),
) {
    for pane in &mut self.panes {
        if let Some(ref mut ai) = pane.ai_session {
            if ai.external_id.as_deref() == Some(external_id) {
                updater(ai);
                return;
            }
        }
    }
}
```

**Step 5: Update `selected_workspace()` to handle `TreeItem::Pane`**

In the `selected_workspace()` method, add a match arm:

```rust
Some(TreeItem::Pane { pane_index, .. }) => {
    self.panes.get(*pane_index).and_then(|p| {
        self.workspaces.get(p.workspace_index)
    })
}
```

**Step 6: Add import for `Pane` at the top of state.rs**

Update the workspace import to include `Pane`:

```rust
use crate::workspace::{
    AiTool, Session, SessionStatus, Workspace, WorktreeManager, get_default_search_paths,
    scan_for_repositories, Pane,
};
```

**Step 7: Verify it compiles**

Run: `cargo build 2>&1`
Expected: Compiles. There will be unused import warnings for `Pane` which is expected at this stage.

**Step 8: Commit**

```bash
git add src/app/state.rs
git commit -m "feat: add TreeItem::Pane variant and pane state management to AppState"
```

---

### Task 5: Add panes to tree building (`rebuild_tree_with_manager`)

**Files:**
- Modify: `src/app/state.rs:195-373` (update `rebuild_tree_with_manager`)

**Step 1: Update tree building to include panes under worktrees**

In `rebuild_tree_with_manager()`, replace the session insertion block (lines around 314-334) to insert **both** sessions and panes. Replace:

```rust
// このワークスペースのセッションを追加
let parent_last = is_last_in_group;
for (sess_idx_pos, &sess_idx) in workspace_sessions.iter().enumerate() {
    self.tree_items.push(TreeItem::Session {
        session_index: sess_idx,
        is_last: sess_idx_pos == workspace_sessions.len() - 1,
        parent_is_last: parent_last,
    });
}
```

With:

```rust
// このワークスペースのペインを追加（セッションより優先）
let workspace_panes = self.panes_for_workspace(ws_idx);
let parent_last = is_last_in_group;

if !workspace_panes.is_empty() {
    // ペインベース表示
    for (pane_idx_pos, &pane_idx) in workspace_panes.iter().enumerate() {
        self.tree_items.push(TreeItem::Pane {
            pane_index: pane_idx,
            is_last: pane_idx_pos == workspace_panes.len() - 1,
            parent_is_last: parent_last,
        });
    }
} else {
    // ペインがない場合は従来のセッション表示（フォールバック）
    for (sess_idx_pos, &sess_idx) in workspace_sessions.iter().enumerate() {
        self.tree_items.push(TreeItem::Session {
            session_index: sess_idx,
            is_last: sess_idx_pos == workspace_sessions.len() - 1,
            parent_is_last: parent_last,
        });
    }
}
```

**Step 2: Update worktree row count display**

In `rebuild_tree_with_manager()`, the existing `sessions_for_workspace` is used to determine `RunningOnly` filtering. Update the filter to also consider panes:

In the RunningOnly filter block (around line 204-209), change:

```rust
if self.list_display_mode == ListDisplayMode::RunningOnly {
    let sessions = self.sessions_for_workspace(idx);
    let panes = self.panes_for_workspace(idx);
    if sessions.is_empty() && panes.is_empty() {
        continue;
    }
}
```

**Step 3: Verify it compiles**

Run: `cargo build 2>&1`
Expected: Compiles with no errors.

**Step 4: Commit**

```bash
git add src/app/state.rs
git commit -m "feat: integrate panes into tree building with session fallback"
```

---

### Task 6: Add pane rendering to workspace_list UI

**Files:**
- Modify: `src/ui/workspace_list.rs:64-237` (add TreeItem::Pane rendering)

**Step 1: Update the worktree row to show pane count**

In `create_tree_row()`, in the `TreeItem::Worktree` arm, after the session count logic (around line 117-123), add pane count support:

```rust
// ペイン数またはセッション数を表示
let pane_count = state.panes_for_workspace(*workspace_index).len();
let session_count = state.sessions_for_workspace(*workspace_index).len();
let child_info = if pane_count > 0 {
    Some(format!(" [{} pane{}]", pane_count, if pane_count > 1 { "s" } else { "" }))
} else if session_count > 0 {
    Some(format!(" [{} session{}]", session_count, if session_count > 1 { "s" } else { "" }))
} else {
    None
};
```

Replace the existing `session_info` variable and its usage with `child_info`.

**Step 2: Update aggregate status to prefer pane-based**

In the worktree row rendering, update the aggregate status call:

```rust
let panes = state.panes_for_workspace(*workspace_index);
let aggregate_status = if !panes.is_empty() {
    state.workspace_aggregate_status_from_panes(*workspace_index)
} else {
    state.workspace_aggregate_status(*workspace_index)
};
```

**Step 3: Add `TreeItem::Pane` rendering**

Add a new match arm in `create_tree_row()` after the `TreeItem::Session` arm:

```rust
TreeItem::Pane {
    pane_index,
    is_last,
    parent_is_last,
} => {
    if let Some(pane) = state.panes.get(*pane_index) {
        let continuation = if *parent_is_last { "  " } else { "│ " };
        let branch_char = if *is_last { "└ " } else { "├ " };
        let tree_prefix = format!("{}{}", continuation, branch_char);

        if pane.is_ai_pane() {
            // AI ペイン: 従来の Session 表示と同じフォーマット
            let ai = pane.ai_session.as_ref().unwrap();
            let tool_icon = ai.tool.icon(state.use_nerd_font);
            let tool_color = ai.tool.color();
            let status_color = ai.status.color();
            let status_icon = ai.status.icon();
            let info = pane.display_info();

            let name_style = if is_selected {
                Style::default().add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            let spans = vec![
                Span::styled("  ", Style::default()),
                Span::styled(tree_prefix, Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{} ", tool_icon), Style::default().fg(tool_color)),
                Span::styled(format!("{} ", status_icon), Style::default().fg(status_color)),
                Span::styled(info, name_style.fg(Color::DarkGray)),
            ];

            Row::new(vec![Line::from(spans)]).height(1)
        } else {
            // 通常ペイン: コマンド名のみ、DarkGray
            let name_style = if is_selected {
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            };

            let spans = vec![
                Span::styled("  ", Style::default()),
                Span::styled(tree_prefix, Style::default().fg(Color::DarkGray)),
                Span::styled("  ", Style::default()), // アイコン分スペース
                Span::styled(pane.command.clone(), name_style),
            ];

            Row::new(vec![Line::from(spans)]).height(1)
        }
    } else {
        Row::new(vec![Line::from("    └ <invalid pane>")]).height(1)
    }
}
```

**Step 4: Verify it compiles**

Run: `cargo build 2>&1`
Expected: Compiles with no errors.

**Step 5: Commit**

```bash
git add src/ui/workspace_list.rs
git commit -m "feat: add pane rendering to workspace list UI"
```

---

### Task 7: Add pane polling to the main event loop

**Files:**
- Modify: `src/main.rs:665-688` (add pane polling in tick handler)

**Step 1: Add pane polling in the tick block**

In `run_app()`, inside the `if tick_count >= 10` block (1-second interval), after the tab query block, add:

```rust
// ペイン情報をポーリング
match mux.list_all_panes() {
    Ok(panes) if !panes.is_empty() => {
        tracing::debug!("Polled {} panes", panes.len());
        state.update_panes(&panes);
        state.rebuild_tree_with_manager(Some(worktree_manager));
    }
    Ok(_) => {
        // ペインなし（マルチプレクサ未接続等）
    }
    Err(e) => {
        tracing::debug!("Failed to poll panes: {}", e);
    }
}
```

**Step 2: Verify it compiles**

Run: `cargo build 2>&1`
Expected: Compiles with no errors.

**Step 3: Commit**

```bash
git add src/main.rs
git commit -m "feat: add pane polling to main event loop on 1-second tick"
```

---

### Task 8: Implement double-click pane focus (window switch + pane select)

**Files:**
- Modify: `src/main.rs:1143-1207` (update Action::Select for panes)
- Modify: `src/main.rs:1454-1460` (update MouseDoubleClick)
- Modify: `src/app/state.rs` (add `selected_pane()` method)

**Step 1: Add `selected_pane()` method to AppState**

Add to `impl AppState`:

```rust
/// 現在選択中のペインを取得
pub fn selected_pane(&self) -> Option<&crate::workspace::Pane> {
    match self.tree_items.get(self.selected_index) {
        Some(TreeItem::Pane { pane_index, .. }) => self.panes.get(*pane_index),
        _ => None,
    }
}
```

**Step 2: Update Action::Select handling in main.rs**

In `handle_action()`, in the `Action::Select` arm, add a check for pane selection **before** the existing workspace selection logic. At the start of `Action::Select`:

```rust
Action::Select => {
    // ペインが選択されている場合: タブ切替 + ペインフォーカス
    if let Some(pane) = state.selected_pane() {
        if mux.is_available() && mux.backend() == multiplexer::MultiplexerBackend::Tmux {
            let session = pane.session_name.clone();
            let window_index = pane.window_index;
            let pane_id = pane.pane_id.clone();

            // ウィンドウ切替
            let target = format!("{}:{}", session, window_index);
            if let Err(e) = mux.go_to_window(&session, &window_index.to_string()) {
                state.status_message = Some(format!("Failed to switch window: {}", e));
            } else {
                // ペインフォーカス (pane_id is like "%12")
                let pane_id_num: Option<u32> = pane_id.strip_prefix('%')
                    .and_then(|s| s.parse().ok());
                if let Some(id) = pane_id_num {
                    if let Err(e) = mux.focus_pane(id) {
                        state.status_message = Some(format!("Failed to focus pane: {}", e));
                    } else {
                        state.status_message = Some(format!("Focused pane {}", pane_id));
                        run_post_select_command(config);
                    }
                }
            }
        }
    } else if let Some(ws) = state.selected_workspace() {
        // 既存のワークスペース選択ロジック（変更なし）
        // ...
    }
}
```

Note: Keep the entire existing `if let Some(ws) = state.selected_workspace()` block as the `else if` branch.

**Step 3: Verify it compiles**

Run: `cargo build 2>&1`
Expected: Compiles with no errors.

**Step 4: Commit**

```bash
git add src/main.rs src/app/state.rs
git commit -m "feat: implement double-click pane focus with window switch"
```

---

### Task 9: Integrate notify/logwatch with pane system

**Files:**
- Modify: `src/main.rs:1006-1110` (update `handle_notify_event` for pane AI session linking)

**Step 1: Update `SessionStatusAnalyzed` handler to update pane AI sessions**

In `handle_notify_event()`, in the `AppEvent::SessionStatusAnalyzed` arm, after updating the session (existing code), add pane AI session update:

```rust
// Also update matching pane AI session
state.update_pane_ai_session_by_external_id(&external_id, |ai| {
    ai.summary = status.display_summary();
    ai.current_task = status.current_task.clone();
    ai.state_detail = Some(status.state_detail.label().to_string());
    ai.status = match status.status {
        crate::logwatch::StatusState::Working => crate::workspace::SessionStatus::Working,
        crate::logwatch::StatusState::Waiting => crate::workspace::SessionStatus::NeedsInput,
        crate::logwatch::StatusState::Completed => crate::workspace::SessionStatus::Success,
        crate::logwatch::StatusState::Error => crate::workspace::SessionStatus::Error,
        crate::logwatch::StatusState::Idle => crate::workspace::SessionStatus::Idle,
        crate::logwatch::StatusState::Disconnected => crate::workspace::SessionStatus::Disconnected,
    };
    if let Some(activity) = status.last_activity {
        ai.last_activity = activity
            .timestamp_millis()
            .try_into()
            .ok()
            .map(|millis: u64| {
                std::time::UNIX_EPOCH + std::time::Duration::from_millis(millis)
            });
    }
});
```

**Step 2: Update `SessionRegister` handler to link notify session to pane**

In the `SessionRegister` handler, after the session registration, try to link to a pane:

```rust
// Try to link session to a pane by matching project_path
for pane in &mut state.panes {
    if let Some(ref mut ai) = pane.ai_session {
        // Match by workspace path
        if let Some(ws) = state.workspaces.get(pane.workspace_index) {
            if ws.project_path == project_path && ai.external_id.is_none() {
                // Check if pane command matches tool
                let expected_cmd = match tool {
                    AiTool::Claude => "claude",
                    AiTool::Kiro => "kiro",
                    AiTool::OpenCode => "opencode",
                    AiTool::Codex => "codex",
                };
                if pane.command == expected_cmd {
                    ai.external_id = Some(external_id.clone());
                    break;
                }
            }
        }
    }
}
```

Note: Due to borrow checker constraints, this may need restructuring. If immutable borrow of `state.workspaces` conflicts with mutable borrow of `state.panes`, collect the workspace paths first, then iterate panes.

**Step 3: Verify it compiles**

Run: `cargo build 2>&1`
Expected: Compiles with no errors.

**Step 4: Commit**

```bash
git add src/main.rs
git commit -m "feat: integrate notify/logwatch with pane AI session tracking"
```

---

### Task 10: Update navigation helpers and help view

**Files:**
- Modify: `src/app/state.rs` (update `collapse`, `expand`, `selected_workspace_branch`, `selected_repo_path` to handle `TreeItem::Pane`)
- Modify: `src/ui/help_view.rs` (update help text)

**Step 1: Update all TreeItem match arms in state.rs**

Search for all `match` expressions on `TreeItem` and add `TreeItem::Pane` handling. Key locations:

In `expand()`: Add `Some(TreeItem::Pane { .. })` to the child items match arm (alongside `Session`, `Branch`).

In `collapse()`: Same — add alongside existing child items.

In `selected_workspace_branch()`: Add:
```rust
Some(TreeItem::Pane { pane_index, .. }) => {
    self.panes.get(*pane_index).and_then(|p| {
        self.workspaces.get(p.workspace_index).map(|ws| ws.branch.clone())
    })
}
```

In `selected_repo_path()`: Add:
```rust
Some(TreeItem::Pane { pane_index, .. }) => {
    self.panes.get(*pane_index).and_then(|p| {
        self.workspaces
            .get(p.workspace_index)
            .map(|ws| ws.project_path.clone())
    })
}
```

**Step 2: Update help view**

In `src/ui/help_view.rs`, the `p` key description already says "Add pane to current tab". No changes needed unless we want to add documentation about the new pane display. Leave as-is to keep changes minimal.

**Step 3: Verify it compiles**

Run: `cargo build 2>&1`
Expected: Compiles with no errors.

**Step 4: Run all tests**

Run: `cargo test`
Expected: All tests pass.

**Step 5: Commit**

```bash
git add src/app/state.rs src/ui/help_view.rs
git commit -m "feat: update navigation helpers to handle TreeItem::Pane"
```

---

### Task 11: Final integration test and cleanup

**Files:**
- All modified files

**Step 1: Full build and test**

Run: `cargo build 2>&1 && cargo test 2>&1`
Expected: Build succeeds, all tests pass.

**Step 2: Check for compiler warnings**

Run: `cargo build 2>&1 | grep warning`
Expected: No warnings (or only pre-existing ones).

**Step 3: Test with tmux manually**

Run inside a tmux session: `cargo run`
Expected:
- Panes appear under their respective worktrees
- AI panes (claude/kiro) show rich status info
- Normal panes show command name in gray
- Double-click on a pane switches to its window and focuses the pane

**Step 4: Commit final state if any cleanup needed**

```bash
git add -A
git commit -m "chore: cleanup and finalize pane monitoring integration"
```

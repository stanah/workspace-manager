# Yazi Integration Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** workspace-managerの選択変更をデバウンス付きで`ya` CLIに送り、yaziのファイルツリーを自動追従させる。

**Architecture:** AppState内にデバウンスタイマーを持ち、選択変更時にYaziCommandをセット。イベントループのpollタイムアウトをデッドラインに合わせ、発火時にfire-and-forgetで`ya emit`をspawnする。

**Tech Stack:** Rust, std::process::Command, std::time::Instant, serde/toml (config)

---

### Task 1: YaziConfig を config.rs に追加

**Files:**
- Modify: `src/app/config.rs`

**Step 1: YaziConfig 構造体を追加**

`src/app/config.rs` の末尾（`ZellijConfig` の `impl` ブロックの後）に以下を追加:

```rust
/// Yazi連携設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YaziConfig {
    /// Yazi連携を有効にするか
    #[serde(default)]
    pub enabled: bool,
    /// 選択変更後のデバウンス時間（ミリ秒）
    #[serde(default = "default_yazi_debounce_ms")]
    pub debounce_ms: u64,
}

fn default_yazi_debounce_ms() -> u64 {
    200
}

impl Default for YaziConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            debounce_ms: default_yazi_debounce_ms(),
        }
    }
}
```

**Step 2: Config 構造体に yazi フィールドを追加**

`Config` 構造体に以下を追加:

```rust
/// Yazi連携設定
#[serde(default)]
pub yazi: YaziConfig,
```

`Config::default()` の `Self { ... }` 内に追加:

```rust
yazi: YaziConfig::default(),
```

**Step 3: ビルド確認**

Run: `cargo build 2>&1 | tail -5`
Expected: ビルド成功

**Step 4: コミット**

```bash
git add src/app/config.rs
git commit -m "feat: add YaziConfig for yazi file manager integration"
```

---

### Task 2: YaziCommand と パス解決メソッドを state.rs に追加

**Files:**
- Modify: `src/app/state.rs`

**Step 1: use 文と YaziCommand enum を追加**

`src/app/state.rs` の先頭の use 文に `std::time::Instant` を追加。
ファイル上部（`ListDisplayMode` の後あたり）に以下を追加:

```rust
/// Yaziに送信するコマンド
#[derive(Debug, Clone)]
pub enum YaziCommand {
    /// ya emit cd <path>
    Cd(std::path::PathBuf),
    /// ya emit reveal <path>
    Reveal(std::path::PathBuf),
}
```

**Step 2: AppState に pending_yazi フィールドを追加**

`AppState` 構造体に以下を追加:

```rust
/// Yazi連携: デバウンス中のコマンド (発火時刻, コマンド)
pub pending_yazi: Option<(Instant, YaziCommand)>,
```

`AppState::new()` の初期化に追加:

```rust
pending_yazi: None,
```

**Step 3: パス解決メソッドを追加**

`AppState` の `impl` ブロックに以下のメソッドを追加:

```rust
/// 現在の選択からYaziコマンドを解決する
pub fn resolve_yazi_command(&self) -> Option<YaziCommand> {
    let selected = self.tree_items.get(self.selected_index)?;
    match selected {
        TreeItem::RepoGroup { path, .. } => {
            Some(YaziCommand::Cd(std::path::PathBuf::from(path)))
        }
        TreeItem::Worktree { workspace_index, .. } => {
            let ws = self.workspaces.get(*workspace_index)?;
            Some(YaziCommand::Cd(std::path::PathBuf::from(&ws.project_path)))
        }
        TreeItem::Session { session_index, .. } => {
            let session = self.sessions.get(*session_index)?;
            let ws = self.workspaces.get(session.workspace_index)?;
            Some(YaziCommand::Cd(std::path::PathBuf::from(&ws.project_path)))
        }
        TreeItem::Pane { pane_index, .. } => {
            let pane = self.panes.get(*pane_index)?;
            Some(YaziCommand::Reveal(pane.cwd.clone()))
        }
        _ => None,
    }
}

/// Yaziデバウンスタイマーをセットする
pub fn schedule_yazi(&mut self, debounce_ms: u64) {
    if let Some(cmd) = self.resolve_yazi_command() {
        let deadline = Instant::now() + std::time::Duration::from_millis(debounce_ms);
        self.pending_yazi = Some((deadline, cmd));
    }
}

/// Yaziデバウンスタイマーが発火可能か確認し、発火する
pub fn fire_yazi_if_ready(&mut self) {
    if let Some((deadline, _)) = &self.pending_yazi {
        if Instant::now() >= *deadline {
            if let Some((_, cmd)) = self.pending_yazi.take() {
                let args: Vec<String> = match &cmd {
                    YaziCommand::Cd(p) => vec![
                        "emit".to_string(),
                        "cd".to_string(),
                        p.to_string_lossy().to_string(),
                    ],
                    YaziCommand::Reveal(p) => vec![
                        "emit".to_string(),
                        "reveal".to_string(),
                        p.to_string_lossy().to_string(),
                    ],
                };
                match std::process::Command::new("ya").args(&args).spawn() {
                    Ok(_) => {
                        tracing::debug!("Sent yazi command: ya {}", args.join(" "));
                    }
                    Err(e) => {
                        tracing::debug!("Failed to send yazi command: {}", e);
                    }
                }
            }
        }
    }
}

/// Yaziデバウンスのデッドラインまでの残り時間を返す
pub fn yazi_timeout(&self) -> Option<std::time::Duration> {
    self.pending_yazi.as_ref().map(|(deadline, _)| {
        deadline.saturating_duration_since(Instant::now())
    })
}
```

**Step 4: ビルド確認**

Run: `cargo build 2>&1 | tail -5`
Expected: ビルド成功

**Step 5: コミット**

```bash
git add src/app/state.rs
git commit -m "feat: add YaziCommand enum and debounce logic to AppState"
```

---

### Task 3: イベントループにデバウンス処理と選択フックを統合

**Files:**
- Modify: `src/main.rs`
- Modify: `src/app/mod.rs` (YaziCommand の re-export が必要な場合)

**Step 1: mod.rs で YaziCommand をエクスポート**

`src/app/mod.rs` を確認し、必要なら `YaziCommand` を pub use に追加。

**Step 2: run_app にYaziConfig を渡す**

`run_tui()` 内の `state` 初期化後に以下を追加:

```rust
let yazi_config = config.yazi.clone();
```

**Step 3: イベントループの poll タイムアウトをデバウンスに合わせる**

`run_app()` 内の `let mut has_event = poll_event(Duration::from_millis(100))?;` の行を以下に変更:

```rust
let poll_timeout = if yazi_config.enabled {
    state.yazi_timeout()
        .map(|d| d.min(Duration::from_millis(100)))
        .unwrap_or(Duration::from_millis(100))
} else {
    Duration::from_millis(100)
};
let mut has_event = poll_event(poll_timeout)?;
```

**Step 4: イベントループ内でデバウンスタイマーを発火**

poll_event の直後（`while let Some(event) = has_event {` の前）に追加:

```rust
if yazi_config.enabled {
    state.fire_yazi_if_ready();
}
```

**Step 5: 選択変更時にデバウンスをスケジュール**

`handle_action()` 内の `Action::MoveUp`、`Action::MoveDown`、`Action::MouseSelect`、`Action::ScrollUp`、`Action::ScrollDown` の各分岐の後でyaziスケジュールを行う。

`handle_action()` のシグネチャに `yazi_config: &YaziConfig` を追加する代わりに、`run_app()` 側で `handle_action()` 呼び出し後に選択変更を検出してスケジュールする方がシンプル。

`run_app()` 内のイベント処理部分で、`handle_action()` の前後で `selected_index` の変化を検出:

```rust
AppEvent::Key(key) => {
    let prev_index = state.selected_index;
    let action = Action::from(key);
    handle_action(state, mux, config, worktree_manager, action)?;
    if yazi_config.enabled && state.selected_index != prev_index {
        state.schedule_yazi(yazi_config.debounce_ms);
    }
}
AppEvent::Mouse(mouse) => {
    let prev_index = state.selected_index;
    // ... existing double-click logic ...
    handle_action(state, mux, config, worktree_manager, action)?;
    if yazi_config.enabled && state.selected_index != prev_index {
        state.schedule_yazi(yazi_config.debounce_ms);
    }
}
```

**Step 6: ビルド確認**

Run: `cargo build 2>&1 | tail -5`
Expected: ビルド成功

**Step 7: 手動テスト**

1. `~/.config/workspace-manager/config.toml` に `[yazi]` セクションを追加:
   ```toml
   [yazi]
   enabled = true
   debounce_ms = 200
   ```
2. Ghosttyで上下ペイン分割
3. 下のペインで `yazi` を起動
4. 上のペインで `cargo run` を起動
5. j/k で選択を移動し、yaziが追従することを確認

**Step 8: コミット**

```bash
git add src/main.rs src/app/mod.rs
git commit -m "feat: integrate yazi debounce into event loop with selection tracking"
```

---

### Task 4: テスト追加

**Files:**
- Modify: `src/app/state.rs` (テストモジュール追加)

**Step 1: state.rs にユニットテストを追加**

```rust
#[cfg(test)]
mod yazi_tests {
    use super::*;

    #[test]
    fn test_resolve_yazi_command_empty_tree() {
        let state = AppState::new();
        assert!(state.resolve_yazi_command().is_none());
    }

    #[test]
    fn test_schedule_yazi_sets_pending() {
        let mut state = AppState::new();
        // pending_yazi is None when no tree items
        state.schedule_yazi(200);
        assert!(state.pending_yazi.is_none());
    }

    #[test]
    fn test_yazi_timeout_none_when_no_pending() {
        let state = AppState::new();
        assert!(state.yazi_timeout().is_none());
    }

    #[test]
    fn test_yazi_timeout_returns_duration_when_pending() {
        let mut state = AppState::new();
        let deadline = Instant::now() + std::time::Duration::from_millis(500);
        state.pending_yazi = Some((deadline, YaziCommand::Cd(std::path::PathBuf::from("/tmp"))));
        let timeout = state.yazi_timeout();
        assert!(timeout.is_some());
        assert!(timeout.unwrap() <= std::time::Duration::from_millis(500));
    }
}
```

**Step 2: テスト実行**

Run: `cargo test yazi_tests -- --nocapture`
Expected: 全テスト PASS

**Step 3: コミット**

```bash
git add src/app/state.rs
git commit -m "test: add unit tests for yazi integration"
```

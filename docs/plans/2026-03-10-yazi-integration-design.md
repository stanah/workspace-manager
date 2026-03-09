# Yazi Integration Design

## Overview

workspace-managerの選択状態をyaziに連携し、ワークスペース/ペイン選択に追従するファイラー体験を実現する。

workspace-managerとyaziは完全に独立したプロセスとして動作し、Ghosttyのペイン分割で上下に配置する。連携はyaziのDDS（Data Distribution Service）の`ya` CLIを通じて行う。

## Architecture

```
┌─────────────────────────┐
│   workspace-manager     │  Ghostty上ペイン
│   (既存のTUI)            │
│                         │
│  選択変更 → デバウンス    │
│         → ya emit cd/reveal
│                ↓        │
├─────────────────────────┤
│   yazi                  │  Ghostty下ペイン（ユーザーが手動で起動）
│   (完全に独立したプロセス) │
└─────────────────────────┘
```

## Configuration

`~/.config/workspace-manager/config.toml`:

```toml
[yazi]
enabled = true
debounce_ms = 200
client_id = 9090  # yazi --client-id <数値> で起動時に指定
```

yaziの起動: `yazi --client-id 9090`

## Selection-to-Command Mapping

| Selected TreeItem | Command | Resolved Path |
|---|---|---|
| RepoGroup | `ya emit-to <client_id> cd <path>` | Repository root |
| Worktree | `ya emit-to <client_id> cd <path>` | Worktree path |
| Pane | `ya emit-to <client_id> reveal <cwd>` | Pane's current working directory |
| Session | `ya emit-to <client_id> cd <path>` | Associated worktree path |
| Branch / Separator | (no action) | — |

Pane選択時のみ`reveal`を使用し、cwdまでフォーカスを合わせる。それ以外はリポジトリ/worktreeのルートに`cd`する。

## Implementation Details

### New Types

```rust
enum YaziCommand {
    Cd(PathBuf),       // ya emit cd <path>
    Reveal(PathBuf),   // ya emit reveal <path>
}

struct YaziConfig {
    enabled: bool,
    debounce_ms: u64,  // default: 200
}
```

### AppState Changes

```rust
// New fields
pending_yazi: Option<(Instant, YaziCommand)>,
yazi_config: YaziConfig,
```

### Debounce Flow

1. Selection changes → set `pending_yazi` to `Some((Instant::now() + debounce_ms, command))`
2. Event loop adjusts `poll_event` timeout to fire at the deadline
3. On timeout: spawn `ya emit cd/reveal` as fire-and-forget, clear `pending_yazi`
4. If selection changes again before deadline, overwrite `pending_yazi` (resets timer)

### Event Loop Integration

The existing `poll_event` timeout is adjusted when `pending_yazi` is set. On timeout expiry:

```rust
if let Some((deadline, cmd)) = &state.pending_yazi {
    if Instant::now() >= *deadline {
        let args = match cmd {
            YaziCommand::Cd(p) => vec!["emit", "cd", p],
            YaziCommand::Reveal(p) => vec!["emit", "reveal", p],
        };
        std::process::Command::new("ya").args(&args).spawn();
        state.pending_yazi = None;
    }
}
```

### Selection Hook

Path resolution uses existing `selected_workspace()` method. On cursor movement (`j/k`, mouse click, etc.), after `selected_index` changes:

1. Determine TreeItem type at new index
2. Resolve path (repo root, worktree path, or pane cwd)
3. Set `pending_yazi` with appropriate YaziCommand

## Files to Modify

- `src/app/config.rs` — Add `YaziConfig` struct and deserialization
- `src/app/state.rs` — Add `pending_yazi` field, `YaziCommand` enum, path resolution method
- `src/app/events.rs` — Adjust poll timeout for debounce
- `src/main.rs` — Handle debounce timer expiry in event loop

## Files NOT Modified

- UI layout (no changes)
- Existing keybindings
- Multiplexer integration

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use std::time::Duration;

/// アプリケーション内部イベント
#[derive(Debug, Clone)]
pub enum AppEvent {
    /// キー入力
    Key(KeyEvent),
    /// ターミナルリサイズ
    Resize(u16, u16),
    /// ワークスペース状態更新（MCPから）
    WorkspaceUpdate {
        session_id: String,
        status: crate::workspace::WorkspaceStatus,
        message: Option<String>,
    },
    /// ワークスペース登録
    WorkspaceRegister {
        session_id: String,
        project_path: String,
        pane_id: Option<u32>,
    },
    /// ワークスペース登録解除
    WorkspaceUnregister { session_id: String },
    /// リフレッシュ要求
    Refresh,
    /// 終了要求
    Quit,
}

/// ユーザーアクション（キー入力から変換）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// 上に移動
    MoveUp,
    /// 下に移動
    MoveDown,
    /// 選択（フォーカス）
    Select,
    /// ヘルプ表示切替
    ToggleHelp,
    /// リフレッシュ
    Refresh,
    /// 終了
    Quit,
    /// Zellij: lazygit起動
    LaunchLazygit,
    /// Zellij: シェル起動
    LaunchShell,
    /// Zellij: yazi起動
    LaunchYazi,
    /// Zellij: 新規Claude Codeセッション
    NewSession,
    /// Zellij: ワークスペース終了
    CloseWorkspace,
    /// 何もしない
    None,
}

impl From<KeyEvent> for Action {
    fn from(key: KeyEvent) -> Self {
        match (key.code, key.modifiers) {
            // 移動
            (KeyCode::Up | KeyCode::Char('k'), _) => Action::MoveUp,
            (KeyCode::Down | KeyCode::Char('j'), _) => Action::MoveDown,
            // 選択
            (KeyCode::Enter, _) => Action::Select,
            // ヘルプ
            (KeyCode::Char('?'), _) => Action::ToggleHelp,
            // リフレッシュ
            (KeyCode::Char('r'), _) => Action::Refresh,
            // 終了
            (KeyCode::Char('q'), _) => Action::Quit,
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => Action::Quit,
            // Zellijアクション
            (KeyCode::Char('l'), _) => Action::LaunchLazygit,
            (KeyCode::Char('g'), _) => Action::LaunchShell,
            (KeyCode::Char('y'), _) => Action::LaunchYazi,
            (KeyCode::Char('n'), _) => Action::NewSession,
            (KeyCode::Char('x'), _) => Action::CloseWorkspace,
            // その他
            _ => Action::None,
        }
    }
}

/// イベントポーリング
pub fn poll_event(timeout: Duration) -> std::io::Result<Option<AppEvent>> {
    if event::poll(timeout)? {
        match event::read()? {
            Event::Key(key) => Ok(Some(AppEvent::Key(key))),
            Event::Resize(w, h) => Ok(Some(AppEvent::Resize(w, h))),
            _ => Ok(None),
        }
    } else {
        Ok(None)
    }
}

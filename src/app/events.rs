use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use std::time::Duration;

/// アプリケーション内部イベント
#[derive(Debug, Clone)]
pub enum AppEvent {
    /// キー入力
    Key(KeyEvent),
    /// マウス入力
    Mouse(MouseEvent),
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
    /// レイアウトを選択して開く（Cmd+Enter / Ctrl+Shift+Enter）
    SelectWithLayout,
    /// 展開/折りたたみ切り替え
    ToggleExpand,
    /// 戻る/閉じる
    Back,
    /// ヘルプ表示切替
    ToggleHelp,
    /// 表示モード切り替え（Worktrees / +Local / +All）
    ToggleDisplayMode,
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
    /// Zellij: ワークスペース終了（Internal→ペイン閉じる、External→タブ閉じる）
    CloseWorkspace,
    /// 新規worktree作成
    CreateWorktree,
    /// worktree削除
    DeleteWorktree,
    /// エディタで開く
    OpenInEditor,
    /// マウスクリックで行選択
    MouseSelect(u16),
    /// マウススクロール上
    ScrollUp,
    /// マウススクロール下
    ScrollDown,
    /// マウスダブルクリックで行選択＋オープン
    MouseDoubleClick(u16),
    /// マウスミドルクリックで行選択＋ワークスペース閉じる
    MouseMiddleClick(u16),
    /// 何もしない
    None,
}

impl From<KeyEvent> for Action {
    fn from(key: KeyEvent) -> Self {
        match (key.code, key.modifiers) {
            // 移動
            (KeyCode::Up | KeyCode::Char('k'), _) => Action::MoveUp,
            (KeyCode::Down | KeyCode::Char('j'), _) => Action::MoveDown,
            // レイアウト選択して開く (Tab)
            (KeyCode::Tab, _) => Action::SelectWithLayout,
            // 選択
            (KeyCode::Enter, _) => Action::Select,
            // 展開/折りたたみ
            (KeyCode::Char(' '), _) => Action::ToggleExpand,
            // ヘルプ
            (KeyCode::Char('?'), _) => Action::ToggleHelp,
            // 表示モード切り替え
            (KeyCode::Char('v'), _) => Action::ToggleDisplayMode,
            // リフレッシュ
            (KeyCode::Char('r'), _) => Action::Refresh,
            // 閉じる/戻る
            (KeyCode::Esc, _) => Action::Back,
            // 終了
            (KeyCode::Char('q'), _) => Action::Quit,
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => Action::Quit,
            // Worktree管理
            (KeyCode::Char('c'), _) | (KeyCode::Char('a'), _) => Action::CreateWorktree,
            (KeyCode::Char('d'), _) | (KeyCode::Delete, _) => Action::DeleteWorktree,
            // エディタで開く
            (KeyCode::Char('e'), _) => Action::OpenInEditor,
            // ワークスペース閉じる（Backspace）
            (KeyCode::Backspace, _) => Action::CloseWorkspace,
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

/// マウスイベントからアクションへの変換
/// list_area_y: リスト領域の開始Y座標
/// header_height: ボーダー(1) + ヘッダー行(1) = 2
pub fn mouse_action(event: MouseEvent, list_area_y: u16, header_height: u16) -> Action {
    match event.kind {
        MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
            // データ行の開始位置 = list_area_y + header_height
            let data_start = list_area_y + header_height;
            if event.row >= data_start {
                let row_index = event.row - data_start;
                Action::MouseSelect(row_index)
            } else {
                Action::None
            }
        }
        MouseEventKind::Down(crossterm::event::MouseButton::Middle) => {
            // ミドルクリックでタブを閉じる
            let data_start = list_area_y + header_height;
            if event.row >= data_start {
                let row_index = event.row - data_start;
                Action::MouseMiddleClick(row_index)
            } else {
                Action::None
            }
        }
        MouseEventKind::ScrollUp => Action::ScrollUp,
        MouseEventKind::ScrollDown => Action::ScrollDown,
        _ => Action::None,
    }
}

/// イベントポーリング
pub fn poll_event(timeout: Duration) -> std::io::Result<Option<AppEvent>> {
    if event::poll(timeout)? {
        match event::read()? {
            Event::Key(key) => Ok(Some(AppEvent::Key(key))),
            Event::Mouse(mouse) => Ok(Some(AppEvent::Mouse(mouse))),
            Event::Resize(w, h) => Ok(Some(AppEvent::Resize(w, h))),
            _ => Ok(None),
        }
    } else {
        Ok(None)
    }
}

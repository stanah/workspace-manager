use crate::workspace::{Workspace, WorkspaceStatus, scan_for_repositories, get_default_search_paths};
use std::collections::HashMap;

/// アプリケーションの表示モード
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ViewMode {
    #[default]
    List,
    Help,
    Detail,
}

/// アプリケーション状態
pub struct AppState {
    /// 検出されたワークスペース一覧
    pub workspaces: Vec<Workspace>,
    /// session_id -> workspace index のマッピング
    session_map: HashMap<String, usize>,
    /// 現在選択中のインデックス
    pub selected_index: usize,
    /// 表示モード
    pub view_mode: ViewMode,
    /// 終了フラグ
    pub should_quit: bool,
    /// ステータスバーメッセージ
    pub status_message: Option<String>,
}

impl AppState {
    /// 新規状態を作成
    pub fn new() -> Self {
        Self {
            workspaces: Vec::new(),
            session_map: HashMap::new(),
            selected_index: 0,
            view_mode: ViewMode::List,
            should_quit: false,
            status_message: None,
        }
    }

    /// ワークスペースをスキャンして読み込み
    pub fn scan_workspaces(&mut self) {
        let search_paths = get_default_search_paths();
        let mut workspaces: Vec<Workspace> = Vec::new();

        for path in &search_paths {
            let infos = scan_for_repositories(path, 3);
            for info in infos {
                workspaces.push(info.into());
            }
        }

        // パスでソート
        workspaces.sort_by(|a, b| a.project_path.cmp(&b.project_path));

        self.workspaces = workspaces;
        self.rebuild_session_map();

        self.status_message = Some(format!("Found {} workspaces", self.workspaces.len()));
    }

    /// session_mapを再構築
    fn rebuild_session_map(&mut self) {
        self.session_map.clear();
        for (idx, ws) in self.workspaces.iter().enumerate() {
            if let Some(ref session_id) = ws.session_id {
                self.session_map.insert(session_id.clone(), idx);
            }
        }
    }

    /// 選択を上に移動
    pub fn move_up(&mut self) {
        if !self.workspaces.is_empty() && self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    /// 選択を下に移動
    pub fn move_down(&mut self) {
        if !self.workspaces.is_empty() && self.selected_index < self.workspaces.len() - 1 {
            self.selected_index += 1;
        }
    }

    /// 現在選択中のワークスペースを取得
    pub fn selected_workspace(&self) -> Option<&Workspace> {
        self.workspaces.get(self.selected_index)
    }

    /// ワークスペースを登録（MCPから）
    pub fn register_workspace(
        &mut self,
        session_id: String,
        project_path: String,
        pane_id: Option<u32>,
    ) {
        // 既存のワークスペースを探す
        if let Some(idx) = self.workspaces.iter().position(|ws| ws.project_path == project_path) {
            self.workspaces[idx].session_id = Some(session_id.clone());
            self.workspaces[idx].pane_id = pane_id;
            self.workspaces[idx].status = WorkspaceStatus::Idle;
            self.session_map.insert(session_id, idx);
        } else {
            // 新規ワークスペースを追加
            let mut ws = Workspace::new(
                project_path,
                "unknown".to_string(),
                "unknown".to_string(),
            );
            ws.session_id = Some(session_id.clone());
            ws.pane_id = pane_id;
            ws.status = WorkspaceStatus::Idle;

            let idx = self.workspaces.len();
            self.workspaces.push(ws);
            self.session_map.insert(session_id, idx);
        }
    }

    /// ワークスペース状態を更新（MCPから）
    pub fn update_workspace_status(
        &mut self,
        session_id: &str,
        status: WorkspaceStatus,
        message: Option<String>,
    ) {
        if let Some(&idx) = self.session_map.get(session_id) {
            if let Some(ws) = self.workspaces.get_mut(idx) {
                ws.update_status(status, message);
            }
        }
    }

    /// ワークスペースを登録解除（MCPから）
    pub fn unregister_workspace(&mut self, session_id: &str) {
        if let Some(&idx) = self.session_map.get(session_id) {
            if let Some(ws) = self.workspaces.get_mut(idx) {
                ws.session_id = None;
                ws.status = WorkspaceStatus::Disconnected;
            }
            self.session_map.remove(session_id);
        }
    }

    /// ヘルプ表示を切り替え
    pub fn toggle_help(&mut self) {
        self.view_mode = match self.view_mode {
            ViewMode::Help => ViewMode::List,
            _ => ViewMode::Help,
        };
    }

    /// アクティブなワークスペース数を取得
    pub fn active_count(&self) -> usize {
        self.workspaces
            .iter()
            .filter(|ws| ws.session_id.is_some())
            .count()
    }

    /// 作業中のワークスペース数を取得
    pub fn working_count(&self) -> usize {
        self.workspaces
            .iter()
            .filter(|ws| ws.status == WorkspaceStatus::Working)
            .count()
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

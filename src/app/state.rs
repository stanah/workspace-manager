use crate::workspace::{Workspace, WorkspaceStatus, scan_for_repositories, get_default_search_paths};
use std::collections::{HashMap, HashSet};

use crate::ui::InputDialog;

/// アプリケーションの表示モード
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ViewMode {
    #[default]
    List,
    Help,
    Detail,
    /// 入力ダイアログ表示中
    Input,
}

/// ツリー表示用のアイテム
#[derive(Debug, Clone)]
pub enum TreeItem {
    /// リポジトリグループ（折りたたみ可能）
    RepoGroup {
        name: String,
        path: String,
        expanded: bool,
        worktree_count: usize,
    },
    /// ワークスペース（worktree）
    Worktree {
        workspace_index: usize,
        is_last: bool,
    },
}

/// アプリケーション状態
pub struct AppState {
    /// 検出されたワークスペース一覧
    pub workspaces: Vec<Workspace>,
    /// ツリー表示用のフラット化されたリスト
    pub tree_items: Vec<TreeItem>,
    /// 折りたたまれたリポジトリのパス
    collapsed_repos: HashSet<String>,
    /// session_id -> workspace index のマッピング
    session_map: HashMap<String, usize>,
    /// 現在選択中のインデックス（tree_items内）
    pub selected_index: usize,
    /// 表示モード
    pub view_mode: ViewMode,
    /// 入力ダイアログ状態
    pub input_dialog: Option<InputDialog>,
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
            tree_items: Vec::new(),
            collapsed_repos: HashSet::new(),
            session_map: HashMap::new(),
            selected_index: 0,
            view_mode: ViewMode::List,
            input_dialog: None,
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
        self.rebuild_tree();

        self.status_message = Some(format!("Found {} workspaces", self.workspaces.len()));
    }

    /// ツリー構造を再構築
    fn rebuild_tree(&mut self) {
        self.tree_items.clear();

        // リポジトリごとにグループ化
        let mut repo_groups: HashMap<String, Vec<usize>> = HashMap::new();

        for (idx, ws) in self.workspaces.iter().enumerate() {
            // 親リポジトリのパスを推定（worktreeの場合は親ディレクトリ）
            let repo_key = self.get_repo_key(ws);
            repo_groups.entry(repo_key).or_default().push(idx);
        }

        // ソートしてツリーアイテムを構築
        let mut repo_keys: Vec<_> = repo_groups.keys().cloned().collect();
        repo_keys.sort();

        for repo_key in repo_keys {
            let indices = &repo_groups[&repo_key];
            let is_expanded = !self.collapsed_repos.contains(&repo_key);

            // リポジトリ名を取得
            let repo_name = indices
                .first()
                .and_then(|&idx| self.workspaces.get(idx))
                .map(|ws| ws.repo_name.clone())
                .unwrap_or_else(|| "unknown".to_string());

            // グループヘッダーを追加
            self.tree_items.push(TreeItem::RepoGroup {
                name: repo_name,
                path: repo_key.clone(),
                expanded: is_expanded,
                worktree_count: indices.len(),
            });

            // 展開されている場合はworktreeを追加
            if is_expanded {
                for (i, &ws_idx) in indices.iter().enumerate() {
                    let is_last = i == indices.len() - 1;
                    self.tree_items.push(TreeItem::Worktree {
                        workspace_index: ws_idx,
                        is_last,
                    });
                }
            }
        }
    }

    /// ワークスペースからリポジトリキーを取得
    fn get_repo_key(&self, ws: &Workspace) -> String {
        // Parallelスタイルのworktreeを検出: repo=branch 形式
        // 例: config=feature -> config
        if let Some(idx) = ws.repo_name.rfind('=') {
            let base_name = &ws.repo_name[..idx];
            // ベース名が空でなければそれを使用
            if !base_name.is_empty() {
                return base_name.to_string();
            }
        }

        // worktreeの.gitファイルから親リポジトリを検出
        let git_path = std::path::Path::new(&ws.project_path).join(".git");
        if git_path.is_file() {
            // worktreeの場合、.gitはファイルで親への参照を含む
            if let Ok(content) = std::fs::read_to_string(&git_path) {
                // "gitdir: /path/to/parent/.git/worktrees/name" 形式
                if let Some(gitdir) = content.strip_prefix("gitdir:") {
                    let gitdir = gitdir.trim();
                    // .git/worktrees/name から親リポジトリを特定
                    if let Some(worktrees_idx) = gitdir.find("/.git/worktrees/") {
                        let parent_path = &gitdir[..worktrees_idx];
                        if let Some(parent_name) = std::path::Path::new(parent_path)
                            .file_name()
                            .and_then(|n| n.to_str())
                        {
                            return parent_name.to_string();
                        }
                    }
                }
            }
        }

        // フォールバック: repo_nameをそのまま使用
        ws.repo_name.clone()
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
        if !self.tree_items.is_empty() && self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    /// 選択を下に移動
    pub fn move_down(&mut self) {
        if !self.tree_items.is_empty() && self.selected_index < self.tree_items.len() - 1 {
            self.selected_index += 1;
        }
    }

    /// 選択中のアイテムを展開/折りたたみ
    pub fn toggle_expand(&mut self) {
        if let Some(TreeItem::RepoGroup { path, expanded, .. }) = self.tree_items.get(self.selected_index).cloned() {
            if expanded {
                self.collapsed_repos.insert(path);
            } else {
                self.collapsed_repos.remove(&path);
            }
            self.rebuild_tree();
        }
    }

    /// 現在選択中のワークスペースを取得
    pub fn selected_workspace(&self) -> Option<&Workspace> {
        match self.tree_items.get(self.selected_index) {
            Some(TreeItem::Worktree { workspace_index, .. }) => {
                self.workspaces.get(*workspace_index)
            }
            Some(TreeItem::RepoGroup { .. }) => {
                // グループが選択されている場合は最初のworktreeを返す
                None
            }
            None => None,
        }
    }

    /// 現在選択中のツリーアイテムを取得
    pub fn selected_tree_item(&self) -> Option<&TreeItem> {
        self.tree_items.get(self.selected_index)
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
        self.rebuild_tree();
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

    /// ツリーアイテム数を取得
    pub fn tree_item_count(&self) -> usize {
        self.tree_items.len()
    }

    /// 新規worktree作成ダイアログを開く
    pub fn open_create_worktree_dialog(&mut self) {
        self.input_dialog = Some(InputDialog::new_create_worktree());
        self.view_mode = ViewMode::Input;
    }

    /// worktree削除ダイアログを開く
    pub fn open_delete_worktree_dialog(&mut self) {
        if let Some(ws) = self.selected_workspace() {
            let path = ws.display_path();
            self.input_dialog = Some(InputDialog::new_delete_worktree(path));
            self.view_mode = ViewMode::Input;
        }
    }

    /// 入力ダイアログを閉じる
    pub fn close_input_dialog(&mut self) {
        self.input_dialog = None;
        self.view_mode = ViewMode::List;
    }

    /// 選択中のリポジトリグループのパスを取得
    pub fn selected_repo_path(&self) -> Option<String> {
        // 現在選択中のアイテムがWorktreeの場合はそのワークスペースのパスを返す
        // RepoGroupの場合は最初のworktreeのパスを返す
        match self.tree_items.get(self.selected_index) {
            Some(TreeItem::Worktree { workspace_index, .. }) => {
                self.workspaces.get(*workspace_index).map(|ws| ws.project_path.clone())
            }
            Some(TreeItem::RepoGroup { path, .. }) => {
                // このグループの最初のworktreeを探す
                for item in &self.tree_items {
                    if let TreeItem::Worktree { workspace_index, .. } = item {
                        if let Some(ws) = self.workspaces.get(*workspace_index) {
                            if ws.repo_name == *path {
                                return Some(ws.project_path.clone());
                            }
                        }
                    }
                }
                None
            }
            None => None,
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

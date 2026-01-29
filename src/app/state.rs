use crate::workspace::{
    AiTool, Session, SessionStatus, Workspace, WorktreeManager, get_default_search_paths,
    scan_for_repositories,
};
use ratatui::widgets::TableState;
use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::ui::{InputDialog, SelectionContext, SelectionDialog, SelectionDialogKind};

/// アプリケーションの表示モード
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ViewMode {
    #[default]
    List,
    Help,
    Detail,
    /// 入力ダイアログ表示中
    Input,
    /// 選択ダイアログ表示中
    Selection,
}

/// リスト表示モード（ブランチ表示の有無）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ListDisplayMode {
    /// 既存worktreeのみ表示
    #[default]
    Worktrees,
    /// worktree + 全ブランチ（ローカル＋リモート）
    WithBranches,
}

impl ListDisplayMode {
    /// 次の表示モードに切り替え
    pub fn next(self) -> Self {
        match self {
            Self::Worktrees => Self::WithBranches,
            Self::WithBranches => Self::Worktrees,
        }
    }

    /// 表示用ラベル
    pub fn label(&self) -> &'static str {
        match self {
            Self::Worktrees => "Worktrees",
            Self::WithBranches => "+Branches",
        }
    }
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
    /// セッション（AI CLI）
    Session {
        session_index: usize,
        is_last: bool,
        parent_is_last: bool,
    },
    /// ブランチ（worktree未作成）
    Branch {
        name: String,
        is_local: bool,
        repo_path: String,
        is_last: bool,
    },
    /// リモートブランチグループ（折りたたみ可能）
    RemoteBranchGroup {
        repo_path: String,
        expanded: bool,
        count: usize,
        is_last: bool,
    },
}

/// アプリケーション状態
pub struct AppState {
    /// 検出されたワークスペース一覧
    pub workspaces: Vec<Workspace>,
    /// アクティブなセッション一覧
    pub sessions: Vec<Session>,
    /// ツリー表示用のフラット化されたリスト
    pub tree_items: Vec<TreeItem>,
    /// 折りたたまれたリポジトリのパス
    collapsed_repos: HashSet<String>,
    /// 折りたたまれたリモートブランチグループのリポパス
    expanded_remote_branches: HashSet<String>,
    /// external_id -> session index のマッピング
    session_map: HashMap<String, usize>,
    /// workspace_index -> session indices のマッピング
    sessions_by_workspace: HashMap<usize, Vec<usize>>,
    /// 現在選択中のインデックス（tree_items内）
    pub selected_index: usize,
    /// 表示モード
    pub view_mode: ViewMode,
    /// リスト表示モード（ブランチ表示の有無）
    pub list_display_mode: ListDisplayMode,
    /// 入力ダイアログ状態
    pub input_dialog: Option<InputDialog>,
    /// 選択ダイアログ状態
    pub selection_dialog: Option<SelectionDialog>,
    /// 終了フラグ
    pub should_quit: bool,
    /// ステータスバーメッセージ
    pub status_message: Option<String>,
    /// Zellijで開いているタブ名のキャッシュ
    open_tabs: HashSet<String>,
    /// ブランチフィルター（検索文字列）
    pub branch_filter: Option<String>,
    /// テーブルのスクロール状態（フレーム間で維持）
    pub table_state: TableState,
}

impl AppState {
    /// 新規状態を作成
    pub fn new() -> Self {
        Self {
            workspaces: Vec::new(),
            sessions: Vec::new(),
            tree_items: Vec::new(),
            collapsed_repos: HashSet::new(),
            expanded_remote_branches: HashSet::new(),
            session_map: HashMap::new(),
            sessions_by_workspace: HashMap::new(),
            selected_index: 0,
            view_mode: ViewMode::List,
            list_display_mode: ListDisplayMode::default(),
            input_dialog: None,
            selection_dialog: None,
            should_quit: false,
            status_message: None,
            open_tabs: HashSet::new(),
            branch_filter: None,
            table_state: TableState::default(),
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
        self.rebuild_tree();

        self.status_message = Some(format!("Found {} workspaces", self.workspaces.len()));
    }

    /// ツリー構造を再構築
    pub fn rebuild_tree(&mut self) {
        self.rebuild_tree_with_manager(None);
    }

    /// WorktreeManagerを使ってツリー構造を再構築（ブランチ情報含む）
    pub fn rebuild_tree_with_manager(&mut self, worktree_manager: Option<&WorktreeManager>) {
        self.tree_items.clear();

        // リポジトリごとにグループ化
        let mut repo_groups: HashMap<String, Vec<usize>> = HashMap::new();
        let mut repo_paths: HashMap<String, String> = HashMap::new(); // repo_key -> project_path

        for (idx, ws) in self.workspaces.iter().enumerate() {
            // 親リポジトリのパスを推定（worktreeの場合は親ディレクトリ）
            let repo_key = self.get_repo_key(ws);
            repo_groups.entry(repo_key.clone()).or_default().push(idx);
            // 最初のワークスペースのパスを保存
            repo_paths
                .entry(repo_key)
                .or_insert_with(|| ws.project_path.clone());
        }

        // ソートしてツリーアイテムを構築
        let mut repo_keys: Vec<_> = repo_groups.keys().cloned().collect();
        repo_keys.sort();

        for repo_key in repo_keys {
            let indices = &repo_groups[&repo_key];
            let is_expanded = !self.collapsed_repos.contains(&repo_key);
            let repo_path = repo_paths.get(&repo_key).cloned().unwrap_or_default();

            // リポジトリ名を取得
            let repo_name = indices
                .first()
                .and_then(|&idx| self.workspaces.get(idx))
                .map(|ws| ws.repo_name.clone())
                .unwrap_or_else(|| "unknown".to_string());

            // 既存worktreeのブランチ名を収集
            let existing_branches: HashSet<String> = indices
                .iter()
                .filter_map(|&idx| self.workspaces.get(idx))
                .map(|ws| ws.branch.clone())
                .collect();

            // ブランチ情報を取得
            let (local_branches, remote_branches) =
                if self.list_display_mode != ListDisplayMode::Worktrees {
                    if let Some(manager) = worktree_manager {
                        // フィルターを適用するクロージャ
                        let filter_ref = self.branch_filter.as_ref();
                        let matches_filter = |b: &String| -> bool {
                            match filter_ref {
                                Some(filter) if !filter.is_empty() => {
                                    b.to_lowercase().contains(&filter.to_lowercase())
                                }
                                _ => true,
                            }
                        };

                        let local = manager
                            .list_local_branches(std::path::Path::new(&repo_path))
                            .unwrap_or_default()
                            .into_iter()
                            .filter(|b| !existing_branches.contains(b) && matches_filter(b))
                            .collect::<Vec<_>>();

                        let remote = if self.list_display_mode == ListDisplayMode::WithBranches {
                            let max_branches = manager.config().max_remote_branches;
                            let branches: Vec<_> = manager
                                .list_remote_branches(std::path::Path::new(&repo_path))
                                .unwrap_or_default()
                                .into_iter()
                                .filter(|b| {
                                    !existing_branches.contains(b)
                                        && !local.contains(b)
                                        && matches_filter(b)
                                })
                                .collect();
                            // 上限を適用（0は無制限）
                            if max_branches > 0 && branches.len() > max_branches {
                                branches.into_iter().take(max_branches).collect()
                            } else {
                                branches
                            }
                        } else {
                            Vec::new()
                        };

                        (local, remote)
                    } else {
                        (Vec::new(), Vec::new())
                    }
                } else {
                    (Vec::new(), Vec::new())
                };

            let remote_expanded = self.expanded_remote_branches.contains(&repo_key);

            // グループヘッダーを追加
            self.tree_items.push(TreeItem::RepoGroup {
                name: repo_name,
                path: repo_key.clone(),
                expanded: is_expanded,
                worktree_count: indices.len(),
            });

            // 展開されている場合はworktreeとセッション、ブランチを追加
            if is_expanded {
                let has_local_branches = !local_branches.is_empty();
                let has_remote_branches = !remote_branches.is_empty();

                // RepoGroup直下の子: Worktree群、Session群、ローカルBranch群、RemoteBranchGroup
                // 各アイテムの is_last = 「同じ親の中で最後の子か」

                // Worktreeとそのセッションを追加
                for (ws_idx_pos, &ws_idx) in indices.iter().enumerate() {
                    let workspace_sessions = self.sessions_for_workspace(ws_idx);
                    let is_last_in_group = ws_idx_pos == indices.len() - 1
                        && !has_local_branches
                        && !has_remote_branches;

                    self.tree_items.push(TreeItem::Worktree {
                        workspace_index: ws_idx,
                        is_last: is_last_in_group,
                    });

                    // このワークスペースのセッションを追加
                    let parent_last = is_last_in_group;
                    for (sess_idx_pos, &sess_idx) in workspace_sessions.iter().enumerate() {
                        self.tree_items.push(TreeItem::Session {
                            session_index: sess_idx,
                            is_last: sess_idx_pos == workspace_sessions.len() - 1,
                            parent_is_last: parent_last,
                        });
                    }
                }

                // ローカルブランチを追加
                let local_count = local_branches.len();
                for (i, branch) in local_branches.into_iter().enumerate() {
                    let is_last = i == local_count - 1 && !has_remote_branches;
                    self.tree_items.push(TreeItem::Branch {
                        name: branch,
                        is_local: true,
                        repo_path: repo_path.clone(),
                        is_last,
                    });
                }

                // リモートブランチグループを追加（常にRepoGroup直下の最後の子）
                if has_remote_branches {
                    let remote_count = remote_branches.len();

                    self.tree_items.push(TreeItem::RemoteBranchGroup {
                        repo_path: repo_path.clone(),
                        expanded: remote_expanded,
                        count: remote_count,
                        is_last: true, // リモートグループは常にRepoGroup内の最後
                    });

                    if remote_expanded {
                        let branch_count = remote_branches.len();
                        for (i, branch) in remote_branches.into_iter().enumerate() {
                            self.tree_items.push(TreeItem::Branch {
                                name: branch,
                                is_local: false,
                                repo_path: repo_path.clone(),
                                is_last: i == branch_count - 1, // RemoteBranchGroup内の最後
                            });
                        }
                    }
                }
            }
        }
    }

    /// ワークスペースからリポジトリキーを取得
    fn get_repo_key(&self, ws: &Workspace) -> String {
        // Parallelスタイルのworktreeを検出: repo__branch 形式
        // 例: config__feature -> config
        if let Some(idx) = ws.repo_name.rfind("__") {
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

    // ===== Session management =====

    /// セッションを追加
    pub fn add_session(&mut self, session: Session) -> usize {
        let workspace_index = session.workspace_index;
        let external_id = session.external_id.clone();
        let session_index = self.sessions.len();

        self.sessions.push(session);
        self.session_map.insert(external_id, session_index);
        self.sessions_by_workspace
            .entry(workspace_index)
            .or_default()
            .push(session_index);

        session_index
    }

    /// セッションを外部IDで検索
    pub fn get_session_by_external_id(&self, external_id: &str) -> Option<&Session> {
        self.session_map
            .get(external_id)
            .and_then(|&idx| self.sessions.get(idx))
    }

    /// セッションを外部IDで検索（mutable）
    pub fn get_session_by_external_id_mut(&mut self, external_id: &str) -> Option<&mut Session> {
        self.session_map
            .get(external_id)
            .and_then(|&idx| self.sessions.get_mut(idx))
    }

    /// ワークスペースのセッション一覧を取得（切断されたセッションは除外）
    pub fn sessions_for_workspace(&self, workspace_index: usize) -> Vec<usize> {
        self.sessions_by_workspace
            .get(&workspace_index)
            .map(|indices| {
                indices
                    .iter()
                    .filter(|&&idx| {
                        self.sessions
                            .get(idx)
                            .map(|s| s.is_active())
                            .unwrap_or(false)
                    })
                    .copied()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// セッションを削除（実際には切断状態にする）
    pub fn remove_session(&mut self, external_id: &str) {
        if let Some(&session_index) = self.session_map.get(external_id) {
            if let Some(session) = self.sessions.get_mut(session_index) {
                session.disconnect();
            }
        }
    }

    /// セッションステータスを更新
    pub fn update_session_status(
        &mut self,
        external_id: &str,
        status: SessionStatus,
        message: Option<String>,
    ) {
        if let Some(&session_index) = self.session_map.get(external_id) {
            if let Some(session) = self.sessions.get_mut(session_index) {
                session.update_status(status, message);
            }
        }
    }

    /// プロジェクトパスからワークスペースインデックスを検索
    pub fn find_workspace_by_path(&self, project_path: &str) -> Option<usize> {
        // 正規化されたパスで比較
        let normalized = normalize_path(project_path);
        self.workspaces
            .iter()
            .position(|w| normalize_path(&w.project_path) == normalized)
    }

    /// セッションを登録（新規または既存を更新）
    pub fn register_session(
        &mut self,
        external_id: String,
        project_path: &str,
        tool: AiTool,
        pane_id: Option<u32>,
    ) -> Option<usize> {
        // ワークスペースを検索
        let workspace_index = self.find_workspace_by_path(project_path)?;

        // 既存セッションがあれば更新
        if let Some(&session_index) = self.session_map.get(&external_id) {
            if let Some(session) = self.sessions.get_mut(session_index) {
                session.status = SessionStatus::Idle;
                session.pane_id = pane_id;
                session.updated_at = std::time::SystemTime::now();
            }
            return Some(session_index);
        }

        // 新規セッションを作成
        let mut session = Session::new(external_id, workspace_index, tool);
        session.pane_id = pane_id;
        let session_index = self.add_session(session);

        Some(session_index)
    }

    /// ワークスペースの集約ステータスを取得
    /// 優先度: Working > NeedsInput > Idle > Disconnected
    pub fn workspace_aggregate_status(&self, workspace_index: usize) -> SessionStatus {
        let session_indices = self.sessions_for_workspace(workspace_index);

        if session_indices.is_empty() {
            return SessionStatus::Disconnected;
        }

        let mut has_working = false;
        let mut has_needs_input = false;
        let mut has_idle = false;

        for &idx in &session_indices {
            if let Some(session) = self.sessions.get(idx) {
                match session.status {
                    SessionStatus::Working => has_working = true,
                    SessionStatus::NeedsInput => has_needs_input = true,
                    SessionStatus::Idle | SessionStatus::Success => has_idle = true,
                    _ => {}
                }
            }
        }

        if has_working {
            SessionStatus::Working
        } else if has_needs_input {
            SessionStatus::NeedsInput
        } else if has_idle {
            SessionStatus::Idle
        } else {
            SessionStatus::Disconnected
        }
    }

    // ===== Navigation =====

    /// 選択インデックスを設定し、テーブルのスクロール状態も同期する
    pub fn set_selected_index(&mut self, index: usize) {
        self.selected_index = index;
        self.table_state.select(Some(index));
    }

    /// 選択を上に移動
    pub fn move_up(&mut self) {
        if !self.tree_items.is_empty() && self.selected_index > 0 {
            self.set_selected_index(self.selected_index - 1);
        }
    }

    /// 選択を下に移動
    pub fn move_down(&mut self) {
        if !self.tree_items.is_empty() && self.selected_index < self.tree_items.len() - 1 {
            self.set_selected_index(self.selected_index + 1);
        }
    }

    /// 選択中のアイテムを展開/折りたたみ
    pub fn toggle_expand(&mut self) {
        match self.tree_items.get(self.selected_index).cloned() {
            Some(TreeItem::RepoGroup { path, expanded, .. }) => {
                if expanded {
                    self.collapsed_repos.insert(path);
                } else {
                    self.collapsed_repos.remove(&path);
                }
            }
            Some(TreeItem::RemoteBranchGroup { repo_path, expanded, .. }) => {
                let repo_key = self.find_repo_key_for_path(&repo_path);
                if expanded {
                    self.expanded_remote_branches.remove(&repo_key);
                } else {
                    self.expanded_remote_branches.insert(repo_key);
                }
            }
            _ => {}
        }
    }

    /// 選択中のアイテムを展開（右キー）
    /// 注意: 呼び出し側で rebuild_tree_with_manager() を呼ぶこと
    pub fn expand(&mut self) {
        match self.tree_items.get(self.selected_index).cloned() {
            Some(TreeItem::RepoGroup { path, expanded, .. }) => {
                if !expanded {
                    self.collapsed_repos.remove(&path);
                }
            }
            Some(TreeItem::RemoteBranchGroup { repo_path, expanded, .. }) => {
                if !expanded {
                    let repo_key = self.find_repo_key_for_path(&repo_path);
                    self.expanded_remote_branches.insert(repo_key);
                }
            }
            Some(TreeItem::Worktree { .. })
            | Some(TreeItem::Session { .. })
            | Some(TreeItem::Branch { .. }) => {
                // 子アイテム: 親RepoGroupへ移動
                self.move_to_parent_repo_group();
            }
            None => {}
        }
    }

    /// 選択中のアイテムを折りたたみ（左キー）
    /// 注意: 呼び出し側で rebuild_tree_with_manager() を呼ぶこと
    pub fn collapse(&mut self) {
        match self.tree_items.get(self.selected_index).cloned() {
            Some(TreeItem::RepoGroup { path, expanded, .. }) => {
                if expanded {
                    self.collapsed_repos.insert(path);
                }
            }
            Some(TreeItem::RemoteBranchGroup { repo_path, expanded, .. }) => {
                if expanded {
                    let repo_key = self.find_repo_key_for_path(&repo_path);
                    self.expanded_remote_branches.remove(&repo_key);
                } else {
                    // 折りたたみ済みなら親RepoGroupへ移動
                    self.move_to_parent_repo_group();
                }
            }
            Some(TreeItem::Worktree { .. })
            | Some(TreeItem::Session { .. })
            | Some(TreeItem::Branch { .. }) => {
                // 子アイテム: 親RepoGroupに移動して折りたたみ
                if let Some(parent_idx) = self.find_parent_repo_group_index() {
                    self.set_selected_index(parent_idx);
                    if let Some(TreeItem::RepoGroup { path, .. }) = self.tree_items.get(parent_idx).cloned() {
                        self.collapsed_repos.insert(path);
                    }
                }
            }
            None => {}
        }
    }

    /// 親RepoGroupのインデックスを探す
    fn find_parent_repo_group_index(&self) -> Option<usize> {
        for i in (0..self.selected_index).rev() {
            if matches!(self.tree_items.get(i), Some(TreeItem::RepoGroup { .. })) {
                return Some(i);
            }
        }
        None
    }

    /// 親RepoGroupへカーソル移動
    fn move_to_parent_repo_group(&mut self) {
        if let Some(idx) = self.find_parent_repo_group_index() {
            self.set_selected_index(idx);
        }
    }

    /// repo_pathからrepo_keyを逆引き
    fn find_repo_key_for_path(&self, repo_path: &str) -> String {
        for ws in &self.workspaces {
            if ws.project_path == repo_path {
                return self.get_repo_key(ws);
            }
        }
        // フォールバック: repo_pathをそのまま使用
        repo_path.to_string()
    }

    /// 現在選択中のワークスペースを取得
    pub fn selected_workspace(&self) -> Option<&Workspace> {
        match self.tree_items.get(self.selected_index) {
            Some(TreeItem::Worktree { workspace_index, .. }) => {
                self.workspaces.get(*workspace_index)
            }
            Some(TreeItem::Session { session_index, .. }) => {
                self.sessions.get(*session_index).and_then(|s| {
                    self.workspaces.get(s.workspace_index)
                })
            }
            Some(TreeItem::RepoGroup { .. })
            | Some(TreeItem::Branch { .. })
            | Some(TreeItem::RemoteBranchGroup { .. }) => {
                // グループまたはブランチが選択されている場合はNone
                None
            }
            None => None,
        }
    }

    /// 現在選択中のセッションを取得
    pub fn selected_session(&self) -> Option<&Session> {
        match self.tree_items.get(self.selected_index) {
            Some(TreeItem::Session { session_index, .. }) => self.sessions.get(*session_index),
            _ => None,
        }
    }

    /// 現在選択中のブランチ情報を取得
    pub fn selected_branch_info(&self) -> Option<(&str, bool, &str)> {
        match self.tree_items.get(self.selected_index) {
            Some(TreeItem::Branch {
                name,
                is_local,
                repo_path,
                ..
            }) => Some((name.as_str(), *is_local, repo_path.as_str())),
            _ => None,
        }
    }

    /// 表示モードを切り替え
    pub fn toggle_display_mode(&mut self) {
        self.list_display_mode = self.list_display_mode.next();
    }

    /// 現在選択中のツリーアイテムを取得
    #[allow(dead_code)]
    pub fn selected_tree_item(&self) -> Option<&TreeItem> {
        self.tree_items.get(self.selected_index)
    }

    /// ヘルプ表示を切り替え
    pub fn toggle_help(&mut self) {
        self.view_mode = match self.view_mode {
            ViewMode::Help => ViewMode::List,
            _ => ViewMode::Help,
        };
    }

    /// アクティブなセッション数を取得
    pub fn active_count(&self) -> usize {
        self.sessions.iter().filter(|s| s.is_active()).count()
    }

    /// 作業中のセッション数を取得
    pub fn working_count(&self) -> usize {
        self.sessions
            .iter()
            .filter(|s| s.status == SessionStatus::Working)
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
    pub fn open_delete_worktree_dialog(&mut self, force: bool) {
        if let Some(ws) = self.selected_workspace() {
            let path = ws.display_path();
            self.input_dialog = Some(InputDialog::new_delete_worktree(path, force));
            self.view_mode = ViewMode::Input;
        }
    }

    /// 入力ダイアログを閉じる
    pub fn close_input_dialog(&mut self) {
        self.input_dialog = None;
        self.view_mode = ViewMode::List;
    }

    /// セッション選択ダイアログを開く
    pub fn open_session_select_dialog(&mut self, sessions: Vec<String>, context: SelectionContext) {
        self.selection_dialog = Some(SelectionDialog::new_session_select(sessions, context));
        self.view_mode = ViewMode::Selection;
    }

    /// レイアウト選択ダイアログを開く
    pub fn open_layout_select_dialog(&mut self, layouts: Vec<String>, context: SelectionContext) {
        self.selection_dialog = Some(SelectionDialog::new_layout_select(layouts, context));
        self.view_mode = ViewMode::Selection;
    }

    /// 選択ダイアログを閉じる
    pub fn close_selection_dialog(&mut self) {
        self.selection_dialog = None;
        self.view_mode = ViewMode::List;
    }

    /// 選択ダイアログの選択を上に移動
    pub fn selection_move_up(&mut self) {
        if let Some(ref mut dialog) = self.selection_dialog {
            dialog.move_up();
        }
    }

    /// 選択ダイアログの選択を下に移動
    pub fn selection_move_down(&mut self) {
        if let Some(ref mut dialog) = self.selection_dialog {
            dialog.move_down();
        }
    }

    /// 選択ダイアログで選択されたアイテムを取得
    pub fn get_selected_dialog_item(&self) -> Option<&str> {
        self.selection_dialog
            .as_ref()
            .and_then(|d| d.selected_item())
    }

    /// 選択ダイアログの種類を取得
    pub fn selection_dialog_kind(&self) -> Option<&SelectionDialogKind> {
        self.selection_dialog.as_ref().map(|d| &d.kind)
    }

    /// 選択ダイアログのコンテキストを取得
    pub fn selection_dialog_context(&self) -> Option<&SelectionContext> {
        self.selection_dialog
            .as_ref()
            .and_then(|d| d.context.as_ref())
    }

    /// Zellijで開いているタブ名を更新
    pub fn update_open_tabs(&mut self, tabs: Vec<String>) {
        self.open_tabs = tabs.into_iter().collect();
    }

    /// ワークスペースがZellijタブとして開いているか確認
    /// タブ名は通常 "{repo}/{branch}" 形式なので、複数パターンでマッチング
    pub fn is_workspace_open(&self, repo_name: &str, branch: &str) -> bool {
        // パターン1: "{repo}/{branch}" 形式（デフォルト）
        let pattern1 = format!("{}/{}", repo_name, branch);
        // パターン2: ブランチ名のみ
        let pattern2 = branch;
        // パターン3: "__" 形式のrepo名の場合、ベース名で検索
        let base_repo = repo_name.split("__").next().unwrap_or(repo_name);
        let pattern3 = format!("{}/{}", base_repo, branch);

        self.open_tabs.contains(&pattern1)
            || self.open_tabs.contains(pattern2)
            || self.open_tabs.contains(&pattern3)
    }

    /// タブ名でワークスペースを選択
    /// タブ名は "{repo}/{branch}" 形式を想定し、各ワークスペースと照合する
    pub fn select_by_tab_name(&mut self, tab_name: &str) -> bool {
        for (idx, item) in self.tree_items.iter().enumerate() {
            if let TreeItem::Worktree { workspace_index, .. } = item {
                if let Some(ws) = self.workspaces.get(*workspace_index) {
                    // パターン1: "{repo}/{branch}" 形式
                    let pattern1 = format!("{}/{}", ws.repo_name, ws.branch);
                    // パターン2: "__" 形式のrepo名のベース名
                    let base_repo = ws.repo_name.split("__").next().unwrap_or(&ws.repo_name);
                    let pattern2 = format!("{}/{}", base_repo, ws.branch);

                    if tab_name == pattern1 || tab_name == pattern2 || tab_name == ws.branch {
                        self.set_selected_index(idx);
                        return true;
                    }
                }
            }
        }
        false
    }

    /// 選択中のリポジトリのルートパスを取得
    ///
    /// ワークツリーが選択されている場合でも、git commondir から
    /// 元のリポジトリルートを解決して返す。
    pub fn selected_repo_path(&self) -> Option<String> {
        let path = match self.tree_items.get(self.selected_index) {
            Some(TreeItem::Worktree { workspace_index, .. }) => self
                .workspaces
                .get(*workspace_index)
                .map(|ws| ws.project_path.clone()),
            Some(TreeItem::Session { session_index, .. }) => {
                self.sessions.get(*session_index).and_then(|s| {
                    self.workspaces
                        .get(s.workspace_index)
                        .map(|ws| ws.project_path.clone())
                })
            }
            Some(TreeItem::Branch { repo_path, .. }) => Some(repo_path.clone()),
            Some(TreeItem::RemoteBranchGroup { repo_path, .. }) => Some(repo_path.clone()),
            Some(TreeItem::RepoGroup { path, .. }) => {
                // このグループの最初のworktreeを探す
                for item in &self.tree_items {
                    if let TreeItem::Worktree { workspace_index, .. } = item {
                        if let Some(ws) = self.workspaces.get(*workspace_index) {
                            if ws.repo_name == *path {
                                return Some(resolve_repo_root(&ws.project_path));
                            }
                        }
                    }
                }
                None
            }
            None => None,
        };
        path.map(|p| resolve_repo_root(&p))
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

/// ワークツリーパスからリポジトリルートパスを解決する
///
/// ワークツリーの場合、git dir 内の commondir ファイルから
/// 元のリポジトリの .git ディレクトリを辿り、そのルートを返す。
/// 通常のリポジトリの場合は .git の親ディレクトリを返す。
/// 解決できない場合は元のパスをそのまま返す。
fn resolve_repo_root(path: &str) -> String {
    let Ok(repo) = git2::Repository::open(Path::new(path)) else {
        return path.to_string();
    };

    let git_dir = if repo.is_worktree() {
        // ワークツリーの場合: repo.path() は .git/worktrees/<name>/
        // commondir ファイルに共有 .git ディレクトリへの相対パスが記載されている
        let commondir_file = repo.path().join("commondir");
        let Ok(content) = std::fs::read_to_string(&commondir_file) else {
            return path.to_string();
        };
        repo.path().join(content.trim()).canonicalize().ok()
    } else {
        // 通常のリポジトリ: repo.path() は .git/
        Some(repo.path().to_path_buf())
    };

    // .git ディレクトリの親がリポジトリルート
    git_dir
        .as_deref()
        .and_then(|d| d.parent())
        .and_then(|r| r.to_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| path.to_string())
}

/// Normalize a path by expanding ~ to home directory
fn normalize_path(path: &str) -> String {
    if path.starts_with("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return format!("{}{}", home.to_string_lossy(), &path[1..]);
        }
    }
    path.to_string()
}

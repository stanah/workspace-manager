use crate::workspace::{
    AiTool, Pane, Session, SessionStatus, Workspace, WorktreeManager, get_default_search_paths,
    scan_for_repositories,
};
use ratatui::widgets::TableState;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::Instant;

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
    /// アクティブセッションがあるワークスペースのみ表示
    RunningOnly,
}

impl ListDisplayMode {
    /// 次の表示モードに切り替え
    pub fn next(self) -> Self {
        match self {
            Self::Worktrees => Self::WithBranches,
            Self::WithBranches => Self::RunningOnly,
            Self::RunningOnly => Self::Worktrees,
        }
    }

    /// 表示用ラベル
    pub fn label(&self) -> &'static str {
        match self {
            Self::Worktrees => "Worktrees",
            Self::WithBranches => "+Branches",
            Self::RunningOnly => "Running",
        }
    }
}

/// Yaziに送信するコマンド
#[derive(Debug, Clone)]
pub enum YaziCommand {
    /// ya emit cd <path>
    Cd(std::path::PathBuf),
    /// ya emit reveal <path>
    Reveal(std::path::PathBuf),
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
    /// ペイン（マルチプレクサのペイン）
    Pane {
        pane_index: usize,
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
    /// 区切り線
    Separator,
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
    /// 検出されたペイン一覧
    pub panes: Vec<Pane>,
    /// pane_id -> pane index のマッピング
    pane_map: HashMap<String, usize>,
    /// workspace_index -> pane indices のマッピング
    panes_by_workspace: HashMap<usize, Vec<usize>>,
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
    /// Nerd Fontアイコンを使用するか
    pub use_nerd_font: bool,
    /// タブ名テンプレート（マッチング用）
    pub tab_name_template: String,
    /// お気に入りリポジトリ（repo_key のセット）
    pub favorite_repos: HashSet<String>,
    /// Yazi連携: デバウンス中のコマンド (発火時刻, コマンド)
    pub pending_yazi: Option<(Instant, YaziCommand)>,
    /// Yazi連携: 最後に送信したコマンドのパス（重複送信防止）
    pub last_yazi_path: Option<std::path::PathBuf>,
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
            panes: Vec::new(),
            pane_map: HashMap::new(),
            panes_by_workspace: HashMap::new(),
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
            use_nerd_font: true,
            tab_name_template: "{repo}/{branch}".to_string(),
            favorite_repos: HashSet::new(),
            pending_yazi: None,
            last_yazi_path: None,
        }
    }

    /// テンプレートからタブ名を生成
    fn generate_tab_name(&self, repo_name: &str, branch: &str) -> String {
        self.tab_name_template
            .replace("{repo}", repo_name)
            .replace("{branch}", branch)
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
            // RunningOnly モードでは、アクティブセッションがないワークスペースをスキップ
            if self.list_display_mode == ListDisplayMode::RunningOnly {
                let sessions = self.sessions_for_workspace(idx);
                let panes = self.panes_for_workspace(idx);
                if sessions.is_empty() && panes.is_empty() {
                    continue;
                }
            }

            // 親リポジトリのパスを推定（worktreeの場合は親ディレクトリ）
            let repo_key = self.get_repo_key(ws);
            repo_groups.entry(repo_key.clone()).or_default().push(idx);
            // 最初のワークスペースのパスを保存
            repo_paths
                .entry(repo_key)
                .or_insert_with(|| ws.project_path.clone());
        }

        // お気に入りを先頭に、それ以外を後に
        let mut fav_keys: Vec<_> = repo_groups.keys()
            .filter(|k| self.favorite_repos.contains(k.as_str()))
            .cloned().collect();
        let mut other_keys: Vec<_> = repo_groups.keys()
            .filter(|k| !self.favorite_repos.contains(k.as_str()))
            .cloned().collect();
        fav_keys.sort();
        other_keys.sort();

        let has_favorites = !fav_keys.is_empty();
        let has_others = !other_keys.is_empty();
        let mut repo_keys = fav_keys;
        if has_favorites && has_others {
            repo_keys.push("__separator__".to_string());
        }
        repo_keys.extend(other_keys);

        for repo_key in repo_keys {
            if repo_key == "__separator__" {
                self.tree_items.push(TreeItem::Separator);
                continue;
            }
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
                if self.list_display_mode == ListDisplayMode::WithBranches {
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

                    // ペインがあればペインベース表示、なければセッションフォールバック
                    let workspace_panes = self.panes_for_workspace(ws_idx);
                    let parent_last = is_last_in_group;

                    if !workspace_panes.is_empty() {
                        for (pane_idx_pos, &pane_idx) in workspace_panes.iter().enumerate() {
                            self.tree_items.push(TreeItem::Pane {
                                pane_index: pane_idx,
                                is_last: pane_idx_pos == workspace_panes.len() - 1,
                                parent_is_last: parent_last,
                            });
                        }
                    } else {
                        for (sess_idx_pos, &sess_idx) in workspace_sessions.iter().enumerate() {
                            self.tree_items.push(TreeItem::Session {
                                session_index: sess_idx,
                                is_last: sess_idx_pos == workspace_sessions.len() - 1,
                                parent_is_last: parent_last,
                            });
                        }
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
    pub fn get_repo_key(&self, ws: &Workspace) -> String {
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

    // ===== Pane management =====

    /// PaneInfo リストからペイン状態を更新（差分処理）
    pub fn update_panes(&mut self, pane_infos: &[crate::multiplexer::PaneInfo]) {
        use crate::workspace::pane::AiSessionInfo;

        let mut new_panes: Vec<Pane> = Vec::new();
        let mut new_pane_map: HashMap<String, usize> = HashMap::new();
        let mut new_panes_by_workspace: HashMap<usize, Vec<usize>> = HashMap::new();

        for info in pane_infos {
            let workspace_index = self.find_workspace_by_cwd(&info.cwd);
            let Some(workspace_index) = workspace_index else {
                continue;
            };

            let pane_index = new_panes.len();

            // 既存ペインの AI セッション情報を引き継ぐ
            let prev_ai_session = self.pane_map
                .get(&info.pane_id)
                .and_then(|&idx| self.panes.get(idx))
                .and_then(|p| p.ai_session.clone());

            // AI ツール検出
            let ai_session = if let Some(tool) = Pane::detect_ai_tool(&info.command) {
                Some(prev_ai_session.unwrap_or_else(|| AiSessionInfo {
                    tool,
                    status: SessionStatus::Idle,
                    state_detail: None,
                    summary: None,
                    current_task: None,
                    last_activity: Some(std::time::SystemTime::now()),
                    external_id: None,
                }))
            } else {
                None
            };

            new_panes.push(Pane {
                pane_id: info.pane_id.clone(),
                workspace_index,
                window_name: info.window_name.clone(),
                window_index: info.window_index,
                pane_index: info.pane_index,
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
        let mut best_match: Option<(usize, usize)> = None;

        for (idx, ws) in self.workspaces.iter().enumerate() {
            let ws_path = &ws.project_path;
            if cwd_str.starts_with(ws_path)
                && (cwd_str.len() == ws_path.len()
                    || cwd_str.as_bytes().get(ws_path.len()) == Some(&b'/'))
            {
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
    pub fn workspace_aggregate_status_from_panes(&self, workspace_index: usize) -> SessionStatus {
        let pane_indices = self.panes_for_workspace(workspace_index);
        if pane_indices.is_empty() {
            return SessionStatus::Disconnected;
        }

        let mut has_working = false;
        let mut has_needs_input = false;
        let mut has_idle = false;

        for &idx in &pane_indices {
            if let Some(pane) = self.panes.get(idx) {
                if let Some(status) = pane.ai_status() {
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

    /// 現在選択中のペインを取得
    pub fn selected_pane(&self) -> Option<&Pane> {
        match self.tree_items.get(self.selected_index) {
            Some(TreeItem::Pane { pane_index, .. }) => self.panes.get(*pane_index),
            _ => None,
        }
    }

    /// 選択中のアイテムが属するリポジトリキーを取得
    pub fn selected_repo_key(&self) -> Option<String> {
        match self.tree_items.get(self.selected_index) {
            Some(TreeItem::RepoGroup { path, .. }) => Some(path.clone()),
            Some(TreeItem::Worktree { workspace_index, .. }) => {
                self.workspaces.get(*workspace_index).map(|ws| self.get_repo_key(ws))
            }
            Some(TreeItem::Session { session_index, .. }) => {
                self.sessions.get(*session_index).and_then(|s| {
                    self.workspaces.get(s.workspace_index).map(|ws| self.get_repo_key(ws))
                })
            }
            Some(TreeItem::Pane { pane_index, .. }) => {
                self.panes.get(*pane_index).and_then(|p| {
                    self.workspaces.get(p.workspace_index).map(|ws| self.get_repo_key(ws))
                })
            }
            _ => None,
        }
    }

    /// お気に入りをトグル
    pub fn toggle_favorite(&mut self, repo_key: &str) {
        if self.favorite_repos.contains(repo_key) {
            self.favorite_repos.remove(repo_key);
        } else {
            self.favorite_repos.insert(repo_key.to_string());
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
            let mut new_index = self.selected_index - 1;
            // Separator をスキップ
            if matches!(self.tree_items.get(new_index), Some(TreeItem::Separator)) && new_index > 0 {
                new_index -= 1;
            }
            self.set_selected_index(new_index);
        }
    }

    /// 選択を下に移動
    pub fn move_down(&mut self) {
        if !self.tree_items.is_empty() && self.selected_index < self.tree_items.len() - 1 {
            let mut new_index = self.selected_index + 1;
            // Separator をスキップ
            if matches!(self.tree_items.get(new_index), Some(TreeItem::Separator))
                && new_index < self.tree_items.len() - 1
            {
                new_index += 1;
            }
            self.set_selected_index(new_index);
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
            | Some(TreeItem::Pane { .. })
            | Some(TreeItem::Branch { .. }) => {
                // 子アイテム: 親RepoGroupへ移動
                self.move_to_parent_repo_group();
            }
            Some(TreeItem::Separator) | None => {}
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
            | Some(TreeItem::Pane { .. })
            | Some(TreeItem::Branch { .. }) => {
                // 子アイテム: 親RepoGroupに移動して折りたたみ
                if let Some(parent_idx) = self.find_parent_repo_group_index() {
                    self.set_selected_index(parent_idx);
                    if let Some(TreeItem::RepoGroup { path, .. }) = self.tree_items.get(parent_idx).cloned() {
                        self.collapsed_repos.insert(path);
                    }
                }
            }
            Some(TreeItem::Separator) | None => {}
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
            Some(TreeItem::Pane { pane_index, .. }) => {
                self.panes.get(*pane_index).and_then(|p| {
                    self.workspaces.get(p.workspace_index)
                })
            }
            Some(TreeItem::RepoGroup { .. })
            | Some(TreeItem::Branch { .. })
            | Some(TreeItem::RemoteBranchGroup { .. })
            | Some(TreeItem::Separator) => {
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

    /// 選択中のワークスペースのブランチ名を取得
    pub fn selected_workspace_branch(&self) -> Option<String> {
        match self.tree_items.get(self.selected_index) {
            Some(TreeItem::Worktree { workspace_index, .. }) => {
                self.workspaces.get(*workspace_index).map(|ws| ws.branch.clone())
            }
            Some(TreeItem::Session { session_index, .. }) => {
                self.sessions.get(*session_index).and_then(|s| {
                    self.workspaces.get(s.workspace_index).map(|ws| ws.branch.clone())
                })
            }
            Some(TreeItem::Pane { pane_index, .. }) => {
                self.panes.get(*pane_index).and_then(|p| {
                    self.workspaces.get(p.workspace_index).map(|ws| ws.branch.clone())
                })
            }
            Some(TreeItem::RepoGroup { .. }) => {
                // グループ内の最初のワークスペースのブランチを返す
                for item in self.tree_items.iter().skip(self.selected_index + 1) {
                    match item {
                        TreeItem::Worktree { workspace_index, .. } => {
                            return self.workspaces.get(*workspace_index).map(|ws| ws.branch.clone());
                        }
                        TreeItem::RepoGroup { .. } => break,
                        _ => continue,
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// 新規worktree作成ダイアログを開く
    pub fn open_create_worktree_dialog(&mut self) {
        let base_branch = self.selected_workspace_branch();
        self.input_dialog = Some(InputDialog::new_create_worktree(base_branch));
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

    /// ワークスペースがタブとして開いているか確認
    /// tab_name_template に基づいてマッチング
    pub fn is_workspace_open(&self, repo_name: &str, branch: &str) -> bool {
        // パターン1: テンプレートによるタブ名
        let pattern1 = self.generate_tab_name(repo_name, branch);
        // パターン2: ブランチ名のみ
        let pattern2 = branch;
        // パターン3: "__" 形式のrepo名の場合、ベース名でテンプレート適用
        let base_repo = repo_name.split("__").next().unwrap_or(repo_name);
        let pattern3 = self.generate_tab_name(base_repo, branch);

        self.open_tabs.contains(&pattern1)
            || self.open_tabs.contains(pattern2)
            || self.open_tabs.contains(&pattern3)
    }

    /// タブ名でワークスペースを選択
    /// tab_name_template に基づいて各ワークスペースと照合する
    pub fn select_by_tab_name(&mut self, tab_name: &str) -> bool {
        for (idx, item) in self.tree_items.iter().enumerate() {
            if let TreeItem::Worktree { workspace_index, .. } = item {
                if let Some(ws) = self.workspaces.get(*workspace_index) {
                    let pattern1 = self.generate_tab_name(&ws.repo_name, &ws.branch);
                    let base_repo = ws.repo_name.split("__").next().unwrap_or(&ws.repo_name);
                    let pattern2 = self.generate_tab_name(base_repo, &ws.branch);

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
            Some(TreeItem::Pane { pane_index, .. }) => {
                self.panes.get(*pane_index).and_then(|p| {
                    self.workspaces
                        .get(p.workspace_index)
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
            Some(TreeItem::Separator) | None => None,
        };
        path.map(|p| resolve_repo_root(&p))
    }

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

    /// Yaziデバウンスタイマーをセットする（前回と同じパスならスキップ）
    pub fn schedule_yazi(&mut self, debounce_ms: u64) {
        if let Some(cmd) = self.resolve_yazi_command() {
            let new_path = match &cmd {
                YaziCommand::Cd(p) | YaziCommand::Reveal(p) => p.clone(),
            };
            if self.last_yazi_path.as_ref() == Some(&new_path) {
                return;
            }
            let deadline = Instant::now() + std::time::Duration::from_millis(debounce_ms);
            self.pending_yazi = Some((deadline, cmd));
        }
    }

    /// Yaziデバウンスタイマーが発火可能か確認し、発火する
    pub fn fire_yazi_if_ready(&mut self, client_id: u64) {
        if let Some((deadline, _)) = &self.pending_yazi {
            if Instant::now() >= *deadline {
                if let Some((_, cmd)) = self.pending_yazi.take() {
                    let path = match &cmd {
                        YaziCommand::Cd(p) | YaziCommand::Reveal(p) => p.clone(),
                    };
                    let args: Vec<String> = match &cmd {
                        YaziCommand::Cd(p) => vec![
                            "emit-to".to_string(),
                            client_id.to_string(),
                            "cd".to_string(),
                            p.to_string_lossy().to_string(),
                        ],
                        YaziCommand::Reveal(p) => vec![
                            "emit-to".to_string(),
                            client_id.to_string(),
                            "reveal".to_string(),
                            p.to_string_lossy().to_string(),
                        ],
                    };
                    match std::process::Command::new("ya").args(&args).spawn() {
                        Ok(_) => {
                            tracing::debug!("Sent yazi command: ya {}", args.join(" "));
                            self.last_yazi_path = Some(path);
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

#[cfg(test)]
mod yazi_tests {
    use super::*;

    #[test]
    fn test_resolve_yazi_command_empty_tree() {
        let state = AppState::new();
        assert!(state.resolve_yazi_command().is_none());
    }

    #[test]
    fn test_schedule_yazi_no_items() {
        let mut state = AppState::new();
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

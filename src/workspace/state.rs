use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// ワークスペース情報（Git worktree単位）
///
/// Note: セッション情報（status, session_id等）は Session 構造体に移動しました。
/// 1つのワークスペースに複数のセッションを持てるようになりました。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    /// 一意識別子
    pub id: Uuid,
    /// プロジェクトパス
    pub project_path: String,
    /// リポジトリ名
    pub repo_name: String,
    /// ブランチ名
    pub branch: String,
    /// 最終更新時刻
    pub updated_at: std::time::SystemTime,
}

impl Workspace {
    /// 新規ワークスペースを作成
    pub fn new(project_path: String, repo_name: String, branch: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            project_path,
            repo_name,
            branch,
            updated_at: std::time::SystemTime::now(),
        }
    }

    /// 表示用の短縮パスを返す
    pub fn display_path(&self) -> String {
        if let Some(base_dirs) = directories::BaseDirs::new() {
            let home = base_dirs.home_dir();
            if let Some(stripped) = self.project_path.strip_prefix(home.to_string_lossy().as_ref())
            {
                return format!("~{}", stripped);
            }
        }
        self.project_path.clone()
    }
}

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Worktreeパステンプレート
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WorktreePathStyle {
    /// リポジトリと並列に配置: {repo_parent}/{repo}={branch}
    /// 例: ~/work/config=feature-branch
    Parallel,
    /// ghq形式で配置: {ghq_root}/{host}/{owner}/{repo}={branch}
    /// 例: ~/ghq/github.com/stanah/config=feature-branch
    Ghq,
    /// リポジトリ内の.worktreesディレクトリ: {repo}/.worktrees/{branch}
    Subdirectory,
    /// カスタムテンプレート
    Custom(String),
}

impl Default for WorktreePathStyle {
    fn default() -> Self {
        WorktreePathStyle::Parallel
    }
}

/// Worktree設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeConfig {
    /// パス生成スタイル
    pub path_style: WorktreePathStyle,
    /// ghq root（Ghqスタイル使用時）
    pub ghq_root: Option<PathBuf>,
    /// デフォルトのリモート
    pub default_remote: String,
}

impl Default for WorktreeConfig {
    fn default() -> Self {
        // ghq rootを自動検出
        let ghq_root = std::env::var("GHQ_ROOT")
            .ok()
            .map(PathBuf::from)
            .or_else(|| {
                dirs::home_dir().map(|h| h.join("ghq"))
            });

        Self {
            path_style: WorktreePathStyle::Parallel,
            ghq_root,
            default_remote: "origin".to_string(),
        }
    }
}

impl WorktreeConfig {
    /// worktreeのパスを生成
    pub fn generate_worktree_path(
        &self,
        repo_path: &std::path::Path,
        branch: &str,
        remote_url: Option<&str>,
    ) -> PathBuf {
        let safe_branch = branch.replace('/', "-");

        match &self.path_style {
            WorktreePathStyle::Parallel => {
                // リポジトリと同じ親ディレクトリに配置
                let repo_name = repo_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("repo");
                let parent = repo_path.parent().unwrap_or(repo_path);
                parent.join(format!("{}={}", repo_name, safe_branch))
            }
            WorktreePathStyle::Ghq => {
                // ghq形式: {ghq_root}/{host}/{owner}/{repo}={branch}
                if let (Some(ghq_root), Some(url)) = (&self.ghq_root, remote_url) {
                    if let Some((host, owner, repo)) = parse_git_url(url) {
                        return ghq_root.join(host).join(owner).join(format!("{}={}", repo, safe_branch));
                    }
                }
                // フォールバック: Parallelスタイル
                let repo_name = repo_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("repo");
                let parent = repo_path.parent().unwrap_or(repo_path);
                parent.join(format!("{}={}", repo_name, safe_branch))
            }
            WorktreePathStyle::Subdirectory => {
                repo_path.join(".worktrees").join(&safe_branch)
            }
            WorktreePathStyle::Custom(template) => {
                let repo_name = repo_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("repo");
                let path_str = template
                    .replace("{repo}", repo_name)
                    .replace("{branch}", &safe_branch)
                    .replace("{repo_path}", &repo_path.to_string_lossy());
                PathBuf::from(path_str)
            }
        }
    }
}

/// Git URLをパース (host, owner, repo)
fn parse_git_url(url: &str) -> Option<(String, String, String)> {
    // SSH形式: git@github.com:owner/repo.git
    if let Some(rest) = url.strip_prefix("git@") {
        let parts: Vec<&str> = rest.splitn(2, ':').collect();
        if parts.len() == 2 {
            let host = parts[0].to_string();
            let path = parts[1].trim_end_matches(".git");
            let path_parts: Vec<&str> = path.split('/').collect();
            if path_parts.len() >= 2 {
                return Some((
                    host,
                    path_parts[0].to_string(),
                    path_parts[1].to_string(),
                ));
            }
        }
    }
    // HTTPS形式: https://github.com/owner/repo.git
    if url.starts_with("https://") || url.starts_with("http://") {
        let without_protocol = url
            .strip_prefix("https://")
            .or_else(|| url.strip_prefix("http://"))
            .unwrap_or(url);
        let parts: Vec<&str> = without_protocol.split('/').collect();
        if parts.len() >= 3 {
            let host = parts[0].to_string();
            let owner = parts[1].to_string();
            let repo = parts[2].trim_end_matches(".git").to_string();
            return Some((host, owner, repo));
        }
    }
    None
}

/// アプリケーション設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// 検索対象ディレクトリ
    pub search_paths: Vec<PathBuf>,
    /// 最大検索深度
    pub max_scan_depth: usize,
    /// MCPサーバーソケットパス
    pub socket_path: PathBuf,
    /// ログレベル
    pub log_level: String,
    /// Zellij連携設定
    pub zellij: ZellijConfig,
    /// Worktree設定
    pub worktree: WorktreeConfig,
}

impl Default for Config {
    fn default() -> Self {
        let socket_path = std::env::temp_dir().join("workspace-manager.sock");

        Self {
            search_paths: crate::workspace::get_default_search_paths(),
            max_scan_depth: 3,
            socket_path,
            log_level: "info".to_string(),
            zellij: ZellijConfig::default(),
            worktree: WorktreeConfig::default(),
        }
    }
}

impl Config {
    /// 設定ファイルから読み込み
    pub fn load() -> Result<Self> {
        let config_path = Self::config_path()?;

        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)?;
            let config: Config = toml::from_str(&content)
                .map_err(|e| anyhow::anyhow!("Failed to parse config: {}", e))?;
            Ok(config)
        } else {
            Ok(Self::default())
        }
    }

    /// 設定ファイルパスを取得
    pub fn config_path() -> Result<PathBuf> {
        let dirs = directories::ProjectDirs::from("", "", "workspace-manager")
            .ok_or_else(|| anyhow::anyhow!("Failed to determine config directory"))?;

        Ok(dirs.config_dir().join("config.toml"))
    }

    /// デフォルト設定をファイルに保存
    pub fn save_default() -> Result<()> {
        let config_path = Self::config_path()?;
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let config = Self::default();
        let content = toml::to_string_pretty(&config)?;
        std::fs::write(config_path, content)?;

        Ok(())
    }
}

mod dirs {
    pub fn home_dir() -> Option<std::path::PathBuf> {
        std::env::var_os("HOME").map(std::path::PathBuf::from)
    }
}

/// Zellij連携設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZellijConfig {
    /// Zellij連携を有効にするか
    pub enabled: bool,
    /// 対象セッション名（未設定時は選択ダイアログを表示）
    pub session_name: Option<String>,
    /// デフォルトレイアウトファイル
    pub default_layout: Option<PathBuf>,
    /// レイアウトディレクトリ（選択用）
    pub layout_dir: Option<PathBuf>,
    /// タブ名テンプレート（{repo}, {branch} を置換）
    pub tab_name_template: String,
}

impl Default for ZellijConfig {
    fn default() -> Self {
        let layout_dir = directories::ProjectDirs::from("", "", "zellij")
            .map(|d| d.config_dir().join("layouts"))
            .or_else(|| dirs::home_dir().map(|h| h.join(".config/zellij/layouts")));

        Self {
            enabled: true,
            session_name: None,
            default_layout: None,
            layout_dir,
            tab_name_template: "{repo}/{branch}".to_string(),
        }
    }
}

impl ZellijConfig {
    /// テンプレートからタブ名を生成
    pub fn generate_tab_name(&self, repo: &str, branch: &str) -> String {
        self.tab_name_template
            .replace("{repo}", repo)
            .replace("{branch}", branch)
    }
}

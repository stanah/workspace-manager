use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Worktreeパステンプレート
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WorktreePathStyle {
    /// リポジトリと並列に配置: {repo_parent}/{repo}__{branch}
    /// 例: ~/work/config__feature-branch
    Parallel,
    /// ghq形式で配置: {ghq_root}/{host}/{owner}/{repo}__{branch}
    /// 例: ~/ghq/github.com/stanah/config__feature-branch
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
    /// リモートブランチの最大表示数（0で無制限）
    #[serde(default = "default_max_remote_branches")]
    pub max_remote_branches: usize,
}

fn default_max_remote_branches() -> usize {
    10
}

impl Default for WorktreeConfig {
    fn default() -> Self {
        // ghq rootを自動検出
        let ghq_root = std::env::var("GHQ_ROOT")
            .ok()
            .map(PathBuf::from)
            .or_else(|| {
                directories::BaseDirs::new().map(|d| d.home_dir().join("ghq"))
            });

        Self {
            path_style: WorktreePathStyle::Parallel,
            ghq_root,
            default_remote: "origin".to_string(),
            max_remote_branches: default_max_remote_branches(),
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
                parent.join(format!("{}__{}", repo_name, safe_branch))
            }
            WorktreePathStyle::Ghq => {
                // ghq形式: {ghq_root}/{host}/{owner}/{repo}__{branch}
                if let (Some(ghq_root), Some(url)) = (&self.ghq_root, remote_url) {
                    if let Some((host, owner, repo)) = parse_git_url(url) {
                        return ghq_root.join(host).join(owner).join(format!("{}__{}", repo, safe_branch));
                    }
                }
                // フォールバック: Parallelスタイル
                let repo_name = repo_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("repo");
                let parent = repo_path.parent().unwrap_or(repo_path);
                parent.join(format!("{}__{}", repo_name, safe_branch))
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
    /// エディタコマンド（code, cursor, vim など）
    #[serde(default = "default_editor")]
    pub editor: String,
    /// Zellij連携設定
    pub zellij: ZellijConfig,
    /// Worktree設定
    pub worktree: WorktreeConfig,
    /// Log watch設定
    #[serde(default)]
    pub logwatch: LogWatchConfig,
}

fn default_editor() -> String {
    "code".to_string()
}

impl Default for Config {
    fn default() -> Self {
        let socket_path = std::env::temp_dir().join("workspace-manager.sock");

        Self {
            search_paths: crate::workspace::get_default_search_paths(),
            max_scan_depth: 3,
            socket_path,
            log_level: "info".to_string(),
            editor: default_editor(),
            zellij: ZellijConfig::default(),
            worktree: WorktreeConfig::default(),
            logwatch: LogWatchConfig::default(),
        }
    }
}

/// Log watch configuration for CLI status tracking
///
/// ## Architecture
/// - Claude Code: Event-driven via hooks (no AI analysis, no polling)
/// - Kiro CLI: SQLite polling (no hooks needed, reads from database)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogWatchConfig {
    /// Enable log watching and analysis
    #[serde(default = "default_logwatch_enabled")]
    pub enabled: bool,

    // === Claude Code Settings ===
    /// Enable Claude Code hooks integration
    #[serde(default = "default_claude_hooks_enabled")]
    pub claude_hooks_enabled: bool,
    /// Claude home directory (for log reading if needed)
    #[serde(default = "default_claude_home")]
    pub claude_home: PathBuf,

    // === Kiro CLI Settings ===
    /// Enable Kiro CLI SQLite polling
    #[serde(default = "default_kiro_polling_enabled")]
    pub kiro_polling_enabled: bool,
    /// Kiro polling interval in seconds
    #[serde(default = "default_kiro_polling_interval")]
    pub kiro_polling_interval_secs: u64,
    /// Path to Kiro CLI SQLite database
    #[serde(default = "default_kiro_db_path")]
    pub kiro_db_path: PathBuf,

    // === Legacy Settings (for backwards compatibility) ===
    /// CLI tool to use for analysis ("claude" or "kiro") - DEPRECATED
    #[serde(default = "default_analyzer_tool")]
    pub analyzer_tool: String,
    /// Interval between analyses in seconds - DEPRECATED, use kiro_polling_interval_secs
    #[serde(default = "default_analysis_interval")]
    pub analysis_interval_secs: u64,
    /// Maximum log lines to analyze
    #[serde(default = "default_max_log_lines")]
    pub max_log_lines: usize,
    /// Kiro logs directory (optional) - DEPRECATED, use kiro_db_path
    pub kiro_logs_dir: Option<PathBuf>,
    /// Use heuristic analysis instead of AI (for testing/offline) - DEPRECATED
    #[serde(default)]
    pub use_heuristic: bool,
    /// Enable periodic polling - DEPRECATED, use kiro_polling_enabled
    #[serde(default = "default_polling_enabled")]
    pub polling_enabled: bool,
}

fn default_claude_hooks_enabled() -> bool {
    true
}

fn default_kiro_polling_enabled() -> bool {
    true
}

fn default_kiro_polling_interval() -> u64 {
    10 // Poll every 10 seconds
}

fn default_kiro_db_path() -> PathBuf {
    directories::BaseDirs::new()
        .map(|d| d.home_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("Library/Application Support/kiro-cli/data.sqlite3")
}

fn default_polling_enabled() -> bool {
    true
}

fn default_logwatch_enabled() -> bool {
    true // Enabled by default (no API costs with new architecture)
}

fn default_analyzer_tool() -> String {
    "claude".to_string()
}

fn default_analysis_interval() -> u64 {
    10
}

fn default_max_log_lines() -> usize {
    500
}

fn default_claude_home() -> PathBuf {
    directories::BaseDirs::new()
        .map(|d| d.home_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".claude")
}

impl Default for LogWatchConfig {
    fn default() -> Self {
        let claude_home = default_claude_home();

        // Kiro logs on macOS (legacy)
        let kiro_logs_dir = if cfg!(target_os = "macos") {
            directories::BaseDirs::new().map(|d| {
                d.home_dir().join("Library/Application Support/Kiro/logs/kiroAgent")
            })
        } else {
            None
        };

        Self {
            enabled: default_logwatch_enabled(),
            // Claude Code settings
            claude_hooks_enabled: default_claude_hooks_enabled(),
            claude_home,
            // Kiro CLI settings
            kiro_polling_enabled: default_kiro_polling_enabled(),
            kiro_polling_interval_secs: default_kiro_polling_interval(),
            kiro_db_path: default_kiro_db_path(),
            // Legacy settings
            analyzer_tool: default_analyzer_tool(),
            analysis_interval_secs: default_analysis_interval(),
            max_log_lines: default_max_log_lines(),
            kiro_logs_dir,
            use_heuristic: false,
            polling_enabled: default_polling_enabled(),
        }
    }
}

impl Config {
    /// 設定ファイルから読み込み（存在しない場合はデフォルトを作成して保存）
    pub fn load() -> Result<Self> {
        let config_path = Self::config_path()?;

        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)?;
            let config: Config = toml::from_str(&content)
                .map_err(|e| anyhow::anyhow!("Failed to parse config: {}", e))?;
            Ok(config)
        } else {
            // 初回起動時はデフォルト設定をファイルに保存
            let config = Self::default();
            if let Err(e) = config.save() {
                tracing::warn!("Failed to save default config: {}", e);
            }
            Ok(config)
        }
    }

    /// 設定ファイルパスを取得
    pub fn config_path() -> Result<PathBuf> {
        // ~/.config/workspace-manager/config.toml を使用
        let base_dirs = directories::BaseDirs::new()
            .ok_or_else(|| anyhow::anyhow!("Failed to determine home directory"))?;
        Ok(base_dirs.home_dir().join(".config/workspace-manager/config.toml"))
    }

    /// 現在の設定をファイルに保存
    pub fn save(&self) -> Result<()> {
        let config_path = Self::config_path()?;
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = toml::to_string_pretty(self)?;
        std::fs::write(config_path, content)?;

        Ok(())
    }

    /// Zellijセッション名を更新して保存
    pub fn save_zellij_session(&mut self, session_name: String) -> Result<()> {
        self.zellij.session_name = Some(session_name);
        self.save()
    }

    /// Zellijデフォルトレイアウトを更新して保存
    pub fn save_zellij_layout(&mut self, layout_path: PathBuf) -> Result<()> {
        self.zellij.default_layout = Some(layout_path);
        self.save()
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
    /// AIコマンド（claude, kiro-cli, codex など）
    pub ai_command: String,
}

impl Default for ZellijConfig {
    fn default() -> Self {
        // workspace-manager のレイアウトディレクトリを使用
        let layout_dir = directories::BaseDirs::new().map(|d| d.home_dir().join(".config/workspace-manager/layouts"));

        Self {
            enabled: true,
            session_name: None,
            default_layout: None,
            layout_dir,
            tab_name_template: "{repo}/{branch}".to_string(),
            ai_command: "claude".to_string(),
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

    /// レイアウトディレクトリを取得（なければ作成）
    pub fn ensure_layout_dir(&self) -> Result<PathBuf> {
        let layout_dir = self.layout_dir.clone()
            .or_else(|| directories::BaseDirs::new().map(|d| d.home_dir().join(".config/workspace-manager/layouts")))
            .ok_or_else(|| anyhow::anyhow!("Failed to determine layout directory"))?;

        if !layout_dir.exists() {
            std::fs::create_dir_all(&layout_dir)?;
        }

        Ok(layout_dir)
    }

    /// 組み込みレイアウトをテンプレートから生成
    pub fn generate_builtin_layouts(&self) -> Result<()> {
        let layout_dir = self.ensure_layout_dir()?;
        let ai_cmd = &self.ai_command;

        // 組み込みテンプレート
        let templates = [
            ("simple", include_str!("../../layouts/simple.kdl.template")),
            ("with-shell", include_str!("../../layouts/with-shell.kdl.template")),
            ("dev", include_str!("../../layouts/dev.kdl.template")),
        ];

        for (name, template) in templates {
            let content = template.replace("{{AI_COMMAND}}", ai_cmd);
            let path = layout_dir.join(format!("{}.kdl", name));
            std::fs::write(&path, content)?;
        }

        Ok(())
    }
}

pub mod tmux;
pub mod zellij;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// マルチプレクサバックエンドの種類
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MultiplexerBackend {
    Zellij,
    Tmux,
    None,
}

/// ウィンドウ/タブ操作の結果
#[derive(Debug, Clone)]
pub enum WindowActionResult {
    /// 既存ウィンドウ/タブに切り替えた
    SwitchedToExisting(String),
    /// 新規ウィンドウ/タブを作成した
    CreatedNew(String),
    /// セッションが見つからない
    SessionNotFound(String),
}

/// マルチプレクサの共通インターフェース
///
/// Zellij と tmux の両方を統一的に扱うための trait。
/// Zellij の「タブ」と tmux の「ウィンドウ」を同一の概念として扱う。
pub trait Multiplexer {
    /// マルチプレクサが利用可能か
    fn is_available(&self) -> bool;

    /// マルチプレクサ内部で実行中か
    fn is_internal(&self) -> bool;

    /// バックエンドの種類を返す
    fn backend(&self) -> MultiplexerBackend;

    /// セッション名を取得
    fn session_name(&self) -> Option<&str>;

    /// セッション名を設定
    fn set_session_name(&mut self, name: String);

    // === セッション・ウィンドウ管理 ===

    /// セッション一覧を取得
    fn list_sessions(&self) -> Result<Vec<String>>;

    /// 指定セッションのウィンドウ/タブ名一覧を取得
    fn query_window_names(&self, session: &str) -> Result<Vec<String>>;

    /// 指定ウィンドウ/タブに切り替え
    fn go_to_window(&self, session: &str, name: &str) -> Result<()>;

    /// 新規ウィンドウ/タブを作成
    fn new_window(
        &self,
        session: &str,
        name: &str,
        cwd: &Path,
        layout: Option<&Path>,
    ) -> Result<()>;

    /// ウィンドウ/タブを閉じる
    fn close_window(&self, session: &str, name: &str) -> Result<()>;

    /// ワークスペースをウィンドウ/タブとして開く（高レベルAPI）
    fn open_workspace_window(
        &self,
        name: &str,
        cwd: &Path,
        layout: Option<&Path>,
    ) -> Result<WindowActionResult>;

    /// レイアウトファイル一覧を取得
    fn list_layouts(&self, layout_dir: &Path) -> Result<Vec<String>>;

    // === ペイン操作 ===

    /// 指定ペインにフォーカス
    fn focus_pane(&self, pane_id: u32) -> Result<()>;

    /// ペインを閉じる
    fn close_pane(&self, pane_id: u32) -> Result<()>;

    /// 指定ディレクトリでコマンドを起動（新ペイン）
    fn launch_command(&self, cwd: &Path, command: &[&str]) -> Result<()>;

    // === tmux 固有（オプショナル） ===

    /// ペイン/ウィンドウにキーを送信（tmux のみ）
    fn send_keys(&self, target: &str, keys: &str) -> Result<()> {
        let _ = (target, keys);
        anyhow::bail!(
            "{:?} does not support send_keys",
            self.backend()
        )
    }

    /// ペインの出力を取得（tmux のみ）
    fn capture_pane(&self, target: &str) -> Result<String> {
        let _ = target;
        anyhow::bail!(
            "{:?} does not support capture_pane",
            self.backend()
        )
    }

    /// タブ切り替え後にシェルコマンドを実行（非同期spawn）
    fn run_post_select_command(command: &str) -> Result<()>
    where
        Self: Sized,
    {
        std::process::Command::new("sh")
            .args(["-c", command])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| anyhow::anyhow!("Failed to execute post_select_command: {}", e))?;
        Ok(())
    }
}

/// マルチプレクサ設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiplexerConfig {
    /// バックエンド選択: auto | zellij | tmux | none
    #[serde(default = "default_mux_backend")]
    pub backend: String,
    /// セッション名
    pub session_name: Option<String>,
    /// タブ/ウィンドウ名テンプレート
    #[serde(default = "default_tab_name_template")]
    pub tab_name_template: String,
    /// AIコマンド
    #[serde(default = "default_ai_command")]
    pub ai_command: String,
    /// デフォルトレイアウトファイル
    #[serde(default)]
    pub default_layout: Option<PathBuf>,
    /// レイアウトディレクトリ（選択用）
    #[serde(default)]
    pub layout_dir: Option<PathBuf>,
    /// タブ切り替え後に実行するコマンド
    #[serde(default)]
    pub post_select_command: Option<String>,
}

fn default_mux_backend() -> String {
    "auto".to_string()
}

fn default_tab_name_template() -> String {
    "{repo}/{branch}".to_string()
}

fn default_ai_command() -> String {
    "claude".to_string()
}

impl Default for MultiplexerConfig {
    fn default() -> Self {
        Self {
            backend: default_mux_backend(),
            session_name: None,
            tab_name_template: default_tab_name_template(),
            ai_command: default_ai_command(),
            default_layout: None,
            layout_dir: None,
            post_select_command: None,
        }
    }
}

impl MultiplexerConfig {
    /// テンプレートからタブ/ウィンドウ名を生成
    pub fn generate_tab_name(&self, repo: &str, branch: &str) -> String {
        self.tab_name_template
            .replace("{repo}", repo)
            .replace("{branch}", branch)
    }
}

/// ZellijConfig から MultiplexerConfig を作成（フォールバック用）
pub fn multiplexer_config_from_zellij(zellij: &crate::app::ZellijConfig) -> MultiplexerConfig {
    let backend = if zellij.enabled {
        "auto".to_string()
    } else {
        "none".to_string()
    };
    MultiplexerConfig {
        backend,
        session_name: zellij.session_name.clone(),
        tab_name_template: zellij.tab_name_template.clone(),
        ai_command: zellij.ai_command.clone(),
        default_layout: zellij.default_layout.clone(),
        layout_dir: zellij.layout_dir.clone(),
        post_select_command: zellij.post_select_command.clone(),
    }
}

/// 環境と設定から適切な Multiplexer バックエンドを生成
pub fn create_multiplexer(
    mux_config: Option<&MultiplexerConfig>,
    zellij_config: &crate::app::ZellijConfig,
) -> Box<dyn Multiplexer> {
    let backend_str = mux_config
        .map(|c| c.backend.as_str())
        .unwrap_or("auto");

    let session_name = mux_config
        .and_then(|c| c.session_name.clone())
        .or_else(|| zellij_config.session_name.clone());

    match backend_str {
        "zellij" => Box::new(zellij::ZellijMultiplexer::auto_detect(session_name)),
        "tmux" => Box::new(tmux::TmuxMultiplexer::auto_detect(session_name)),
        "none" => Box::new(zellij::ZellijMultiplexer::new_disabled()),
        _ => {
            // auto: 環境変数で自動検出
            if std::env::var("ZELLIJ").is_ok() {
                Box::new(zellij::ZellijMultiplexer::new_internal())
            } else if std::env::var("TMUX").is_ok() {
                Box::new(tmux::TmuxMultiplexer::new_internal())
            } else if let Some(session) = session_name {
                // 外部モード: zellij を優先（後方互換）
                if zellij_config.enabled {
                    Box::new(zellij::ZellijMultiplexer::new_external(session))
                } else {
                    Box::new(tmux::TmuxMultiplexer::new_external(session))
                }
            } else {
                Box::new(zellij::ZellijMultiplexer::new_disabled())
            }
        }
    }
}

// 後方互換の re-export
pub use self::zellij::ZellijMultiplexer;
pub use self::tmux::TmuxMultiplexer;

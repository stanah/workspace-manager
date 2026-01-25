use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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
    /// Zellijモード有効化
    pub zellij_enabled: bool,
}

impl Default for Config {
    fn default() -> Self {
        let socket_path = std::env::temp_dir().join("workspace-manager.sock");

        Self {
            search_paths: crate::workspace::get_default_search_paths(),
            max_scan_depth: 3,
            socket_path,
            log_level: "info".to_string(),
            zellij_enabled: std::env::var("ZELLIJ").is_ok(),
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

// tomlクレートが依存に無いので、Phase 4で追加
mod toml {
    use serde::{Deserialize, Serialize};

    pub fn from_str<'de, T: Deserialize<'de>>(_s: &'de str) -> Result<T, String> {
        Err("TOML parsing not implemented yet".to_string())
    }

    pub fn to_string_pretty<T: Serialize>(_value: &T) -> Result<String, std::fmt::Error> {
        Ok("# Configuration not yet implemented\n".to_string())
    }
}

use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

/// Zellij動作モード
#[derive(Debug, Clone)]
pub enum ZellijMode {
    /// Zellij内で実行（従来の動作）
    Internal,
    /// 外部から指定セッションを操作
    External { session_name: String },
}

/// タブ操作の結果
#[derive(Debug, Clone)]
pub enum TabActionResult {
    /// 既存タブに切り替えた
    SwitchedToExisting(String),
    /// 新規タブを作成した
    CreatedNew(String),
    /// セッションが見つからない
    SessionNotFound(String),
}

/// Zellij操作のラッパー
pub struct ZellijActions {
    /// 動作モード
    mode: ZellijMode,
}

impl ZellijActions {
    /// Zellij内で実行するインスタンスを作成
    pub fn new_internal() -> Self {
        Self {
            mode: ZellijMode::Internal,
        }
    }

    /// 外部から指定セッションを操作するインスタンスを作成
    pub fn new_external(session_name: String) -> Self {
        Self {
            mode: ZellijMode::External { session_name },
        }
    }

    /// 環境から自動検出してインスタンスを作成
    pub fn auto_detect(config_session: Option<String>) -> Self {
        if std::env::var("ZELLIJ").is_ok() {
            // Zellij内で実行中
            Self::new_internal()
        } else if let Some(session) = config_session {
            // 設定にセッション名がある場合
            Self::new_external(session)
        } else {
            // 外部モードだがセッション未設定
            Self {
                mode: ZellijMode::External {
                    session_name: String::new(),
                },
            }
        }
    }

    /// 従来のコンストラクタ（互換性維持）
    pub fn new() -> Self {
        Self::auto_detect(None)
    }

    /// Zellij機能が利用可能か確認
    pub fn is_available(&self) -> bool {
        match &self.mode {
            ZellijMode::Internal => true,
            ZellijMode::External { session_name } => !session_name.is_empty(),
        }
    }

    /// Zellij内で実行中か確認
    pub fn is_internal(&self) -> bool {
        matches!(self.mode, ZellijMode::Internal)
    }

    /// セッション名を取得（External modeのみ）
    pub fn session_name(&self) -> Option<&str> {
        match &self.mode {
            ZellijMode::External { session_name } if !session_name.is_empty() => {
                Some(session_name)
            }
            _ => None,
        }
    }

    /// セッション名を設定（External modeのみ）
    pub fn set_session_name(&mut self, name: String) {
        if let ZellijMode::External { session_name } = &mut self.mode {
            *session_name = name;
        }
    }

    // ========================================
    // セッション・タブ管理（External mode用）
    // ========================================

    /// Zellijセッション一覧を取得
    pub fn list_sessions(&self) -> Result<Vec<String>> {
        let output = Command::new("zellij")
            .args(["list-sessions", "--no-formatting"])
            .output()
            .context("Failed to execute zellij list-sessions")?;

        if !output.status.success() {
            // Zellijが実行されていない場合は空リストを返す
            return Ok(Vec::new());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let sessions: Vec<String> = stdout
            .lines()
            .map(|line| {
                // 形式: "session_name (EXITED - ...)" または "session_name (current)"
                // セッション名だけ抽出
                line.split_whitespace()
                    .next()
                    .unwrap_or(line)
                    .to_string()
            })
            .filter(|s| !s.is_empty())
            .collect();

        Ok(sessions)
    }

    /// 指定セッションが存在するか確認
    pub fn session_exists(&self, name: &str) -> Result<bool> {
        let sessions = self.list_sessions()?;
        Ok(sessions.iter().any(|s| s == name))
    }

    /// 指定セッションのタブ名一覧を取得
    pub fn query_tab_names(&self, session: &str) -> Result<Vec<String>> {
        let output = Command::new("zellij")
            .args(["--session", session, "action", "query-tab-names"])
            .output()
            .context("Failed to query tab names")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to query tab names: {}", stderr);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let tabs: Vec<String> = stdout
            .lines()
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty())
            .collect();

        Ok(tabs)
    }

    /// 指定タブに切り替え
    pub fn go_to_tab(&self, session: &str, tab_name: &str) -> Result<()> {
        let status = Command::new("zellij")
            .args(["--session", session, "action", "go-to-tab-name", tab_name])
            .status()
            .context("Failed to switch tab")?;

        if !status.success() {
            anyhow::bail!("Failed to switch to tab: {}", tab_name);
        }

        Ok(())
    }

    /// タブを閉じる
    pub fn close_tab(&self, session: &str, tab_name: &str) -> Result<()> {
        // まず対象タブに切り替え
        self.go_to_tab(session, tab_name)?;

        // タブを閉じる
        let status = Command::new("zellij")
            .args(["--session", session, "action", "close-tab"])
            .status()
            .context("Failed to close tab")?;

        if !status.success() {
            anyhow::bail!("Failed to close tab: {}", tab_name);
        }

        Ok(())
    }

    /// 新規タブを作成
    pub fn new_tab(
        &self,
        session: &str,
        tab_name: &str,
        cwd: &Path,
        layout: Option<&Path>,
    ) -> Result<()> {
        let cwd_str = cwd.to_string_lossy();
        let mut args = vec![
            "--session",
            session,
            "action",
            "new-tab",
            "--name",
            tab_name,
            "--cwd",
            &cwd_str,
        ];

        let layout_str;
        if let Some(layout_path) = layout {
            layout_str = layout_path.to_string_lossy().to_string();
            args.push("--layout");
            args.push(&layout_str);
        }

        let status = Command::new("zellij")
            .args(&args)
            .status()
            .context("Failed to create new tab")?;

        if !status.success() {
            anyhow::bail!("Failed to create tab: {}", tab_name);
        }

        Ok(())
    }

    /// ワークスペースをタブとして開く（高レベルAPI）
    pub fn open_workspace_tab(
        &self,
        tab_name: &str,
        cwd: &Path,
        layout: Option<&Path>,
    ) -> Result<TabActionResult> {
        let session = match &self.mode {
            ZellijMode::External { session_name } if !session_name.is_empty() => session_name,
            _ => anyhow::bail!("No session configured for external mode"),
        };

        // セッション存在確認
        if !self.session_exists(session)? {
            return Ok(TabActionResult::SessionNotFound(session.clone()));
        }

        // 既存タブを確認
        let tabs = self.query_tab_names(session)?;
        if tabs.iter().any(|t| t == tab_name) {
            // 既存タブに切り替え
            self.go_to_tab(session, tab_name)?;
            return Ok(TabActionResult::SwitchedToExisting(tab_name.to_string()));
        }

        // 新規タブ作成
        self.new_tab(session, tab_name, cwd, layout)?;
        Ok(TabActionResult::CreatedNew(tab_name.to_string()))
    }

    /// レイアウトファイル一覧を取得
    pub fn list_layouts(&self, layout_dir: &Path) -> Result<Vec<String>> {
        if !layout_dir.exists() {
            return Ok(Vec::new());
        }

        let mut layouts = Vec::new();
        for entry in std::fs::read_dir(layout_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().map_or(false, |ext| ext == "kdl") {
                if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                    layouts.push(name.to_string());
                }
            }
        }
        layouts.sort();
        Ok(layouts)
    }

    // ========================================
    // 従来のペイン操作（Internal mode用）
    // ========================================

    /// 指定ペインにフォーカス
    pub fn focus_pane(&self, pane_id: u32) -> Result<()> {
        if !matches!(self.mode, ZellijMode::Internal) {
            anyhow::bail!("Not running inside Zellij");
        }

        Command::new("zellij")
            .args(["action", "focus-pane", "--pane-id", &pane_id.to_string()])
            .status()
            .context("Failed to execute zellij action")?;

        Ok(())
    }

    /// 新規ペインを作成
    #[allow(dead_code)]
    pub fn new_pane(&self, cwd: &Path, command: &[&str]) -> Result<u32> {
        if !matches!(self.mode, ZellijMode::Internal) {
            anyhow::bail!("Not running inside Zellij");
        }

        let cwd_str = cwd.to_string_lossy();
        let mut args: Vec<&str> = vec!["run", "--cwd", &cwd_str, "--"];
        args.extend(command);

        let _output = Command::new("zellij")
            .args(&args)
            .output()
            .context("Failed to execute zellij run")?;

        // TODO: pane IDを取得する方法を実装
        Ok(0)
    }

    /// 指定ディレクトリでシェルを起動
    pub fn launch_shell(&self, cwd: &Path) -> Result<()> {
        if !matches!(self.mode, ZellijMode::Internal) {
            anyhow::bail!("Not running inside Zellij");
        }

        Command::new("zellij")
            .args(["run", "--cwd", &cwd.to_string_lossy(), "--", "zsh"])
            .status()
            .context("Failed to launch shell")?;

        Ok(())
    }

    /// lazygitを起動
    pub fn launch_lazygit(&self, cwd: &Path) -> Result<()> {
        if !matches!(self.mode, ZellijMode::Internal) {
            anyhow::bail!("Not running inside Zellij");
        }

        Command::new("zellij")
            .args(["run", "--cwd", &cwd.to_string_lossy(), "--", "lazygit"])
            .status()
            .context("Failed to launch lazygit")?;

        Ok(())
    }

    /// yaziを起動
    pub fn launch_yazi(&self, cwd: &Path) -> Result<()> {
        if !matches!(self.mode, ZellijMode::Internal) {
            anyhow::bail!("Not running inside Zellij");
        }

        Command::new("zellij")
            .args(["run", "--cwd", &cwd.to_string_lossy(), "--", "yazi"])
            .status()
            .context("Failed to launch yazi")?;

        Ok(())
    }

    /// 新規Claude Codeセッションを起動
    pub fn launch_claude(&self, cwd: &Path) -> Result<()> {
        if !matches!(self.mode, ZellijMode::Internal) {
            anyhow::bail!("Not running inside Zellij");
        }

        Command::new("zellij")
            .args(["run", "--cwd", &cwd.to_string_lossy(), "--", "claude"])
            .status()
            .context("Failed to launch Claude Code")?;

        Ok(())
    }

    /// ペインを閉じる
    pub fn close_pane(&self, pane_id: u32) -> Result<()> {
        if !matches!(self.mode, ZellijMode::Internal) {
            anyhow::bail!("Not running inside Zellij");
        }

        Command::new("zellij")
            .args(["action", "close-pane", "--pane-id", &pane_id.to_string()])
            .status()
            .context("Failed to close pane")?;

        Ok(())
    }
}

impl Default for ZellijActions {
    fn default() -> Self {
        Self::new()
    }
}

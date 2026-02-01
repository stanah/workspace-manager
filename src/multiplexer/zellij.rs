use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

use super::{Multiplexer, MultiplexerBackend, WindowActionResult};

/// Zellij動作モード
#[derive(Debug, Clone)]
pub enum ZellijMode {
    /// Zellij内で実行（従来の動作）
    Internal,
    /// 外部から指定セッションを操作
    External { session_name: String },
    /// 無効（マルチプレクサ非使用）
    Disabled,
}

/// Zellij操作のラッパー（Multiplexer trait 実装）
pub struct ZellijMultiplexer {
    mode: ZellijMode,
}

impl ZellijMultiplexer {
    pub fn new_internal() -> Self {
        Self {
            mode: ZellijMode::Internal,
        }
    }

    pub fn new_external(session_name: String) -> Self {
        Self {
            mode: ZellijMode::External { session_name },
        }
    }

    pub fn new_disabled() -> Self {
        Self {
            mode: ZellijMode::Disabled,
        }
    }

    pub fn auto_detect(config_session: Option<String>) -> Self {
        if std::env::var("ZELLIJ").is_ok() {
            Self::new_internal()
        } else if let Some(session) = config_session {
            Self::new_external(session)
        } else {
            Self {
                mode: ZellijMode::External {
                    session_name: String::new(),
                },
            }
        }
    }

    /// セッション存在確認
    fn session_exists(&self, name: &str) -> Result<bool> {
        let sessions = self.list_sessions()?;
        Ok(sessions.iter().any(|s| s == name))
    }

    /// 新規ペインを作成（Internal mode 用）
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
}

impl Multiplexer for ZellijMultiplexer {
    fn is_available(&self) -> bool {
        match &self.mode {
            ZellijMode::Internal => true,
            ZellijMode::External { session_name } => !session_name.is_empty(),
            ZellijMode::Disabled => false,
        }
    }

    fn is_internal(&self) -> bool {
        matches!(self.mode, ZellijMode::Internal)
    }

    fn backend(&self) -> MultiplexerBackend {
        MultiplexerBackend::Zellij
    }

    fn session_name(&self) -> Option<&str> {
        match &self.mode {
            ZellijMode::External { session_name } if !session_name.is_empty() => {
                Some(session_name)
            }
            _ => None,
        }
    }

    fn set_session_name(&mut self, name: String) {
        if let ZellijMode::External { session_name } = &mut self.mode {
            *session_name = name;
        }
    }

    fn list_sessions(&self) -> Result<Vec<String>> {
        let output = Command::new("zellij")
            .args(["list-sessions", "--no-formatting"])
            .output()
            .context("Failed to execute zellij list-sessions")?;

        if !output.status.success() {
            return Ok(Vec::new());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let sessions: Vec<String> = stdout
            .lines()
            .map(|line| {
                line.split_whitespace()
                    .next()
                    .unwrap_or(line)
                    .to_string()
            })
            .filter(|s| !s.is_empty())
            .collect();

        Ok(sessions)
    }

    fn query_window_names(&self, session: &str) -> Result<Vec<String>> {
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

    fn go_to_window(&self, session: &str, name: &str) -> Result<()> {
        let status = Command::new("zellij")
            .args(["--session", session, "action", "go-to-tab-name", name])
            .status()
            .context("Failed to switch tab")?;

        if !status.success() {
            anyhow::bail!("Failed to switch to tab: {}", name);
        }
        Ok(())
    }

    fn new_window(
        &self,
        session: &str,
        name: &str,
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
            name,
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
            anyhow::bail!("Failed to create tab: {}", name);
        }
        Ok(())
    }

    fn close_window(&self, session: &str, name: &str) -> Result<()> {
        self.go_to_window(session, name)?;
        let status = Command::new("zellij")
            .args(["--session", session, "action", "close-tab"])
            .status()
            .context("Failed to close tab")?;

        if !status.success() {
            anyhow::bail!("Failed to close tab: {}", name);
        }
        Ok(())
    }

    fn open_workspace_window(
        &self,
        name: &str,
        cwd: &Path,
        layout: Option<&Path>,
    ) -> Result<WindowActionResult> {
        let session = match &self.mode {
            ZellijMode::External { session_name } if !session_name.is_empty() => session_name,
            _ => anyhow::bail!("No session configured for external mode"),
        };

        if !self.session_exists(session)? {
            return Ok(WindowActionResult::SessionNotFound(session.clone()));
        }

        let tabs = self.query_window_names(session)?;
        if tabs.iter().any(|t| t == name) {
            self.go_to_window(session, name)?;
            return Ok(WindowActionResult::SwitchedToExisting(name.to_string()));
        }

        self.new_window(session, name, cwd, layout)?;
        Ok(WindowActionResult::CreatedNew(name.to_string()))
    }

    fn list_layouts(&self, layout_dir: &Path) -> Result<Vec<String>> {
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

    fn focus_pane(&self, pane_id: u32) -> Result<()> {
        if !matches!(self.mode, ZellijMode::Internal) {
            anyhow::bail!("Not running inside Zellij");
        }
        Command::new("zellij")
            .args(["action", "focus-pane", "--pane-id", &pane_id.to_string()])
            .status()
            .context("Failed to execute zellij action")?;
        Ok(())
    }

    fn close_pane(&self, pane_id: u32) -> Result<()> {
        if !matches!(self.mode, ZellijMode::Internal) {
            anyhow::bail!("Not running inside Zellij");
        }
        Command::new("zellij")
            .args(["action", "close-pane", "--pane-id", &pane_id.to_string()])
            .status()
            .context("Failed to close pane")?;
        Ok(())
    }

    fn launch_command(&self, cwd: &Path, command: &[&str]) -> Result<()> {
        if !matches!(self.mode, ZellijMode::Internal) {
            anyhow::bail!("Not running inside Zellij");
        }
        let cwd_str = cwd.to_string_lossy();
        let mut args: Vec<&str> = vec!["run", "--cwd", &cwd_str, "--"];
        args.extend(command);

        Command::new("zellij")
            .args(&args)
            .status()
            .context("Failed to execute zellij run")?;
        Ok(())
    }
}

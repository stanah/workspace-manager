use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

use super::{Multiplexer, MultiplexerBackend, WindowActionResult};

/// tmux動作モード
#[derive(Debug, Clone)]
pub enum TmuxMode {
    /// tmux内で実行
    Internal,
    /// 外部から指定セッションを操作
    External,
}

/// tmux操作のラッパー（Multiplexer trait 実装）
pub struct TmuxMultiplexer {
    mode: TmuxMode,
    session_name: String,
}

impl TmuxMultiplexer {
    pub fn new_internal() -> Self {
        // Internal モード: 現在のセッション名を取得してキャッシュ
        let session_name = Command::new("tmux")
            .args(["display-message", "-p", "#{session_name}"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default();

        Self {
            mode: TmuxMode::Internal,
            session_name,
        }
    }

    pub fn new_external(session_name: String) -> Self {
        Self {
            mode: TmuxMode::External,
            session_name,
        }
    }

    pub fn auto_detect(config_session: Option<String>) -> Self {
        if std::env::var("TMUX").is_ok() {
            Self::new_internal()
        } else if let Some(session) = config_session {
            Self::new_external(session)
        } else {
            Self {
                mode: TmuxMode::External,
                session_name: String::new(),
            }
        }
    }

    /// 操作対象のセッション名を解決
    fn resolve_session(&self) -> Result<String> {
        if !self.session_name.is_empty() {
            Ok(self.session_name.clone())
        } else {
            anyhow::bail!("No session configured")
        }
    }

    /// @workspace-name ユーザーオプションでウィンドウを検索し、window_index を返す
    fn find_window_by_workspace_name(&self, session: &str, name: &str) -> Result<Option<String>> {
        let output = Command::new("tmux")
            .args([
                "list-windows", "-t", session,
                "-F", "#{window_index}\t#{@workspace-name}",
            ])
            .output()
            .context("Failed to list tmux windows")?;

        if !output.status.success() {
            return Ok(None);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if let Some((idx, ws_name)) = line.split_once('\t') {
                if ws_name == name {
                    return Ok(Some(idx.to_string()));
                }
            }
        }
        Ok(None)
    }

    /// セッション存在確認
    fn session_exists(&self, name: &str) -> Result<bool> {
        let status = Command::new("tmux")
            .args(["has-session", "-t", name])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .context("Failed to check tmux session")?;

        Ok(status.success())
    }

    /// zellij 互換: lazygit 起動
    pub fn launch_lazygit(&self, cwd: &Path) -> Result<()> {
        self.launch_command(cwd, &["lazygit"])
    }

    /// zellij 互換: shell 起動
    pub fn launch_shell(&self, cwd: &Path) -> Result<()> {
        self.launch_command(cwd, &["zsh"])
    }

    /// zellij 互換: yazi 起動
    pub fn launch_yazi(&self, cwd: &Path) -> Result<()> {
        self.launch_command(cwd, &["yazi"])
    }

    /// zellij 互換: claude 起動
    pub fn launch_claude(&self, cwd: &Path) -> Result<()> {
        self.launch_command(cwd, &["claude"])
    }
}

impl Multiplexer for TmuxMultiplexer {
    fn is_available(&self) -> bool {
        match &self.mode {
            TmuxMode::Internal => true,
            TmuxMode::External => !self.session_name.is_empty(),
        }
    }

    fn is_internal(&self) -> bool {
        matches!(self.mode, TmuxMode::Internal)
    }

    fn backend(&self) -> MultiplexerBackend {
        MultiplexerBackend::Tmux
    }

    fn session_name(&self) -> Option<&str> {
        if self.session_name.is_empty() {
            None
        } else {
            Some(&self.session_name)
        }
    }

    fn set_session_name(&mut self, name: String) {
        self.session_name = name;
    }

    fn list_sessions(&self) -> Result<Vec<String>> {
        let output = Command::new("tmux")
            .args(["list-sessions", "-F", "#{session_name}"])
            .output()
            .context("Failed to execute tmux list-sessions")?;

        if !output.status.success() {
            return Ok(Vec::new());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let sessions: Vec<String> = stdout
            .lines()
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty())
            .collect();

        Ok(sessions)
    }

    fn query_window_names(&self, session: &str) -> Result<Vec<String>> {
        // @workspace-name が設定されていればそちらを優先、なければ window_name を返す
        let output = Command::new("tmux")
            .args([
                "list-windows", "-t", session,
                "-F", "#{@workspace-name}\t#{window_name}",
            ])
            .output()
            .context("Failed to list tmux windows")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to list windows: {}", stderr);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let windows: Vec<String> = stdout
            .lines()
            .filter(|s| !s.is_empty())
            .map(|line| {
                if let Some((ws_name, win_name)) = line.split_once('\t') {
                    if ws_name.is_empty() {
                        win_name.to_string()
                    } else {
                        ws_name.to_string()
                    }
                } else {
                    line.to_string()
                }
            })
            .collect();

        Ok(windows)
    }

    fn go_to_window(&self, session: &str, name: &str) -> Result<()> {
        let target = format!("{}:{}", session, name);
        let status = Command::new("tmux")
            .args(["select-window", "-t", &target])
            .status()
            .context("Failed to switch window")?;

        if !status.success() {
            anyhow::bail!("Failed to switch to window: {}", name);
        }
        Ok(())
    }

    fn new_window(
        &self,
        session: &str,
        name: &str,
        cwd: &Path,
        _layout: Option<&Path>,
    ) -> Result<()> {
        let cwd_str = cwd.to_string_lossy();
        let status = Command::new("tmux")
            .args([
                "new-window",
                "-t", session,
                "-n", name,
                "-c", &cwd_str,
            ])
            .status()
            .context("Failed to create new window")?;

        if !status.success() {
            anyhow::bail!("Failed to create window: {}", name);
        }

        // ワークスペース名をユーザーオプションに保存（ウィンドウ検索用）
        let target = format!("{}:{}", session, name);
        let _ = Command::new("tmux")
            .args(["set-window-option", "-t", &target, "@workspace-name", name])
            .status();
        // -n で設定した名前を維持するため automatic-rename を無効化
        let _ = Command::new("tmux")
            .args(["set-window-option", "-t", &target, "automatic-rename", "off"])
            .status();

        Ok(())
    }

    fn close_window(&self, session: &str, name: &str) -> Result<()> {
        // @workspace-name でウィンドウを特定してから閉じる
        let target = if let Ok(Some(idx)) = self.find_window_by_workspace_name(session, name) {
            format!("{}:{}", session, idx)
        } else {
            format!("{}:{}", session, name)
        };

        let status = Command::new("tmux")
            .args(["kill-window", "-t", &target])
            .status()
            .context("Failed to close window")?;

        if !status.success() {
            anyhow::bail!("Failed to close window: {}", name);
        }
        Ok(())
    }

    fn open_workspace_window(
        &self,
        name: &str,
        cwd: &Path,
        layout: Option<&Path>,
    ) -> Result<WindowActionResult> {
        let session = self.resolve_session()?;

        if !self.session_exists(&session)? {
            return Ok(WindowActionResult::SessionNotFound(session));
        }

        // @workspace-name でマッチするウィンドウを検索
        let existing = self.find_window_by_workspace_name(&session, name)?;
        if let Some(window_id) = existing {
            let target = format!("{}:{}", session, window_id);
            let _ = Command::new("tmux")
                .args(["select-window", "-t", &target])
                .status();
            return Ok(WindowActionResult::SwitchedToExisting(name.to_string()));
        }

        self.new_window(&session, name, cwd, layout)?;
        Ok(WindowActionResult::CreatedNew(name.to_string()))
    }

    fn list_layouts(&self, layout_dir: &Path) -> Result<Vec<String>> {
        // tmux はレイアウト概念が異なるが、互換のためファイル一覧を返す
        if !layout_dir.exists() {
            return Ok(Vec::new());
        }

        let mut layouts = Vec::new();
        for entry in std::fs::read_dir(layout_dir)? {
            let entry = entry?;
            let path = entry.path();
            // tmux layout files or kdl files
            let ext = path.extension().and_then(|e| e.to_str());
            if matches!(ext, Some("kdl") | Some("conf")) {
                if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                    layouts.push(name.to_string());
                }
            }
        }
        layouts.sort();
        Ok(layouts)
    }

    fn focus_pane(&self, pane_id: u32) -> Result<()> {
        let target = format!("%{}", pane_id);
        let status = Command::new("tmux")
            .args(["select-pane", "-t", &target])
            .status()
            .context("Failed to focus pane")?;

        if !status.success() {
            anyhow::bail!("Failed to focus pane: {}", pane_id);
        }
        Ok(())
    }

    fn close_pane(&self, pane_id: u32) -> Result<()> {
        let target = format!("%{}", pane_id);
        let status = Command::new("tmux")
            .args(["kill-pane", "-t", &target])
            .status()
            .context("Failed to close pane")?;

        if !status.success() {
            anyhow::bail!("Failed to close pane: {}", pane_id);
        }
        Ok(())
    }

    fn launch_command(&self, cwd: &Path, command: &[&str]) -> Result<()> {
        let cwd_str = cwd.to_string_lossy();
        let cmd_str = command.join(" ");
        let session = self.resolve_session()?;

        let status = Command::new("tmux")
            .args([
                "split-window",
                "-t", &session,
                "-c", &cwd_str,
                &cmd_str,
            ])
            .status()
            .context("Failed to launch command in tmux")?;

        if !status.success() {
            anyhow::bail!("Failed to launch command: {}", cmd_str);
        }
        Ok(())
    }

    fn send_keys(&self, target: &str, keys: &str) -> Result<()> {
        let status = Command::new("tmux")
            .args(["send-keys", "-t", target, keys, "Enter"])
            .status()
            .context("Failed to send keys")?;

        if !status.success() {
            anyhow::bail!("Failed to send keys to: {}", target);
        }
        Ok(())
    }

    fn capture_pane(&self, target: &str) -> Result<String> {
        let output = Command::new("tmux")
            .args(["capture-pane", "-t", target, "-p"])
            .output()
            .context("Failed to capture pane")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to capture pane: {}", stderr);
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

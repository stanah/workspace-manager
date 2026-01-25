use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

/// Zellij操作のラッパー
pub struct ZellijActions {
    /// Zellij内で実行中かどうか
    in_zellij: bool,
}

impl ZellijActions {
    /// 新規インスタンス作成
    pub fn new() -> Self {
        Self {
            in_zellij: std::env::var("ZELLIJ").is_ok(),
        }
    }

    /// Zellij内で実行中か確認
    pub fn is_available(&self) -> bool {
        self.in_zellij
    }

    /// 指定ペインにフォーカス
    pub fn focus_pane(&self, pane_id: u32) -> Result<()> {
        if !self.in_zellij {
            anyhow::bail!("Not running inside Zellij");
        }

        Command::new("zellij")
            .args(["action", "focus-pane", "--pane-id", &pane_id.to_string()])
            .status()
            .context("Failed to execute zellij action")?;

        Ok(())
    }

    /// 新規ペインを作成
    pub fn new_pane(&self, cwd: &Path, command: &[&str]) -> Result<u32> {
        if !self.in_zellij {
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
        // 現在のZellij CLIではpane IDを直接取得できないため、
        // 環境変数か別の方法で追跡する必要がある
        Ok(0)
    }

    /// 指定ディレクトリでシェルを起動
    pub fn launch_shell(&self, cwd: &Path) -> Result<()> {
        if !self.in_zellij {
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
        if !self.in_zellij {
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
        if !self.in_zellij {
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
        if !self.in_zellij {
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
        if !self.in_zellij {
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

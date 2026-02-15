use anyhow::{Context, Result};
use git2::{Repository, BranchType};
use std::path::{Path, PathBuf};
use tracing::info;

use crate::app::config::WorktreeConfig;

/// Worktree管理
pub struct WorktreeManager {
    config: WorktreeConfig,
}

impl WorktreeManager {
    pub fn new(config: WorktreeConfig) -> Self {
        Self { config }
    }

    /// 設定への参照を取得
    pub fn config(&self) -> &WorktreeConfig {
        &self.config
    }

    /// 新しいworktreeを作成
    pub fn create_worktree(
        &self,
        repo_path: &Path,
        branch_name: &str,
        create_branch: bool,
        start_point: Option<&str>,
    ) -> Result<PathBuf> {
        let repo = Repository::open(repo_path)
            .context("Failed to open repository")?;

        // リモートURLを取得
        let remote_url = repo
            .find_remote(&self.config.default_remote)
            .ok()
            .and_then(|r| r.url().map(|s| s.to_string()));

        // worktreeのパスを生成
        let worktree_path = self.config.generate_worktree_path(
            repo_path,
            branch_name,
            remote_url.as_deref(),
        );

        // パスが既に存在するかチェック
        if worktree_path.exists() {
            anyhow::bail!("Worktree path already exists: {}", worktree_path.display());
        }

        // 親ディレクトリを作成
        if let Some(parent) = worktree_path.parent() {
            std::fs::create_dir_all(parent)
                .context("Failed to create parent directory")?;
        }

        // ブランチの存在確認
        let branch_exists = repo.find_branch(branch_name, BranchType::Local).is_ok();
        let remote_branch_exists = repo
            .find_branch(&format!("{}/{}", self.config.default_remote, branch_name), BranchType::Remote)
            .is_ok();

        if create_branch && !branch_exists {
            // 新規ブランチを作成してworktreeを追加
            // git worktree add -b <branch> <path> [<start-point>]
            self.run_git_worktree_add(repo_path, &worktree_path, branch_name, true, start_point)?;
        } else if branch_exists {
            // 既存のローカルブランチでworktreeを追加
            self.run_git_worktree_add(repo_path, &worktree_path, branch_name, false, None)?;
        } else if remote_branch_exists {
            // リモートブランチを追跡するローカルブランチを作成
            self.run_git_worktree_add_tracking(repo_path, &worktree_path, branch_name)?;
        } else {
            anyhow::bail!(
                "Branch '{}' does not exist. Use create_branch=true to create it.",
                branch_name
            );
        }

        info!("Created worktree at: {}", worktree_path.display());
        Ok(worktree_path)
    }

    /// git worktree add を実行
    fn run_git_worktree_add(
        &self,
        repo_path: &Path,
        worktree_path: &Path,
        branch_name: &str,
        create_branch: bool,
        start_point: Option<&str>,
    ) -> Result<()> {
        let mut cmd = std::process::Command::new("git");
        cmd.current_dir(repo_path);
        cmd.arg("worktree").arg("add");

        if create_branch {
            // git worktree add -b <new-branch> <path> [<start-point>]
            cmd.arg("-b").arg(branch_name);
            cmd.arg(worktree_path);
            if let Some(sp) = start_point {
                cmd.arg(sp);
            }
        } else {
            // git worktree add <path> <existing-branch>
            cmd.arg(worktree_path).arg(branch_name);
        }

        let output = cmd.output().context("Failed to execute git worktree add")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("git worktree add failed: {}", stderr);
        }

        Ok(())
    }

    /// リモートブランチを追跡するworktreeを追加
    fn run_git_worktree_add_tracking(
        &self,
        repo_path: &Path,
        worktree_path: &Path,
        branch_name: &str,
    ) -> Result<()> {
        let remote_branch = format!("{}/{}", self.config.default_remote, branch_name);

        let output = std::process::Command::new("git")
            .current_dir(repo_path)
            .args(["worktree", "add", "--track", "-b", branch_name])
            .arg(worktree_path)
            .arg(&remote_branch)
            .output()
            .context("Failed to execute git worktree add")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("git worktree add failed: {}", stderr);
        }

        Ok(())
    }

    /// worktreeを削除
    pub fn remove_worktree(&self, repo_path: &Path, worktree_path: &Path, force: bool) -> Result<()> {
        let mut cmd = std::process::Command::new("git");
        cmd.current_dir(repo_path);
        cmd.arg("worktree").arg("remove");

        if force {
            cmd.arg("--force");
        }

        cmd.arg(worktree_path);

        let output = cmd.output().context("Failed to execute git worktree remove")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("git worktree remove failed: {}", stderr);
        }

        info!("Removed worktree: {}", worktree_path.display());
        Ok(())
    }

    /// リポジトリのworktree一覧を取得
    #[allow(dead_code)]
    pub fn list_worktrees(&self, repo_path: &Path) -> Result<Vec<WorktreeListInfo>> {
        let output = std::process::Command::new("git")
            .current_dir(repo_path)
            .args(["worktree", "list", "--porcelain"])
            .output()
            .context("Failed to execute git worktree list")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("git worktree list failed: {}", stderr);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut worktrees = Vec::new();
        let mut current: Option<WorktreeListInfo> = None;

        for line in stdout.lines() {
            if line.starts_with("worktree ") {
                if let Some(wt) = current.take() {
                    worktrees.push(wt);
                }
                let path = line.strip_prefix("worktree ").unwrap_or("");
                current = Some(WorktreeListInfo {
                    path: PathBuf::from(path),
                    branch: None,
                    commit: None,
                    is_bare: false,
                    is_detached: false,
                });
            } else if let Some(ref mut wt) = current {
                if line.starts_with("HEAD ") {
                    wt.commit = line.strip_prefix("HEAD ").map(|s| s.to_string());
                } else if line.starts_with("branch ") {
                    let branch = line.strip_prefix("branch refs/heads/").unwrap_or(
                        line.strip_prefix("branch ").unwrap_or("")
                    );
                    wt.branch = Some(branch.to_string());
                } else if line == "bare" {
                    wt.is_bare = true;
                } else if line == "detached" {
                    wt.is_detached = true;
                }
            }
        }

        if let Some(wt) = current {
            worktrees.push(wt);
        }

        Ok(worktrees)
    }

    /// リモートブランチ一覧を取得
    pub fn list_remote_branches(&self, repo_path: &Path) -> Result<Vec<String>> {
        let repo = Repository::open(repo_path)?;
        let mut branches = Vec::new();

        for branch in repo.branches(Some(BranchType::Remote))? {
            let (branch, _) = branch?;
            if let Some(name) = branch.name()? {
                // origin/HEAD などを除外
                if !name.ends_with("/HEAD") {
                    // origin/ プレフィックスを除去（スラッシュを含むブランチ名に対応）
                    // "origin/claude/feature" -> "claude/feature"
                    if let Some(idx) = name.find('/') {
                        let short_name = &name[idx + 1..];
                        if !short_name.is_empty() {
                            branches.push(short_name.to_string());
                        }
                    }
                }
            }
        }

        branches.sort();
        branches.dedup();
        Ok(branches)
    }

    /// ローカルブランチ一覧を取得
    pub fn list_local_branches(&self, repo_path: &Path) -> Result<Vec<String>> {
        let repo = Repository::open(repo_path)?;
        let mut branches = Vec::new();

        for branch in repo.branches(Some(BranchType::Local))? {
            let (branch, _) = branch?;
            if let Some(name) = branch.name()? {
                branches.push(name.to_string());
            }
        }

        branches.sort();
        Ok(branches)
    }
}

/// Worktree一覧情報（list_worktrees用）
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct WorktreeListInfo {
    pub path: PathBuf,
    pub branch: Option<String>,
    pub commit: Option<String>,
    pub is_bare: bool,
    pub is_detached: bool,
}

impl Default for WorktreeManager {
    fn default() -> Self {
        Self::new(WorktreeConfig::default())
    }
}

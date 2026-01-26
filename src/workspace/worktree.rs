use anyhow::Result;
use git2::Repository;
use std::path::{Path, PathBuf};
use tracing::debug;

use super::state::Workspace;

/// Git worktreeの情報
#[derive(Debug, Clone)]
pub struct WorktreeInfo {
    /// worktreeのパス
    pub path: PathBuf,
    /// リポジトリ名
    pub repo_name: String,
    /// ブランチ名
    pub branch: String,
    /// メインworktreeかどうか
    pub is_main: bool,
}

/// 指定ディレクトリからgit worktreeを検出
pub fn detect_worktrees(search_paths: &[PathBuf]) -> Vec<WorktreeInfo> {
    let mut worktrees = Vec::new();

    for path in search_paths {
        if let Ok(infos) = find_worktrees_in_path(path) {
            worktrees.extend(infos);
        }
    }

    // 重複を除去（パスベース）
    worktrees.sort_by(|a, b| a.path.cmp(&b.path));
    worktrees.dedup_by(|a, b| a.path == b.path);

    worktrees
}

/// 指定パス内のworktreeを検索
fn find_worktrees_in_path(path: &Path) -> Result<Vec<WorktreeInfo>> {
    let mut results = Vec::new();

    // パスがgitリポジトリかチェック
    if let Ok(repo) = Repository::discover(path) {
        // worktree一覧を取得
        if let Ok(worktrees) = repo.worktrees() {
            for name in worktrees.iter().flatten() {
                if let Ok(wt) = repo.find_worktree(name) {
                    if let Some(wt_path) = wt.path().parent() {
                        if let Some(info) = extract_worktree_info(wt_path) {
                            results.push(info);
                        }
                    }
                }
            }
        }

        // メインリポジトリも追加
        if let Some(workdir) = repo.workdir() {
            if let Some(info) = extract_worktree_info(workdir) {
                let mut info = info;
                info.is_main = true;
                results.push(info);
            }
        }
    }

    Ok(results)
}

/// パスからworktree情報を抽出
fn extract_worktree_info(path: &Path) -> Option<WorktreeInfo> {
    let repo = Repository::open(path).ok()?;

    // リポジトリ名を取得
    // worktreeの場合、ディレクトリ名が "repo__branch" 形式になっている可能性がある
    let dir_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");

    // "__" 区切りの場合はベースリポジトリ名を抽出
    let repo_name = if let Some(idx) = dir_name.find("__") {
        dir_name[..idx].to_string()
    } else {
        dir_name.to_string()
    };

    // ブランチ名を取得
    let branch = get_current_branch(&repo).unwrap_or_else(|| "detached".to_string());

    Some(WorktreeInfo {
        path: path.to_path_buf(),
        repo_name,
        branch,
        is_main: false,
    })
}

/// 現在のブランチ名を取得
fn get_current_branch(repo: &Repository) -> Option<String> {
    let head = repo.head().ok()?;
    if head.is_branch() {
        head.shorthand().map(|s| s.to_string())
    } else {
        // detached HEAD
        head.target()
            .map(|oid| format!("{:.7}", oid.to_string()))
    }
}

/// WorktreeInfoからWorkspaceを生成
impl From<WorktreeInfo> for Workspace {
    fn from(info: WorktreeInfo) -> Self {
        Workspace::new(
            info.path.to_string_lossy().to_string(),
            info.repo_name,
            info.branch,
        )
    }
}

/// ホームディレクトリ配下の一般的な開発ディレクトリを検索
pub fn get_default_search_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        // よく使われる開発ディレクトリ
        let common_dirs = ["work", "projects", "src", "dev", "code", "repos"];
        for dir in common_dirs {
            let path = home.join(dir);
            if path.exists() && path.is_dir() {
                paths.push(path);
            }
        }
    }

    paths
}

/// ディレクトリを再帰的に走査してgitリポジトリを検出
pub fn scan_for_repositories(base_path: &Path, max_depth: usize) -> Vec<WorktreeInfo> {
    let mut results = Vec::new();
    scan_recursive(base_path, max_depth, 0, &mut results);
    results
}

fn scan_recursive(path: &Path, max_depth: usize, current_depth: usize, results: &mut Vec<WorktreeInfo>) {
    if current_depth > max_depth {
        return;
    }

    // .gitディレクトリがあればリポジトリ
    let git_dir = path.join(".git");
    if git_dir.exists() {
        if let Some(info) = extract_worktree_info(path) {
            debug!("Found repository: {:?}", path);
            results.push(info);
        }
        // worktreeも検出
        if let Ok(repo) = Repository::open(path) {
            if let Ok(worktrees) = repo.worktrees() {
                for name in worktrees.iter().flatten() {
                    if let Ok(wt) = repo.find_worktree(name) {
                        if let Some(wt_path) = wt.path().parent() {
                            if let Some(info) = extract_worktree_info(wt_path) {
                                results.push(info);
                            }
                        }
                    }
                }
            }
        }
        return; // リポジトリ内は再帰しない
    }

    // サブディレクトリを走査
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.filter_map(|e| e.ok()) {
            let entry_path = entry.path();
            if entry_path.is_dir() {
                // 隠しディレクトリとnode_modulesはスキップ
                let name = entry_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if !name.starts_with('.') && name != "node_modules" && name != "target" {
                    scan_recursive(&entry_path, max_depth, current_depth + 1, results);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_default_search_paths() {
        let paths = get_default_search_paths();
        // HOMEが設定されていればパスが返される（空でもOK）
        assert!(paths.iter().all(|p| p.exists()));
    }
}

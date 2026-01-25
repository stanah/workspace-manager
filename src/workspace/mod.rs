pub mod manager;
pub mod state;
pub mod worktree;

pub use manager::WorktreeManager;
pub use state::{Workspace, WorkspaceStatus};
pub use worktree::{detect_worktrees, get_default_search_paths, scan_for_repositories, WorktreeInfo};

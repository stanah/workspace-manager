pub mod manager;
pub mod session;
pub mod state;
pub mod worktree;

pub use manager::WorktreeManager;
pub use session::{
    AiTool, Session, SessionId, SessionStatus, claude_external_id, kiro_external_id,
    parse_external_id,
};
pub use state::Workspace;
pub use worktree::{detect_worktrees, get_default_search_paths, scan_for_repositories, WorktreeInfo};

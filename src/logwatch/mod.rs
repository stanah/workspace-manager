//! Log-based CLI status tracking via AI analysis
//!
//! This module provides functionality to monitor CLI tool logs (Claude Code, Kiro-CLI)
//! and analyze them using AI to extract structured status information.
//!
//! ## Architecture
//!
//! - **Claude Code**: Uses sessions-index.json polling to read session status
//! - **Kiro CLI**: Uses SQLite polling to read status from database

pub mod analyzer;
pub mod claude_sessions;
pub mod collector;
pub mod kiro_sqlite;
pub mod schema;

pub use analyzer::LogAnalyzer;
pub use claude_sessions::{ClaudeProcessInfo, ClaudeSession, ClaudeSessionsConfig, ClaudeSessionsFetcher};
pub use collector::LogCollector;
pub use kiro_sqlite::{KiroSqliteConfig, KiroSqliteFetcher, KiroStatus};
pub use schema::{AnalysisProgress, SessionStatus, StatusDetail, StatusState};

//! Log-based CLI status tracking via AI analysis
//!
//! This module provides functionality to monitor CLI tool logs (Claude Code, Kiro-CLI)
//! and analyze them using AI to extract structured status information.
//!
//! ## Architecture
//!
//! - **Claude Code**: Uses hooks for event-driven status updates (no AI analysis, no polling)
//! - **Kiro CLI**: Uses SQLite polling to read status from database (no hooks needed)

pub mod analyzer;
pub mod collector;
pub mod kiro_sqlite;
pub mod schema;

pub use analyzer::LogAnalyzer;
pub use collector::LogCollector;
pub use kiro_sqlite::{KiroSqliteConfig, KiroSqliteFetcher, KiroStatus};
pub use schema::{AnalysisProgress, SessionStatus, StatusDetail, StatusState};

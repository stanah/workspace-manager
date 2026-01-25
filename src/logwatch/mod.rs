//! Log-based CLI status tracking via AI analysis
//!
//! This module provides functionality to monitor CLI tool logs (Claude Code, Kiro-CLI)
//! and analyze them using AI to extract structured status information.

pub mod analyzer;
pub mod collector;
pub mod schema;

pub use analyzer::LogAnalyzer;
pub use collector::LogCollector;
pub use schema::{AnalysisProgress, SessionStatus, StatusDetail, StatusState};

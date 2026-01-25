//! Notification module for receiving status updates from AI CLI tools
//!
//! This module provides Unix Domain Socket based communication between
//! AI CLI tools (Claude Code, Kiro CLI, OpenCode, Codex) and the workspace-manager TUI.

pub mod client;
pub mod protocol;
pub mod server;

pub use client::send_notification;
pub use protocol::NotifyMessage;
pub use server::run_listener;

/// Default socket path for the notification server
pub fn socket_path() -> std::path::PathBuf {
    directories::ProjectDirs::from("", "", "workspace-manager")
        .map(|d| d.runtime_dir().unwrap_or(d.data_dir()).to_path_buf())
        .unwrap_or_else(|| std::env::temp_dir().join("workspace-manager"))
        .join("notify.sock")
}

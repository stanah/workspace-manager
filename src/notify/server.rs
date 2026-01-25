//! Unix Domain Socket server for receiving notifications

use anyhow::{Context, Result};
use std::path::Path;
use tokio::io::AsyncReadExt;
use tokio::net::UnixListener;
use tokio::sync::mpsc;

use super::protocol::NotifyMessage;
use crate::app::AppEvent;
use crate::workspace::WorkspaceStatus;

/// Run the notification listener
///
/// This function spawns a background task that listens for incoming connections
/// on a Unix domain socket and converts received messages to AppEvents.
///
/// # Arguments
/// * `socket_path` - Path to the Unix domain socket
/// * `tx` - Channel sender for AppEvents
///
/// # Returns
/// A Result indicating whether the listener was started successfully
pub async fn run_listener(socket_path: &Path, tx: mpsc::Sender<AppEvent>) -> Result<()> {
    // Remove existing socket if present
    if socket_path.exists() {
        std::fs::remove_file(socket_path).context("Failed to remove existing socket")?;
    }

    // Ensure parent directory exists
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent).context("Failed to create socket directory")?;
    }

    let listener = UnixListener::bind(socket_path)
        .with_context(|| format!("Failed to bind to socket: {}", socket_path.display()))?;

    tracing::info!("Notification listener started at: {}", socket_path.display());

    loop {
        match listener.accept().await {
            Ok((stream, _addr)) => {
                let tx = tx.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(stream, tx).await {
                        tracing::warn!("Error handling connection: {}", e);
                    }
                });
            }
            Err(e) => {
                tracing::warn!("Failed to accept connection: {}", e);
            }
        }
    }
}

async fn handle_connection(
    mut stream: tokio::net::UnixStream,
    tx: mpsc::Sender<AppEvent>,
) -> Result<()> {
    // Read length-prefixed message
    let mut len_buf = [0u8; 4];
    stream
        .read_exact(&mut len_buf)
        .await
        .context("Failed to read message length")?;

    let len = u32::from_be_bytes(len_buf) as usize;
    if len > 1024 * 1024 {
        anyhow::bail!("Message too large: {} bytes", len);
    }

    let mut buf = vec![0u8; len];
    stream
        .read_exact(&mut buf)
        .await
        .context("Failed to read message body")?;

    let message: NotifyMessage =
        serde_json::from_slice(&buf).context("Failed to parse message")?;

    let event = message_to_event(message);
    tx.send(event)
        .await
        .context("Failed to send event to main loop")?;

    Ok(())
}

fn message_to_event(message: NotifyMessage) -> AppEvent {
    match message {
        NotifyMessage::Register {
            session_id,
            project_path,
            tool: _,
        } => AppEvent::WorkspaceRegister {
            session_id,
            project_path,
            pane_id: None,
        },
        NotifyMessage::Status {
            session_id,
            status,
            message,
        } => AppEvent::WorkspaceUpdate {
            session_id,
            status: WorkspaceStatus::from_str(&status),
            message,
        },
        NotifyMessage::Unregister { session_id } => AppEvent::WorkspaceUnregister { session_id },
    }
}

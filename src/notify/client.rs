//! Client for sending notifications to the workspace-manager TUI

use anyhow::{Context, Result};
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

use super::protocol::NotifyMessage;

/// Send a notification message to the workspace-manager TUI
///
/// # Arguments
/// * `socket_path` - Path to the Unix domain socket
/// * `message` - The notification message to send
///
/// # Returns
/// Ok(()) if the message was sent successfully, or an error if the connection failed
pub fn send_notification(socket_path: &Path, message: &NotifyMessage) -> Result<()> {
    // Connect with timeout
    let stream = UnixStream::connect(socket_path)
        .with_context(|| format!("Failed to connect to socket: {}", socket_path.display()))?;

    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .context("Failed to set write timeout")?;

    send_message(&stream, message)
}

fn send_message(mut stream: &UnixStream, message: &NotifyMessage) -> Result<()> {
    let json = serde_json::to_string(message).context("Failed to serialize message")?;

    // Write length-prefixed message
    let len = json.len() as u32;
    stream
        .write_all(&len.to_be_bytes())
        .context("Failed to write message length")?;
    stream
        .write_all(json.as_bytes())
        .context("Failed to write message")?;
    stream.flush().context("Failed to flush stream")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;
    use tempfile::tempdir;

    #[test]
    fn test_send_notification() {
        let dir = tempdir().unwrap();
        let socket_path = dir.path().join("test.sock");

        // Start a listener in a thread
        let listener = UnixListener::bind(&socket_path).unwrap();
        let socket_path_clone = socket_path.clone();

        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut len_buf = [0u8; 4];
            std::io::Read::read_exact(&mut stream, &mut len_buf).unwrap();
            let len = u32::from_be_bytes(len_buf) as usize;
            let mut buf = vec![0u8; len];
            std::io::Read::read_exact(&mut stream, &mut buf).unwrap();
            String::from_utf8(buf).unwrap()
        });

        // Send a message
        let msg = NotifyMessage::Status {
            session_id: "test".to_string(),
            status: "working".to_string(),
            message: None,
        };
        send_notification(&socket_path_clone, &msg).unwrap();

        let received = handle.join().unwrap();
        assert!(received.contains("\"status\":\"working\""));
    }
}

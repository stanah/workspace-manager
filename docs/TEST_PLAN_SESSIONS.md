# Session Management Test Plan

## Overview

This document outlines tests needed for the multi-session support feature.
The recent bug (sessions not being removed from display) highlighted gaps in test coverage.

## Unit Tests

### 1. `Session` struct (`src/workspace/session.rs`)

```rust
#[test]
fn test_session_disconnect() {
    let mut session = Session::new("ext_id".into(), 0, AiTool::Kiro);
    assert!(session.is_active());

    session.disconnect();

    assert!(!session.is_active());
    assert_eq!(session.status, SessionStatus::Disconnected);
}
```

### 2. `AppState` session management (`src/app/state.rs`)

```rust
#[test]
fn test_register_session() {
    let mut state = AppState::new();
    // Add a workspace first
    state.workspaces.push(Workspace::new("/test/path"));

    let idx = state.register_session("kiro:/test/path:123".into(), "/test/path", AiTool::Kiro, None);

    assert!(idx.is_some());
    assert_eq!(state.sessions.len(), 1);
}

#[test]
fn test_remove_session_marks_disconnected() {
    let mut state = AppState::new();
    state.workspaces.push(Workspace::new("/test/path"));
    state.register_session("kiro:/test/path:123".into(), "/test/path", AiTool::Kiro, None);

    state.remove_session("kiro:/test/path:123");

    let session = state.get_session_by_external_id("kiro:/test/path:123");
    assert!(session.is_some());
    assert!(!session.unwrap().is_active());
}

#[test]
fn test_sessions_for_workspace_excludes_disconnected() {
    let mut state = AppState::new();
    state.workspaces.push(Workspace::new("/test/path"));

    // Add 2 sessions
    state.register_session("kiro:/test/path:111".into(), "/test/path", AiTool::Kiro, None);
    state.register_session("kiro:/test/path:222".into(), "/test/path", AiTool::Kiro, None);

    assert_eq!(state.sessions_for_workspace(0).len(), 2);

    // Disconnect one
    state.remove_session("kiro:/test/path:111");

    assert_eq!(state.sessions_for_workspace(0).len(), 1);
}

#[test]
fn test_duplicate_session_not_registered() {
    let mut state = AppState::new();
    state.workspaces.push(Workspace::new("/test/path"));

    state.register_session("kiro:/test/path:123".into(), "/test/path", AiTool::Kiro, None);
    state.register_session("kiro:/test/path:123".into(), "/test/path", AiTool::Kiro, None);

    assert_eq!(state.sessions.len(), 1);
}
```

### 3. `KiroSqliteFetcher` (`src/logwatch/kiro_sqlite.rs`)

```rust
#[test]
fn test_external_id_format() {
    let status = KiroStatus {
        conversation_id: "abc-123".into(),
        // ...
    };

    assert_eq!(
        status.external_id("/path/to/project"),
        "kiro:/path/to/project:abc-123"
    );
}

#[test]
fn test_get_statuses_respects_process_count() {
    // Requires test database setup
    // Verify LIMIT is applied correctly
}
```

### 4. `ClaudeSessionsFetcher` (`src/logwatch/claude_sessions.rs`)

```rust
#[test]
fn test_claude_external_id_format() {
    assert_eq!(
        claude_external_id("session-uuid-123"),
        "claude:session-uuid-123"
    );
}

#[test]
fn test_process_detection_filters_background() {
    // Verify TTY filter excludes tty=?? processes
}
```

## Integration Tests

### 1. Session lifecycle

```rust
#[tokio::test]
async fn test_session_register_update_unregister_cycle() {
    let (tx, mut rx) = tokio::sync::mpsc::channel(10);
    let mut state = AppState::new();

    // Simulate Register event
    // Verify session added

    // Simulate Update event
    // Verify status changed

    // Simulate Unregister event
    // Verify session marked disconnected
    // Verify tree rebuilt
}
```

### 2. Tree rebuild on session changes

```rust
#[test]
fn test_tree_includes_active_sessions_only() {
    let mut state = AppState::new();
    // Setup workspace and sessions

    state.rebuild_tree();
    let session_items: Vec<_> = state.tree_items.iter()
        .filter(|item| matches!(item, TreeItem::Session { .. }))
        .collect();

    // Verify only active sessions in tree
}
```

## Manual Test Scenarios

### Scenario 1: Basic session display
1. Start TUI with no AI processes
2. Start 1 Kiro process
3. Verify 1 Kiro session appears after ~10s
4. Stop Kiro process
5. Verify session disappears after ~10s

### Scenario 2: Multiple sessions
1. Start TUI
2. Start 2 Kiro processes in same workspace
3. Send message in both
4. Verify 2 sessions displayed
5. Close 1 Kiro
6. Verify 1 session remains

### Scenario 3: Mixed tools
1. Start TUI
2. Start Claude Code and Kiro in same workspace
3. Verify both sessions displayed with correct icons [C] [K]
4. Close both
5. Verify both disappear

### Scenario 4: Session before message
1. Start TUI with 1 Kiro running
2. Start 2nd Kiro (don't send message)
3. Verify display doesn't show wrong old sessions
4. Send message in 2nd Kiro
5. Verify correct 2 sessions displayed

## Priority

1. **High**: Unit tests for `sessions_for_workspace` filtering
2. **High**: Unit tests for `remove_session` behavior
3. **Medium**: Integration test for session lifecycle
4. **Low**: Process detection tests (environment-dependent)

## Notes

- Tests should not depend on actual Claude/Kiro processes
- Use mock data for database queries
- Consider using `tempfile` for test databases

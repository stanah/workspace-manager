use anyhow::Result;
use clap::{Parser, Subcommand};
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture, KeyCode, KeyEvent},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::path::Path;
use std::time::{Duration, Instant};
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use workspace_manager::app::{Action, AppEvent, AppState, Config, mouse_action, poll_event, ViewMode};
use workspace_manager::logwatch::{ClaudeSessionsFetcher, KiroSqliteConfig, KiroSqliteFetcher};
use workspace_manager::notify::{self, NotifyMessage};
use workspace_manager::ui;
use workspace_manager::ui::input_dialog::{InputDialog, InputDialogKind};
use workspace_manager::ui::selection_dialog::{SelectionContext, SelectionDialogKind};
use workspace_manager::workspace::{AiTool, WorktreeManager};
use workspace_manager::zellij::{TabActionResult, ZellijActions};

/// Workspace Manager - TUI for managing Claude Code workspaces
#[derive(Parser)]
#[command(name = "workspace-manager")]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Log level (trace, debug, info, warn, error)
    #[arg(short, long, default_value = "info")]
    log_level: String,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the TUI (default)
    Tui,
    /// Start the MCP daemon server (Phase 2)
    Daemon,
    /// Send a notification to the daemon (Phase 2)
    Notify {
        #[command(subcommand)]
        action: NotifyAction,
    },
}

#[derive(Subcommand)]
enum NotifyAction {
    /// Register a new workspace session
    Register {
        /// Session ID (defaults to CLAUDE_SESSION_ID env var)
        #[arg(long, env = "CLAUDE_SESSION_ID")]
        session_id: String,
        /// Project path (defaults to current directory)
        #[arg(long, default_value = ".")]
        project_path: String,
        /// AI CLI tool name (claude, kiro, opencode, codex)
        #[arg(long)]
        tool: Option<String>,
    },
    /// Update workspace status
    Status {
        /// Session ID (defaults to CLAUDE_SESSION_ID env var)
        #[arg(long, env = "CLAUDE_SESSION_ID")]
        session_id: String,
        /// New status (working, idle)
        status: String,
        /// Optional status message
        #[arg(short, long)]
        message: Option<String>,
    },
    /// Unregister a workspace session
    Unregister {
        /// Session ID (defaults to CLAUDE_SESSION_ID env var)
        #[arg(long, env = "CLAUDE_SESSION_ID")]
        session_id: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // ログ初期化
    init_logging(&cli.log_level)?;

    match cli.command {
        Some(Commands::Daemon) => {
            info!("Daemon mode not yet implemented (Phase 2)");
            eprintln!("Daemon mode will be available in Phase 2");
            Ok(())
        }
        Some(Commands::Notify { action }) => handle_notify(action),
        Some(Commands::Tui) | None => run_tui(),
    }
}

fn handle_notify(action: NotifyAction) -> Result<()> {
    let socket_path = notify::socket_path();

    // Canonicalize project_path for register action
    let message = match action {
        NotifyAction::Register {
            session_id,
            project_path,
            tool,
        } => {
            let project_path = std::fs::canonicalize(&project_path)
                .unwrap_or_else(|_| std::path::PathBuf::from(&project_path))
                .to_string_lossy()
                .to_string();
            NotifyMessage::Register {
                session_id,
                project_path,
                tool,
            }
        }
        NotifyAction::Status {
            session_id,
            status,
            message,
        } => NotifyMessage::Status {
            session_id,
            status,
            message,
        },
        NotifyAction::Unregister { session_id } => NotifyMessage::Unregister { session_id },
    };

    match notify::send_notification(&socket_path, &message) {
        Ok(()) => {
            info!("Notification sent successfully");
            Ok(())
        }
        Err(e) => {
            // If socket doesn't exist, TUI is not running - silently succeed
            if socket_path.exists() {
                eprintln!("Warning: Failed to send notification: {}", e);
            }
            Ok(())
        }
    }
}

fn init_logging(level: &str) -> Result<()> {
    let log_dir = directories::ProjectDirs::from("", "", "workspace-manager")
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| std::env::temp_dir().join("workspace-manager"));

    std::fs::create_dir_all(&log_dir)?;
    let log_file = std::fs::File::create(log_dir.join("workspace-manager.log"))?;

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(level));

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().with_writer(log_file))
        .init();

    info!("Workspace Manager starting");
    Ok(())
}

fn run_tui() -> Result<()> {
    // 設定を先に読み込む（ファイルがなければ作成）
    let mut config = Config::load().unwrap_or_default();

    // 組み込みレイアウトを生成
    if let Err(e) = config.zellij.generate_builtin_layouts() {
        tracing::warn!("Failed to generate layouts: {}", e);
    }

    // Create tokio runtime for async operations
    let runtime = tokio::runtime::Runtime::new()?;

    // Create channel for notify events
    let (notify_tx, notify_rx) = tokio::sync::mpsc::channel::<AppEvent>(100);

    // Start the notification listener in background
    let socket_path = notify::socket_path();
    let notify_tx_clone = notify_tx.clone();
    runtime.spawn(async move {
        if let Err(e) = notify::run_listener(&socket_path, notify_tx_clone).await {
            tracing::error!("Notification listener error: {}", e);
        }
    });

    // Start log watcher if enabled (event-driven for Claude Code, polling for Kiro CLI)
    // Create watch channel to share workspace list with logwatch service
    let (workspace_watch_tx, workspace_watch_rx) = tokio::sync::watch::channel::<Vec<String>>(Vec::new());
    let logwatch_trigger: Option<LogWatchTrigger> = if config.logwatch.enabled {
        let (trigger_tx, trigger_rx) = tokio::sync::mpsc::channel::<String>(100);
        let logwatch_tx = notify_tx.clone();
        let logwatch_config = config.logwatch.clone();
        runtime.spawn(async move {
            run_logwatch(logwatch_config, logwatch_tx, trigger_rx, workspace_watch_rx).await;
        });
        Some(trigger_tx)
    } else {
        None
    };
    // Keep workspace_watch_tx for updating workspace list
    let workspace_watch_tx = if config.logwatch.enabled { Some(workspace_watch_tx) } else { None };

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let mut state = AppState::new();
    let mut zellij = ZellijActions::auto_detect(config.zellij.session_name.clone());
    let worktree_manager = WorktreeManager::new(config.worktree.clone());

    state.scan_workspaces();
    state.rebuild_tree_with_manager(Some(&worktree_manager));

    let result = run_app(&mut terminal, &mut state, &mut zellij, &mut config, &worktree_manager, notify_rx, logwatch_trigger, workspace_watch_tx, &runtime);

    // Clean up socket on exit
    let socket_path = notify::socket_path();
    let _ = std::fs::remove_file(&socket_path);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

/// Simple Claude Code status service (hooks-based, no AI analysis)
/// Channel for triggering log analysis (used for shutdown signaling)
type LogWatchTrigger = tokio::sync::mpsc::Sender<String>;

/// Normalize path for comparison (expand ~ and resolve)
fn normalize_path_for_comparison(path: &str) -> String {
    path.replace("~", &std::env::var("HOME").unwrap_or_default())
}

/// Run log watcher service with new architecture:
/// - Claude Code: sessions-index.json polling
/// - Kiro CLI: SQLite polling (reads status from database)
async fn run_logwatch(
    config: workspace_manager::app::LogWatchConfig,
    tx: tokio::sync::mpsc::Sender<AppEvent>,
    mut trigger_rx: tokio::sync::mpsc::Receiver<String>,
    workspace_rx: tokio::sync::watch::Receiver<Vec<String>>,
) {
    tracing::info!(
        "Log watch service started (Claude polling: {}, Kiro polling: {})",
        config.claude_hooks_enabled,  // Reusing this config flag for Claude polling
        config.kiro_polling_enabled
    );

    // Claude Code: sessions-index.json polling task
    let claude_polling_handle = if config.claude_hooks_enabled {
        let claude_fetcher = ClaudeSessionsFetcher::new();
        let poll_interval = Duration::from_secs(config.kiro_polling_interval_secs); // Use same interval
        let poll_tx = tx.clone();
        let mut poll_workspace_rx = workspace_rx.clone();

        Some(tokio::spawn(async move {
            if !claude_fetcher.is_available() {
                tracing::info!("Claude projects directory not found, polling disabled");
                return;
            }

            tracing::info!(
                "Claude sessions-index polling started (interval: {}s, dir: {:?})",
                poll_interval.as_secs(),
                claude_fetcher.claude_dir()
            );

            // Track previously active sessions to detect disconnections
            let mut prev_active_sessions: std::collections::HashSet<String> = std::collections::HashSet::new();

            loop {
                tokio::time::sleep(poll_interval).await;

                // Get current workspace list
                let workspaces = poll_workspace_rx.borrow_and_update().clone();

                if workspaces.is_empty() {
                    continue;
                }

                // Get running Claude processes with their session IDs
                let running_processes = claude_fetcher.get_running_processes();

                // Fetch sessions from Claude sessions-index.json
                let sessions_by_path = claude_fetcher.get_sessions(&workspaces);
                let mut current_active_sessions: std::collections::HashSet<String> = std::collections::HashSet::new();

                for (path, sessions) in &sessions_by_path {
                    let normalized_path = normalize_path_for_comparison(path);

                    // Get running session IDs for this workspace
                    let running_session_ids: Vec<&str> = running_processes.iter()
                        .filter(|p| p.cwd == normalized_path)
                        .filter_map(|p| p.session_id.as_deref())
                        .collect();

                    // Also get process count (some may not have --resume)
                    let total_process_count = running_processes.iter()
                        .filter(|p| p.cwd == normalized_path)
                        .count();

                    if total_process_count == 0 {
                        continue;
                    }

                    // Match sessions: prefer exact session ID match, fallback to newest
                    let mut matched_count = 0;
                    for session in sessions {
                        // Check if this session's ID matches a running process
                        let is_running = running_session_ids.iter()
                            .any(|&sid| session.session_id == sid);

                        // Or if we haven't matched enough sessions yet (fallback for new sessions without --resume)
                        let should_include = is_running ||
                            (matched_count < total_process_count && running_session_ids.len() < total_process_count);

                        if should_include && matched_count < total_process_count {
                            matched_count += 1;
                            current_active_sessions.insert(session.external_id.clone());
                            let session_status = session.to_session_status();
                            let event = AppEvent::SessionStatusAnalyzed {
                                external_id: session.external_id.clone(),
                                project_path: path.clone(),
                                status: session_status,
                            };
                            if poll_tx.send(event).await.is_err() {
                                tracing::warn!("Claude poll receiver dropped");
                                return;
                            }
                        }
                    }
                }

                // Remove sessions that are no longer active (immediate removal)
                for external_id in prev_active_sessions.difference(&current_active_sessions) {
                    tracing::debug!("Claude session removed: {}", external_id);
                    let event = AppEvent::SessionUnregister {
                        external_id: external_id.clone(),
                    };
                    if poll_tx.send(event).await.is_err() {
                        tracing::warn!("Claude poll receiver dropped");
                        return;
                    }
                }

                prev_active_sessions = current_active_sessions;
            }
        }))
    } else {
        None
    };

    // Kiro CLI: SQLite polling task
    let kiro_polling_handle = if config.kiro_polling_enabled {
        let kiro_config = KiroSqliteConfig {
            db_path: config.kiro_db_path.clone(),
            timeout_secs: 5,
        };
        let kiro_fetcher = KiroSqliteFetcher::with_config(kiro_config);
        let poll_interval = Duration::from_secs(config.kiro_polling_interval_secs);
        let poll_tx = tx.clone();
        let mut poll_workspace_rx = workspace_rx.clone();

        Some(tokio::spawn(async move {
            if !kiro_fetcher.is_available() {
                tracing::info!("Kiro database not found at {:?}, polling disabled", kiro_fetcher.db_path());
                return;
            }

            tracing::info!(
                "Kiro SQLite polling started (interval: {}s, db: {:?})",
                poll_interval.as_secs(),
                kiro_fetcher.db_path()
            );

            // Track active sessions to detect disconnections
            let mut prev_active_sessions: std::collections::HashSet<String> = std::collections::HashSet::new();

            loop {
                tokio::time::sleep(poll_interval).await;

                // Get current workspace list
                let workspaces = poll_workspace_rx.borrow_and_update().clone();

                if workspaces.is_empty() {
                    continue;
                }

                // Fetch sessions from Kiro SQLite (already limited to process_count per workspace)
                let mut current_active_sessions: std::collections::HashSet<String> = std::collections::HashSet::new();

                for (path, status) in kiro_fetcher.get_statuses(&workspaces) {
                    let external_id = status.external_id(&path);
                    current_active_sessions.insert(external_id.clone());

                    let session_status = status.to_session_status(&path);
                    let event = AppEvent::SessionStatusAnalyzed {
                        external_id,
                        project_path: path,
                        status: session_status,
                    };
                    if poll_tx.send(event).await.is_err() {
                        tracing::warn!("Kiro poll receiver dropped");
                        return;
                    }
                }

                // Remove sessions that are no longer active (Kiro: immediate removal)
                for external_id in prev_active_sessions.difference(&current_active_sessions) {
                    let event = AppEvent::SessionUnregister {
                        external_id: external_id.clone(),
                    };
                    if poll_tx.send(event).await.is_err() {
                        tracing::warn!("Kiro poll receiver dropped");
                        return;
                    }
                }

                prev_active_sessions = current_active_sessions;
            }
        }))
    } else {
        None
    };

    // Wait for shutdown signal (trigger_rx closing)
    while trigger_rx.recv().await.is_some() {
        // Ignore triggers - we use polling now
    }

    // Cleanup
    if let Some(handle) = claude_polling_handle {
        handle.abort();
    }
    if let Some(handle) = kiro_polling_handle {
        handle.abort();
    }
    tracing::info!("Log watch service stopped");
}

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut AppState,
    zellij: &mut ZellijActions,
    config: &mut Config,
    worktree_manager: &WorktreeManager,
    mut notify_rx: tokio::sync::mpsc::Receiver<AppEvent>,
    logwatch_trigger: Option<LogWatchTrigger>,
    workspace_watch_tx: Option<tokio::sync::watch::Sender<Vec<String>>>,
    runtime: &tokio::runtime::Runtime,
) -> Result<()> {
    // 起動直後に即座にポーリングするため10から開始
    let mut tick_count = 10u8;
    // ダブルクリック検出用の前回クリック情報
    let mut last_click: Option<(Instant, u16)> = None;
    const DOUBLE_CLICK_THRESHOLD: Duration = Duration::from_millis(300);

    // Send initial workspace list to logwatch service
    if let Some(ref tx) = workspace_watch_tx {
        let paths: Vec<String> = state.workspaces.iter().map(|w| w.project_path.clone()).collect();
        let _ = tx.send(paths);
    }

    loop {
        // Check for notify events (non-blocking)
        while let Ok(event) = notify_rx.try_recv() {
            // Trigger log analysis for relevant events
            if let Some(ref trigger) = logwatch_trigger {
                let path_to_analyze: Option<String> = match &event {
                    AppEvent::SessionRegister { project_path, .. } => {
                        Some(project_path.clone())
                    }
                    AppEvent::SessionUpdate { external_id, .. } => {
                        // Find project path from external_id
                        state.get_session_by_external_id(external_id)
                            .and_then(|s| state.workspaces.get(s.workspace_index))
                            .map(|w| w.project_path.clone())
                    }
                    _ => None,
                };

                if let Some(path) = path_to_analyze {
                    let trigger_clone = trigger.clone();
                    runtime.spawn(async move {
                        // Small delay to let log files be written
                        tokio::time::sleep(Duration::from_millis(500)).await;
                        let _ = trigger_clone.send(path).await;
                    });
                }
            }
            handle_notify_event(state, event);
        }

        // 1秒ごとにZellijタブ状態とワークスペースリストを更新（100ms × 10回 = 1秒）
        if tick_count >= 10 {
            tick_count = 0;
            if let Some(session) = zellij.session_name() {
                match zellij.query_tab_names(session) {
                    Ok(tabs) => {
                        tracing::debug!("Open tabs: {:?}", tabs);
                        state.update_open_tabs(tabs);
                    }
                    Err(e) => {
                        tracing::debug!("Failed to query tabs: {}", e);
                    }
                }
            } else {
                tracing::debug!("No session name configured");
            }

            // Update workspace list for Kiro SQLite polling
            if let Some(ref tx) = workspace_watch_tx {
                let paths: Vec<String> = state.workspaces.iter().map(|w| w.project_path.clone()).collect();
                let _ = tx.send(paths);
            }
        }
        tick_count += 1;

        terminal.draw(|frame| {
            ui::render(frame, state);
        })?;

        if let Some(event) = poll_event(Duration::from_millis(100))? {
            match state.view_mode {
                ViewMode::Input => {
                    if let AppEvent::Key(key) = event {
                        handle_input_event(state, key, worktree_manager)?;
                    }
                }
                ViewMode::Selection => {
                    if let AppEvent::Key(key) = event {
                        handle_selection_event(state, key, zellij, config)?;
                    }
                }
                _ => match event {
                    AppEvent::Key(key) => {
                        let action = Action::from(key);
                        handle_action(state, zellij, config, worktree_manager, action)?;
                    }
                    AppEvent::Mouse(mouse) => {
                        // header_height = 1 (border only, no header row in Table)
                        let action = mouse_action(mouse, 0, 1);
                        // ダブルクリック検出
                        let action = match action {
                            Action::MouseSelect(row) => {
                                let now = Instant::now();
                                let is_double_click = last_click
                                    .map(|(time, prev_row)| {
                                        now.duration_since(time) < DOUBLE_CLICK_THRESHOLD
                                            && prev_row == row
                                    })
                                    .unwrap_or(false);
                                last_click = Some((now, row));
                                if is_double_click {
                                    Action::MouseDoubleClick(row)
                                } else {
                                    Action::MouseSelect(row)
                                }
                            }
                            other => other,
                        };
                        handle_action(state, zellij, config, worktree_manager, action)?;
                    }
                    AppEvent::Resize(_, _) => {}
                    _ => {}
                },
            }
        }

        if state.should_quit {
            break;
        }
    }

    Ok(())
}

/// 入力モードでのキーイベント処理
fn handle_input_event(
    state: &mut AppState,
    key: KeyEvent,
    worktree_manager: &WorktreeManager,
) -> Result<()> {
    // 先に必要な情報を取得
    let repo_path = state.selected_repo_path();
    let dialog_kind = state.input_dialog.as_ref().map(|d| d.kind.clone());
    let dialog_input = state.input_dialog.as_ref().map(|d| d.input.clone());

    match key.code {
        KeyCode::Esc => {
            // FilterBranchesの場合はフィルターをクリア
            if matches!(dialog_kind, Some(InputDialogKind::FilterBranches)) {
                state.branch_filter = None;
                state.close_input_dialog();
                state.rebuild_tree_with_manager(Some(worktree_manager));
                state.status_message = Some("Filter cleared".to_string());
            } else {
                state.close_input_dialog();
            }
        }
        KeyCode::Enter => {
            match dialog_kind {
                Some(InputDialogKind::CreateWorktree) => {
                    let branch_name = dialog_input.unwrap_or_default().trim().to_string();
                    if branch_name.is_empty() {
                        if let Some(ref mut dialog) = state.input_dialog {
                            dialog.set_error("Branch name cannot be empty".to_string());
                        }
                    } else if let Some(ref rp) = repo_path {
                        match worktree_manager.create_worktree(
                            Path::new(rp),
                            &branch_name,
                            true,
                        ) {
                            Ok(path) => {
                                state.status_message = Some(format!(
                                    "Created worktree: {}",
                                    path.display()
                                ));
                                state.close_input_dialog();
                                state.scan_workspaces();
                            }
                            Err(e) => {
                                if let Some(ref mut dialog) = state.input_dialog {
                                    dialog.set_error(format!("Failed: {}", e));
                                }
                            }
                        }
                    } else if let Some(ref mut dialog) = state.input_dialog {
                        dialog.set_error("No repository selected".to_string());
                    }
                }
                Some(InputDialogKind::DeleteWorktree { .. }) => {
                    // 'y'で確認する
                }
                Some(InputDialogKind::FilterBranches) => {
                    let filter = dialog_input.unwrap_or_default().trim().to_string();
                    state.branch_filter = if filter.is_empty() { None } else { Some(filter.clone()) };
                    state.close_input_dialog();
                    state.rebuild_tree_with_manager(Some(worktree_manager));
                    if filter.is_empty() {
                        state.status_message = Some("Filter cleared".to_string());
                    } else {
                        state.status_message = Some(format!("Filter: {}", filter));
                    }
                }
                None => {}
            }
        }
        KeyCode::Char('y') => {
            if let Some(InputDialogKind::DeleteWorktree { path }) = dialog_kind {
                if let Some(ref rp) = repo_path {
                    // チルダを展開
                    let expanded_path = if path.starts_with("~/") {
                        if let Some(home) = std::env::var_os("HOME") {
                            std::path::PathBuf::from(home).join(&path[2..])
                        } else {
                            std::path::PathBuf::from(&path)
                        }
                    } else {
                        std::path::PathBuf::from(&path)
                    };

                    match worktree_manager.remove_worktree(
                        Path::new(rp),
                        &expanded_path,
                        false,
                    ) {
                        Ok(()) => {
                            state.status_message = Some(format!("Deleted worktree: {}", path));
                            state.close_input_dialog();
                            state.scan_workspaces();
                        }
                        Err(e) => {
                            if let Some(ref mut dialog) = state.input_dialog {
                                dialog.set_error(format!("Failed: {}", e));
                            }
                        }
                    }
                }
            }
        }
        KeyCode::Char('n') => {
            if matches!(dialog_kind, Some(InputDialogKind::DeleteWorktree { .. })) {
                state.close_input_dialog();
            } else if let Some(ref mut dialog) = state.input_dialog {
                dialog.insert_char('n');
            }
        }
        KeyCode::Char(c) => {
            if !matches!(dialog_kind, Some(InputDialogKind::DeleteWorktree { .. })) {
                if let Some(ref mut dialog) = state.input_dialog {
                    dialog.insert_char(c);
                }
            }
        }
        KeyCode::Backspace => {
            if let Some(ref mut dialog) = state.input_dialog {
                dialog.backspace();
            }
        }
        KeyCode::Delete => {
            if let Some(ref mut dialog) = state.input_dialog {
                dialog.delete();
            }
        }
        KeyCode::Left => {
            if let Some(ref mut dialog) = state.input_dialog {
                dialog.move_cursor_left();
            }
        }
        KeyCode::Right => {
            if let Some(ref mut dialog) = state.input_dialog {
                dialog.move_cursor_right();
            }
        }
        _ => {}
    }
    Ok(())
}

/// 選択モードでのキーイベント処理
fn handle_selection_event(
    state: &mut AppState,
    key: KeyEvent,
    zellij: &mut ZellijActions,
    config: &mut Config,
) -> Result<()> {
    match key.code {
        KeyCode::Esc => {
            state.close_selection_dialog();
        }
        KeyCode::Up | KeyCode::Char('k') => {
            state.selection_move_up();
        }
        KeyCode::Down | KeyCode::Char('j') => {
            state.selection_move_down();
        }
        KeyCode::Enter => {
            let selected = state.get_selected_dialog_item().map(|s| s.to_string());
            let dialog_kind = state.selection_dialog_kind().cloned();
            let context = state.selection_dialog_context().cloned();

            if let (Some(selected_item), Some(kind), Some(ctx)) = (selected, dialog_kind, context) {
                match kind {
                    SelectionDialogKind::SelectSession => {
                        // セッションを選択した場合、そのセッション名を設定してタブを開く
                        zellij.set_session_name(selected_item.clone());
                        // 設定ファイルに保存
                        if let Err(e) = config.save_zellij_session(selected_item.clone()) {
                            state.status_message = Some(format!("Warning: Failed to save config: {}", e));
                        }
                        state.close_selection_dialog();

                        // タブを開く
                        let tab_name = config.zellij.generate_tab_name(&ctx.repo_name, &ctx.branch_name);
                        let cwd = Path::new(&ctx.workspace_path);
                        let layout = config.zellij.default_layout.as_deref();

                        match zellij.open_workspace_tab(&tab_name, cwd, layout) {
                            Ok(TabActionResult::SwitchedToExisting(name)) => {
                                state.status_message = Some(format!("Switched to tab: {}", name));
                            }
                            Ok(TabActionResult::CreatedNew(name)) => {
                                state.status_message = Some(format!("Created tab: {}", name));
                            }
                            Ok(TabActionResult::SessionNotFound(session)) => {
                                state.status_message = Some(format!("Session '{}' not found", session));
                            }
                            Err(e) => {
                                state.status_message = Some(format!("Error: {}", e));
                            }
                        }
                    }
                    SelectionDialogKind::SelectLayout => {
                        // レイアウトを選択した場合
                        state.close_selection_dialog();

                        let tab_name = config.zellij.generate_tab_name(&ctx.repo_name, &ctx.branch_name);
                        let cwd = Path::new(&ctx.workspace_path);

                        // レイアウトパスを構築
                        let layout_dir = config.zellij.layout_dir.as_ref();
                        let layout_path = layout_dir.map(|dir| dir.join(format!("{}.kdl", selected_item)));
                        let layout = layout_path.as_deref();

                        // デフォルトレイアウトとして保存
                        if let Some(ref path) = layout_path {
                            if let Err(e) = config.save_zellij_layout(path.clone()) {
                                state.status_message = Some(format!("Warning: Failed to save config: {}", e));
                            }
                        }

                        match zellij.open_workspace_tab(&tab_name, cwd, layout) {
                            Ok(TabActionResult::SwitchedToExisting(name)) => {
                                state.status_message = Some(format!("Switched to tab: {}", name));
                            }
                            Ok(TabActionResult::CreatedNew(name)) => {
                                state.status_message = Some(format!("Created tab: {} (layout: {})", name, selected_item));
                            }
                            Ok(TabActionResult::SessionNotFound(session)) => {
                                state.status_message = Some(format!("Session '{}' not found", session));
                            }
                            Err(e) => {
                                state.status_message = Some(format!("Error: {}", e));
                            }
                        }
                    }
                }
            }
        }
        _ => {}
    }
    Ok(())
}

/// Handle notify events from the UDS listener
fn handle_notify_event(state: &mut AppState, event: AppEvent) {
    match event {
        AppEvent::SessionRegister {
            external_id,
            project_path,
            tool,
            pane_id,
        } => {
            tracing::info!(
                "Session registered: external_id={}, path={}, tool={:?}",
                external_id,
                project_path,
                tool
            );
            // Register the session
            if let Some(session_index) = state.register_session(
                external_id.clone(),
                &project_path,
                tool,
                pane_id,
            ) {
                tracing::info!(
                    "Session registered at index {}: {}",
                    session_index,
                    external_id
                );
                // Rebuild tree to show the new session
                state.rebuild_tree();
            } else {
                tracing::warn!(
                    "No matching workspace found for path: {}",
                    project_path
                );
            }
        }
        AppEvent::SessionUpdate {
            external_id,
            status,
            message,
        } => {
            tracing::info!(
                "Session status update: external_id={}, status={:?}",
                external_id,
                status
            );
            state.update_session_status(&external_id, status, message);
        }
        AppEvent::SessionUnregister { external_id } => {
            tracing::info!("Session unregistered: external_id={}", external_id);
            state.remove_session(&external_id);
            state.rebuild_tree();
        }
        AppEvent::SessionStatusAnalyzed {
            external_id,
            project_path,
            status,
        } => {
            tracing::debug!(
                "AI status update: external_id={}, path={}, status={:?}",
                external_id,
                project_path,
                status.status
            );

            // Check if session exists, if not register it first (for polling)
            let is_new_session = state.get_session_by_external_id(&external_id).is_none();
            if is_new_session {
                // Determine tool from external_id prefix
                let tool = if external_id.starts_with("kiro:") {
                    AiTool::Kiro
                } else {
                    AiTool::Claude
                };

                if let Some(_) = state.register_session(
                    external_id.clone(),
                    &project_path,
                    tool,
                    None,
                ) {
                    tracing::debug!("Auto-registered session from polling: {}", external_id);
                    // Rebuild tree to show the new session
                    state.rebuild_tree();
                }
            }

            // Update session with AI analysis status
            if let Some(session) = state.get_session_by_external_id_mut(&external_id) {
                session.update_from_logwatch_status(&status);
                tracing::debug!(
                    "Updated session {} with AI status: {:?}",
                    external_id,
                    session.summary
                );
            }
        }
        _ => {}
    }
}

fn handle_action(
    state: &mut AppState,
    zellij: &mut ZellijActions,
    config: &mut Config,
    _worktree_manager: &WorktreeManager,
    action: Action,
) -> Result<()> {
    match action {
        Action::Quit => {
            state.should_quit = true;
        }
        Action::MoveUp => {
            state.move_up();
        }
        Action::MoveDown => {
            state.move_down();
        }
        Action::ToggleHelp => {
            state.toggle_help();
        }
        Action::Back => {
            if state.view_mode != ViewMode::List {
                state.view_mode = ViewMode::List;
                state.input_dialog = None;
            }
        }
        Action::Refresh => {
            state.status_message = Some("Scanning workspaces...".to_string());
            state.scan_workspaces();
            state.rebuild_tree_with_manager(Some(_worktree_manager));
        }
        Action::Select => {
            if let Some(ws) = state.selected_workspace() {
                // Zellij Internal mode: ペインにフォーカス
                if zellij.is_internal() {
                    // Get pane_id from session associated with this workspace
                    let workspace_index = state.workspaces.iter().position(|w| w.id == ws.id);
                    let pane_id = workspace_index.and_then(|idx| {
                        state.sessions_for_workspace(idx)
                            .first()
                            .and_then(|&si| state.sessions.get(si))
                            .and_then(|s| s.pane_id)
                    });

                    if let Some(pane_id) = pane_id {
                        if let Err(e) = zellij.focus_pane(pane_id) {
                            state.status_message = Some(format!("Failed to focus pane: {}", e));
                        }
                    } else {
                        state.view_mode = ViewMode::Detail;
                    }
                } else if config.zellij.enabled {
                    // Zellij External mode: タブを開く
                    let context = SelectionContext {
                        workspace_path: ws.project_path.clone(),
                        repo_name: ws.repo_name.clone(),
                        branch_name: ws.branch.clone(),
                    };

                    // セッション名が未設定の場合はセッション選択ダイアログを表示
                    if zellij.session_name().is_none() {
                        match zellij.list_sessions() {
                            Ok(sessions) if !sessions.is_empty() => {
                                state.open_session_select_dialog(sessions, context);
                            }
                            Ok(_) => {
                                state.status_message = Some("No Zellij sessions found".to_string());
                            }
                            Err(e) => {
                                state.status_message = Some(format!("Failed to list sessions: {}", e));
                            }
                        }
                    } else {
                        // セッション名が設定済みならタブを開く
                        let tab_name = config.zellij.generate_tab_name(&ws.repo_name, &ws.branch);
                        let cwd = Path::new(&ws.project_path);
                        let layout = config.zellij.default_layout.as_deref();

                        match zellij.open_workspace_tab(&tab_name, cwd, layout) {
                            Ok(TabActionResult::SwitchedToExisting(name)) => {
                                state.status_message = Some(format!("Switched to tab: {}", name));
                            }
                            Ok(TabActionResult::CreatedNew(name)) => {
                                state.status_message = Some(format!("Created tab: {}", name));
                            }
                            Ok(TabActionResult::SessionNotFound(session)) => {
                                state.status_message = Some(format!("Session '{}' not found", session));
                            }
                            Err(e) => {
                                state.status_message = Some(format!("Error: {}", e));
                            }
                        }
                    }
                } else {
                    state.view_mode = ViewMode::Detail;
                }
            }
        }
        Action::SelectWithLayout => {
            if let Some(ws) = state.selected_workspace() {
                if config.zellij.enabled {
                    let context = SelectionContext {
                        workspace_path: ws.project_path.clone(),
                        repo_name: ws.repo_name.clone(),
                        branch_name: ws.branch.clone(),
                    };

                    // まずセッション名が未設定ならセッション選択
                    if zellij.session_name().is_none() && !zellij.is_internal() {
                        match zellij.list_sessions() {
                            Ok(sessions) if !sessions.is_empty() => {
                                state.open_session_select_dialog(sessions, context);
                            }
                            Ok(_) => {
                                state.status_message = Some("No Zellij sessions found".to_string());
                            }
                            Err(e) => {
                                state.status_message = Some(format!("Failed to list sessions: {}", e));
                            }
                        }
                    } else {
                        // レイアウト選択ダイアログを表示
                        let layout_dir = config.zellij.layout_dir.clone()
                            .unwrap_or_else(|| {
                                directories::ProjectDirs::from("", "", "zellij")
                                    .map(|d| d.config_dir().join("layouts"))
                                    .unwrap_or_else(|| Path::new("~/.config/zellij/layouts").to_path_buf())
                            });

                        match zellij.list_layouts(&layout_dir) {
                            Ok(layouts) if !layouts.is_empty() => {
                                state.open_layout_select_dialog(layouts, context);
                            }
                            Ok(_) => {
                                state.status_message = Some("No layouts found".to_string());
                            }
                            Err(e) => {
                                state.status_message = Some(format!("Failed to list layouts: {}", e));
                            }
                        }
                    }
                } else {
                    state.status_message = Some("Zellij integration disabled".to_string());
                }
            }
        }
        Action::ToggleExpand => {
            state.toggle_expand();
        }
        Action::ToggleDisplayMode => {
            state.toggle_display_mode();
            state.rebuild_tree_with_manager(Some(_worktree_manager));
            state.status_message = Some(format!("View: {}", state.list_display_mode.label()));
        }
        Action::FilterBranches => {
            state.input_dialog = Some(InputDialog::new_filter_branches(state.branch_filter.clone()));
            state.view_mode = ViewMode::Input;
        }
        Action::ClearFilter => {
            state.branch_filter = None;
            state.rebuild_tree_with_manager(Some(_worktree_manager));
            state.status_message = Some("Filter cleared".to_string());
        }
        Action::CreateWorktree => {
            // ブランチが選択されている場合は即座にworktree作成
            if let Some((branch_name, _is_local, repo_path)) = state.selected_branch_info() {
                let branch_name = branch_name.to_string();
                let repo_path = repo_path.to_string();
                match _worktree_manager.create_worktree(
                    Path::new(&repo_path),
                    &branch_name,
                    false, // 既存ブランチなのでcreate_branch=false
                ) {
                    Ok(path) => {
                        state.status_message = Some(format!(
                            "Created worktree: {}",
                            path.display()
                        ));
                        state.scan_workspaces();
                        state.rebuild_tree_with_manager(Some(_worktree_manager));
                    }
                    Err(e) => {
                        state.status_message = Some(format!("Failed: {}", e));
                    }
                }
            } else {
                // Worktreeまたはグループ選択時は既存のダイアログを開く
                state.open_create_worktree_dialog();
            }
        }
        Action::DeleteWorktree => {
            state.open_delete_worktree_dialog();
        }
        Action::OpenInEditor => {
            if let Some(ws) = state.selected_workspace() {
                let path = &ws.project_path;
                match std::process::Command::new(&config.editor)
                    .arg(path)
                    .spawn()
                {
                    Ok(_) => {
                        state.status_message = Some(format!("Opened in {}: {}", config.editor, path));
                    }
                    Err(e) => {
                        state.status_message = Some(format!("Failed to open editor: {}", e));
                    }
                }
            }
        }
        Action::LaunchLazygit => {
            if let Some(ws) = state.selected_workspace() {
                if zellij.is_available() {
                    let path = Path::new(&ws.project_path);
                    if let Err(e) = zellij.launch_lazygit(path) {
                        state.status_message = Some(format!("Failed to launch lazygit: {}", e));
                    }
                }
            }
        }
        Action::LaunchShell => {
            if let Some(ws) = state.selected_workspace() {
                if zellij.is_available() {
                    let path = Path::new(&ws.project_path);
                    if let Err(e) = zellij.launch_shell(path) {
                        state.status_message = Some(format!("Failed to launch shell: {}", e));
                    }
                }
            }
        }
        Action::LaunchYazi => {
            if let Some(ws) = state.selected_workspace() {
                if zellij.is_available() {
                    let path = Path::new(&ws.project_path);
                    if let Err(e) = zellij.launch_yazi(path) {
                        state.status_message = Some(format!("Failed to launch yazi: {}", e));
                    }
                }
            }
        }
        Action::NewSession => {
            if let Some(ws) = state.selected_workspace() {
                if zellij.is_available() {
                    let path = Path::new(&ws.project_path);
                    if let Err(e) = zellij.launch_claude(path) {
                        state.status_message = Some(format!("Failed to launch Claude: {}", e));
                    }
                }
            }
        }
        Action::CloseWorkspace => {
            if let Some(ws) = state.selected_workspace() {
                if zellij.is_internal() {
                    // Internal mode: ペインを閉じる
                    // Get pane_id from session associated with this workspace
                    let workspace_index = state.workspaces.iter().position(|w| w.id == ws.id);
                    let pane_id = workspace_index.and_then(|idx| {
                        state.sessions_for_workspace(idx)
                            .first()
                            .and_then(|&si| state.sessions.get(si))
                            .and_then(|s| s.pane_id)
                    });

                    if let Some(pane_id) = pane_id {
                        if let Err(e) = zellij.close_pane(pane_id) {
                            state.status_message = Some(format!("Failed to close pane: {}", e));
                        }
                    }
                } else if config.zellij.enabled {
                    // External mode: タブを閉じる
                    if let Some(session) = zellij.session_name() {
                        let tab_name = config.zellij.generate_tab_name(&ws.repo_name, &ws.branch);
                        match zellij.close_tab(session, &tab_name) {
                            Ok(()) => {
                                state.status_message = Some(format!("Closed tab: {}", tab_name));
                            }
                            Err(e) => {
                                state.status_message = Some(format!("Failed to close tab: {}", e));
                            }
                        }
                    } else {
                        state.status_message = Some("No Zellij session configured".to_string());
                    }
                } else {
                    state.status_message = Some("Zellij integration disabled".to_string());
                }
            }
        }
        Action::MouseSelect(row) => {
            let index = row as usize;
            if index < state.tree_item_count() {
                state.selected_index = index;
            }
        }
        Action::MouseDoubleClick(row) => {
            // まず行を選択
            let index = row as usize;
            if index < state.tree_item_count() {
                state.selected_index = index;
            }
            // Action::Selectと同じ処理を実行（再帰的に呼び出し）
            handle_action(state, zellij, config, _worktree_manager, Action::Select)?;
        }
        Action::MouseMiddleClick(row) => {
            // まず行を選択
            let index = row as usize;
            if index < state.tree_item_count() {
                state.selected_index = index;
            }
            // Action::CloseWorkspaceと同じ処理を実行（再帰的に呼び出し）
            handle_action(state, zellij, config, _worktree_manager, Action::CloseWorkspace)?;
        }
        Action::ScrollUp => {
            state.move_up();
        }
        Action::ScrollDown => {
            state.move_down();
        }
        Action::None => {}
    }

    Ok(())
}

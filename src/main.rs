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
use workspace_manager::logwatch::{LogAnalyzer, LogCollector, collector::CollectorConfig, analyzer::AnalyzerConfig};
use workspace_manager::notify::{self, NotifyMessage};
use workspace_manager::ui;
use workspace_manager::ui::input_dialog::InputDialogKind;
use workspace_manager::ui::selection_dialog::{SelectionContext, SelectionDialogKind};
use workspace_manager::workspace::{WorkspaceStatus, WorktreeManager};
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

/// Normalize a path by expanding ~ to home directory
fn normalize_path(path: &str) -> String {
    if path.starts_with("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return format!("{}{}", home.to_string_lossy(), &path[1..]);
        }
    }
    path.to_string()
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

    // Start log watcher if enabled (event-driven)
    let logwatch_trigger: Option<LogWatchTrigger> = if config.logwatch.enabled {
        let (trigger_tx, trigger_rx) = tokio::sync::mpsc::channel::<String>(100);
        let logwatch_tx = notify_tx.clone();
        let logwatch_config = config.logwatch.clone();
        runtime.spawn(async move {
            run_logwatch(logwatch_config, logwatch_tx, trigger_rx).await;
        });
        Some(trigger_tx)
    } else {
        None
    };

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

    let result = run_app(&mut terminal, &mut state, &mut zellij, &mut config, &worktree_manager, notify_rx, logwatch_trigger, &runtime);

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

/// Log analyzer that can be triggered on-demand
struct LogWatchService {
    collector: LogCollector,
    analyzer: LogAnalyzer,
    use_ai: bool,
    tx: tokio::sync::mpsc::Sender<AppEvent>,
}

impl LogWatchService {
    async fn new(
        config: workspace_manager::app::LogWatchConfig,
        tx: tokio::sync::mpsc::Sender<AppEvent>,
    ) -> Self {
        let collector_config = CollectorConfig {
            claude_home: config.claude_home.clone(),
            kiro_logs_dir: config.kiro_logs_dir.clone(),
            max_lines: config.max_log_lines,
            scan_interval_secs: config.analysis_interval_secs,
            min_file_age_secs: 1,
        };

        let analyzer_config = AnalyzerConfig {
            analyzer_tool: config.analyzer_tool.clone(),
            timeout_secs: 30,
            max_content_length: 50000,
        };

        let collector = LogCollector::new(collector_config);
        let analyzer = LogAnalyzer::new(analyzer_config);

        // Check if analyzer is available
        let use_ai = !config.use_heuristic && analyzer.is_available().await;
        if !use_ai {
            tracing::info!("Using heuristic analysis (AI analyzer not available or disabled)");
        }

        Self {
            collector,
            analyzer,
            use_ai,
            tx,
        }
    }

    /// Analyze logs for a specific project path (triggered by hooks)
    async fn analyze_for_project(&mut self, project_path: &str) {
        use workspace_manager::logwatch::analyzer::extract_status_heuristic;

        tracing::info!("Analyzing logs for project: {}", project_path);

        // Use read_for_project for direct access (event-driven)
        match self.collector.read_for_project(project_path) {
            Ok(Some(log)) => {
                tracing::info!("Found log with {} lines", log.lines.len());

                let status = if self.use_ai {
                    match self.analyzer.analyze(&log).await {
                        Ok(s) => s,
                        Err(e) => {
                            tracing::warn!("AI analysis failed, using heuristic: {}", e);
                            extract_status_heuristic(&log)
                        }
                    }
                } else {
                    extract_status_heuristic(&log)
                };

                tracing::info!("Analysis result: status={:?}, summary={:?}",
                    status.status, status.summary);

                let event = AppEvent::WorkspaceStatusAnalyzed {
                    project_path: project_path.to_string(),
                    status,
                };
                if self.tx.send(event).await.is_err() {
                    tracing::warn!("Event receiver dropped");
                }
            }
            Ok(None) => {
                tracing::info!("No logs found for project: {}", project_path);
            }
            Err(e) => {
                tracing::warn!("Log read error: {}", e);
            }
        }
    }
}

/// Channel for triggering log analysis
type LogWatchTrigger = tokio::sync::mpsc::Sender<String>;

/// Run log watcher service that responds to triggers and does periodic polling
async fn run_logwatch(
    config: workspace_manager::app::LogWatchConfig,
    tx: tokio::sync::mpsc::Sender<AppEvent>,
    mut trigger_rx: tokio::sync::mpsc::Receiver<String>,
) {
    use workspace_manager::logwatch::analyzer::extract_status_heuristic;

    let mut service = LogWatchService::new(config.clone(), tx.clone()).await;
    let poll_interval = Duration::from_secs(config.analysis_interval_secs);

    if config.polling_enabled {
        tracing::info!("Log watch service started (hybrid mode: events + polling every {}s)", config.analysis_interval_secs);
    } else {
        tracing::info!("Log watch service started (event-driven only, polling disabled)");
    }

    // Spawn polling task only if enabled
    let polling_handle = if config.polling_enabled {
        let poll_tx = tx.clone();
        let poll_config = config.clone();
        Some(tokio::spawn(async move {
            let collector_config = CollectorConfig {
                claude_home: poll_config.claude_home.clone(),
                kiro_logs_dir: poll_config.kiro_logs_dir.clone(),
                max_lines: poll_config.max_log_lines,
                scan_interval_secs: poll_config.analysis_interval_secs,
                min_file_age_secs: 1,
            };
            let mut collector = LogCollector::new(collector_config);

            loop {
                tokio::time::sleep(poll_interval).await;

                match collector.scan() {
                    Ok(logs) => {
                        for log in logs {
                            let project_path = log.project_path.clone().unwrap_or_default();
                            if project_path.is_empty() {
                                continue;
                            }

                            let status = extract_status_heuristic(&log);

                            let event = AppEvent::WorkspaceStatusAnalyzed {
                                project_path,
                                status,
                            };
                            if poll_tx.send(event).await.is_err() {
                                tracing::warn!("Poll receiver dropped");
                                return;
                            }
                        }
                    }
                    Err(e) => {
                        tracing::debug!("Poll scan error: {}", e);
                    }
                }
            }
        }))
    } else {
        None
    };

    // Handle event-driven triggers
    while let Some(project_path) = trigger_rx.recv().await {
        service.analyze_for_project(&project_path).await;
    }

    if let Some(handle) = polling_handle {
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
    runtime: &tokio::runtime::Runtime,
) -> Result<()> {
    // 起動直後に即座にポーリングするため10から開始
    let mut tick_count = 10u8;
    // ダブルクリック検出用の前回クリック情報
    let mut last_click: Option<(Instant, u16)> = None;
    const DOUBLE_CLICK_THRESHOLD: Duration = Duration::from_millis(300);

    loop {
        // Check for notify events (non-blocking)
        while let Ok(event) = notify_rx.try_recv() {
            // Trigger log analysis for relevant events
            if let Some(ref trigger) = logwatch_trigger {
                let path_to_analyze: Option<String> = match &event {
                    AppEvent::WorkspaceRegister { project_path, .. } => {
                        Some(project_path.clone())
                    }
                    AppEvent::WorkspaceUpdate { session_id, .. } => {
                        // Find project path from session_id
                        state.workspaces.iter()
                            .find(|w| w.session_id.as_ref() == Some(session_id))
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

        // 1秒ごとにZellijタブ状態を更新（100ms × 10回 = 1秒）
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
            state.close_input_dialog();
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
        AppEvent::WorkspaceRegister {
            session_id,
            project_path,
            pane_id,
        } => {
            tracing::info!(
                "Workspace registered: session={}, path={}",
                session_id,
                project_path
            );
            // Find matching workspace by project_path and update session_id
            // Compare using normalized paths (expand ~ to home dir)
            let normalized_path = normalize_path(&project_path);
            if let Some(ws) = state
                .workspaces
                .iter_mut()
                .find(|w| normalize_path(&w.project_path) == normalized_path)
            {
                ws.session_id = Some(session_id.clone());
                ws.pane_id = pane_id;
                ws.status = WorkspaceStatus::NeedsInput;  // 登録直後は入力待ち
                ws.updated_at = std::time::SystemTime::now();
                tracing::info!("Matched workspace: {} -> session {}", ws.project_path, session_id);
            } else {
                tracing::warn!("No matching workspace found for path: {}", project_path);
            }
        }
        AppEvent::WorkspaceUpdate {
            session_id,
            status,
            message,
        } => {
            tracing::info!(
                "Workspace status update: session={}, status={:?}",
                session_id,
                status
            );
            // Find workspace by session_id and update status
            if let Some(ws) = state
                .workspaces
                .iter_mut()
                .find(|w| w.session_id.as_ref() == Some(&session_id))
            {
                ws.status = status;
                ws.message = message;
                ws.updated_at = std::time::SystemTime::now();
            }
        }
        AppEvent::WorkspaceUnregister { session_id } => {
            tracing::info!("Workspace unregistered: session={}", session_id);
            // Find workspace by session_id and mark as disconnected
            if let Some(ws) = state
                .workspaces
                .iter_mut()
                .find(|w| w.session_id.as_ref() == Some(&session_id))
            {
                ws.session_id = None;
                ws.status = WorkspaceStatus::Disconnected;
                ws.updated_at = std::time::SystemTime::now();
            }
        }
        AppEvent::WorkspaceStatusAnalyzed { project_path, status } => {
            tracing::debug!(
                "AI status update: path={}, status={:?}",
                project_path,
                status.status
            );
            // Find workspace by project_path and update AI status
            let normalized_path = normalize_path(&project_path);
            if let Some(ws) = state
                .workspaces
                .iter_mut()
                .find(|w| normalize_path(&w.project_path) == normalized_path)
            {
                ws.update_from_ai_status(&status);
                tracing::debug!(
                    "Updated workspace {} with AI status: {:?}",
                    ws.repo_name,
                    ws.ai_summary
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
                    if let Some(pane_id) = ws.pane_id {
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
                    if let Some(pane_id) = ws.pane_id {
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

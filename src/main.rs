use anyhow::Result;
use clap::{Parser, Subcommand};
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::time::Duration;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use workspace_manager::app::{Action, AppEvent, AppState, poll_event};
use workspace_manager::ui;
use workspace_manager::zellij::ZellijActions;

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
    /// Register a new workspace
    Register,
    /// Update workspace status
    Status {
        /// New status (idle, working, needs_input, success, error)
        status: String,
    },
    /// Unregister a workspace
    Unregister,
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
        Some(Commands::Notify { action }) => {
            info!("Notify mode not yet implemented (Phase 2)");
            match action {
                NotifyAction::Register => eprintln!("Register notification (Phase 2)"),
                NotifyAction::Status { status } => {
                    eprintln!("Status notification: {} (Phase 2)", status)
                }
                NotifyAction::Unregister => eprintln!("Unregister notification (Phase 2)"),
            }
            Ok(())
        }
        Some(Commands::Tui) | None => run_tui(),
    }
}

fn init_logging(level: &str) -> Result<()> {
    // ログファイルに出力（TUIと干渉しないように）
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
    // ターミナル初期化
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // アプリケーション状態
    let mut state = AppState::new();
    let zellij = ZellijActions::new();

    // 初期スキャン
    state.scan_workspaces();

    // メインループ
    let result = run_app(&mut terminal, &mut state, &zellij);

    // クリーンアップ
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut AppState,
    zellij: &ZellijActions,
) -> Result<()> {
    loop {
        // 描画
        terminal.draw(|frame| {
            ui::render(frame, state);
        })?;

        // イベント処理
        if let Some(event) = poll_event(Duration::from_millis(100))? {
            match event {
                AppEvent::Key(key) => {
                    let action = Action::from(key);
                    handle_action(state, zellij, action)?;
                }
                AppEvent::Resize(_, _) => {
                    // 自動的に再描画される
                }
                _ => {}
            }
        }

        if state.should_quit {
            break;
        }
    }

    Ok(())
}

fn handle_action(state: &mut AppState, zellij: &ZellijActions, action: Action) -> Result<()> {
    use workspace_manager::app::ViewMode;

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
        Action::Refresh => {
            state.status_message = Some("Scanning workspaces...".to_string());
            state.scan_workspaces();
        }
        Action::Select => {
            // Phase 3: Zellijペインにフォーカス
            if let Some(ws) = state.selected_workspace() {
                if let Some(pane_id) = ws.pane_id {
                    if zellij.is_available() {
                        if let Err(e) = zellij.focus_pane(pane_id) {
                            state.status_message = Some(format!("Failed to focus pane: {}", e));
                        }
                    }
                } else {
                    // 詳細ビューを表示
                    state.view_mode = ViewMode::Detail;
                }
            }
        }
        Action::LaunchLazygit => {
            if let Some(ws) = state.selected_workspace() {
                if zellij.is_available() {
                    let path = std::path::Path::new(&ws.project_path);
                    if let Err(e) = zellij.launch_lazygit(path) {
                        state.status_message = Some(format!("Failed to launch lazygit: {}", e));
                    }
                }
            }
        }
        Action::LaunchShell => {
            if let Some(ws) = state.selected_workspace() {
                if zellij.is_available() {
                    let path = std::path::Path::new(&ws.project_path);
                    if let Err(e) = zellij.launch_shell(path) {
                        state.status_message = Some(format!("Failed to launch shell: {}", e));
                    }
                }
            }
        }
        Action::LaunchYazi => {
            if let Some(ws) = state.selected_workspace() {
                if zellij.is_available() {
                    let path = std::path::Path::new(&ws.project_path);
                    if let Err(e) = zellij.launch_yazi(path) {
                        state.status_message = Some(format!("Failed to launch yazi: {}", e));
                    }
                }
            }
        }
        Action::NewSession => {
            if let Some(ws) = state.selected_workspace() {
                if zellij.is_available() {
                    let path = std::path::Path::new(&ws.project_path);
                    if let Err(e) = zellij.launch_claude(path) {
                        state.status_message = Some(format!("Failed to launch Claude: {}", e));
                    }
                }
            }
        }
        Action::CloseWorkspace => {
            if let Some(ws) = state.selected_workspace() {
                if let Some(pane_id) = ws.pane_id {
                    if zellij.is_available() {
                        if let Err(e) = zellij.close_pane(pane_id) {
                            state.status_message = Some(format!("Failed to close pane: {}", e));
                        }
                    }
                }
            }
        }
        Action::None => {}
    }

    Ok(())
}

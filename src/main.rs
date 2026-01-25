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
use std::time::Duration;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use workspace_manager::app::{Action, AppEvent, AppState, Config, mouse_action, poll_event, ViewMode};
use workspace_manager::ui;
use workspace_manager::ui::input_dialog::InputDialogKind;
use workspace_manager::workspace::WorktreeManager;
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
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let config = Config::default();
    let mut state = AppState::new();
    let zellij = ZellijActions::new();
    let worktree_manager = WorktreeManager::new(config.worktree.clone());

    state.scan_workspaces();
    state.rebuild_tree_with_manager(Some(&worktree_manager));

    let result = run_app(&mut terminal, &mut state, &zellij, &worktree_manager);

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
    worktree_manager: &WorktreeManager,
) -> Result<()> {
    loop {
        terminal.draw(|frame| {
            ui::render(frame, state);
        })?;

        if let Some(event) = poll_event(Duration::from_millis(100))? {
            // 入力モードの場合は特別処理
            if state.view_mode == ViewMode::Input {
                if let AppEvent::Key(key) = event {
                    handle_input_event(state, key, worktree_manager)?;
                }
            } else {
                match event {
                    AppEvent::Key(key) => {
                        let action = Action::from(key);
                        handle_action(state, zellij, worktree_manager, action)?;
                    }
                    AppEvent::Mouse(mouse) => {
                        let action = mouse_action(mouse, 0, 2);
                        handle_action(state, zellij, worktree_manager, action)?;
                    }
                    AppEvent::Resize(_, _) => {}
                    _ => {}
                }
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

fn handle_action(
    state: &mut AppState,
    zellij: &ZellijActions,
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
                if let Some(pane_id) = ws.pane_id {
                    if zellij.is_available() {
                        if let Err(e) = zellij.focus_pane(pane_id) {
                            state.status_message = Some(format!("Failed to focus pane: {}", e));
                        }
                    }
                } else {
                    state.view_mode = ViewMode::Detail;
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
                if let Some(pane_id) = ws.pane_id {
                    if zellij.is_available() {
                        if let Err(e) = zellij.close_pane(pane_id) {
                            state.status_message = Some(format!("Failed to close pane: {}", e));
                        }
                    }
                }
            }
        }
        Action::MouseSelect(row) => {
            let index = row as usize;
            if index < state.tree_item_count() {
                state.selected_index = index;
            }
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

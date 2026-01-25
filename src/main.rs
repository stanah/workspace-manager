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
use workspace_manager::ui::selection_dialog::{SelectionContext, SelectionDialogKind};
use workspace_manager::workspace::WorktreeManager;
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

    let mut config = Config::load().unwrap_or_default();
    let mut state = AppState::new();
    let mut zellij = ZellijActions::auto_detect(config.zellij.session_name.clone());
    let worktree_manager = WorktreeManager::new(config.worktree.clone());

    state.scan_workspaces();
    state.rebuild_tree_with_manager(Some(&worktree_manager));

    let result = run_app(&mut terminal, &mut state, &mut zellij, &mut config, &worktree_manager);

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
    zellij: &mut ZellijActions,
    config: &mut Config,
    worktree_manager: &WorktreeManager,
) -> Result<()> {
    loop {
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

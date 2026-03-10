pub mod detail_view;
pub mod git_log;
pub mod help_view;
pub mod input_dialog;
pub mod selection_dialog;
pub mod status_bar;
pub mod workspace_list;

pub use input_dialog::InputDialog;
pub use selection_dialog::{SelectionDialog, SelectionDialogKind, SelectionContext};

use ratatui::{
    layout::{Constraint, Layout, Rect},
    Frame,
};

use crate::app::{AppState, ViewMode};

/// 中央配置用のRect計算（共通ユーティリティ）
pub fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(area);

    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(popup_layout[1])[1]
}

/// メインUIを描画
pub fn render(frame: &mut Frame, state: &mut AppState) {
    let area = frame.area();

    if state.show_git_log {
        let top_pct = (state.git_log_split_ratio * 100.0) as u16;
        let chunks = Layout::vertical([
            Constraint::Percentage(top_pct),
            Constraint::Percentage(100 - top_pct),
        ])
        .split(area);

        workspace_list::render(frame, chunks[0], state);
        state.git_log_area = Some(chunks[1]);
        git_log::render(frame, chunks[1], state);
    } else {
        // 全面表示
        state.git_log_area = None;
        workspace_list::render(frame, area, state);
    }

    // オーバーレイ（全画面に対して表示）
    match &state.view_mode {
        ViewMode::Help => {
            help_view::render(frame, area);
        }
        ViewMode::Detail => {
            if let Some(ws) = state.selected_workspace() {
                detail_view::render(frame, area, ws, state);
            }
        }
        ViewMode::Input => {
            if let Some(ref dialog) = state.input_dialog {
                input_dialog::render(frame, area, dialog);
            }
        }
        ViewMode::Selection => {
            if let Some(ref dialog) = state.selection_dialog {
                selection_dialog::render(frame, area, dialog);
            }
        }
        ViewMode::List => {}
    }
}

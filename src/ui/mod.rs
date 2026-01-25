pub mod detail_view;
pub mod help_view;
pub mod input_dialog;
pub mod selection_dialog;
pub mod status_bar;
pub mod workspace_list;

pub use input_dialog::InputDialog;
pub use selection_dialog::{SelectionDialog, SelectionDialogKind, SelectionContext};

use ratatui::{
    layout::{Constraint, Layout},
    Frame,
};

use crate::app::{AppState, ViewMode};

/// メインUIを描画
pub fn render(frame: &mut Frame, state: &AppState) {
    let area = frame.area();

    // メインレイアウト: ワークスペース一覧 + ステータスバー
    let chunks = Layout::vertical([
        Constraint::Min(5),    // ワークスペース一覧
        Constraint::Length(1), // ステータスバー
    ])
    .split(area);

    // ワークスペース一覧
    workspace_list::render(frame, chunks[0], state);

    // ステータスバー
    status_bar::render(frame, chunks[1], state);

    // オーバーレイ
    match &state.view_mode {
        ViewMode::Help => {
            help_view::render(frame, area);
        }
        ViewMode::Detail => {
            if let Some(ws) = state.selected_workspace() {
                detail_view::render(frame, area, ws);
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

use ratatui::{
    layout::{Constraint, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Row, Table, TableState},
    Frame,
};

use crate::app::AppState;
use crate::workspace::Workspace;

/// ワークスペース一覧を描画
pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
    let header = Row::new(vec![
        "Status",
        "Repository",
        "Branch",
        "Message",
    ])
    .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
    .height(1);

    let rows: Vec<Row> = state
        .workspaces
        .iter()
        .enumerate()
        .map(|(idx, ws)| create_row(ws, idx == state.selected_index))
        .collect();

    let widths = [
        Constraint::Length(8),  // Status
        Constraint::Min(20),    // Repository
        Constraint::Length(25), // Branch
        Constraint::Min(30),    // Message
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .title(" Workspaces ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .row_highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        );

    let mut table_state = TableState::default();
    table_state.select(Some(state.selected_index));

    frame.render_stateful_widget(table, area, &mut table_state);
}

fn create_row(ws: &Workspace, is_selected: bool) -> Row<'static> {
    let status_style = Style::default().fg(ws.status.color());
    let status = Span::styled(format!(" {} ", ws.status.icon()), status_style);

    let repo_style = if is_selected {
        Style::default().add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };

    let branch_style = Style::default().fg(Color::Green);

    let message = ws.message.clone().unwrap_or_default();
    let message_style = Style::default().fg(Color::Gray);

    Row::new(vec![
        Line::from(status),
        Line::from(Span::styled(ws.repo_name.clone(), repo_style)),
        Line::from(Span::styled(ws.branch.clone(), branch_style)),
        Line::from(Span::styled(message, message_style)),
    ])
    .height(1)
}

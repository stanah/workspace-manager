use ratatui::{
    layout::{Constraint, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Row, Table, TableState},
    Frame,
};

use crate::app::{AppState, TreeItem};

/// ワークスペース一覧をツリー形式で描画
pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
    let header = Row::new(vec![
        "Status",
        "Name",
        "Branch",
        "Message",
    ])
    .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
    .height(1);

    let rows: Vec<Row> = state
        .tree_items
        .iter()
        .enumerate()
        .map(|(idx, item)| create_tree_row(item, state, idx == state.selected_index))
        .collect();

    let widths = [
        Constraint::Length(8),  // Status
        Constraint::Min(25),    // Name (with tree prefix)
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

fn create_tree_row(item: &TreeItem, state: &AppState, is_selected: bool) -> Row<'static> {
    match item {
        TreeItem::RepoGroup { name, expanded, worktree_count, .. } => {
            // リポジトリグループ行
            let expand_icon = if *expanded { "▼" } else { "▶" };
            let name_style = Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD);

            Row::new(vec![
                Line::from(Span::styled(
                    format!(" {} ", expand_icon),
                    Style::default().fg(Color::Yellow),
                )),
                Line::from(Span::styled(name.clone(), name_style)),
                Line::from(Span::styled(
                    format!("({} worktrees)", worktree_count),
                    Style::default().fg(Color::DarkGray),
                )),
                Line::from(""),
            ])
            .height(1)
        }
        TreeItem::Worktree { workspace_index, is_last } => {
            // worktree行
            if let Some(ws) = state.workspaces.get(*workspace_index) {
                let tree_prefix = if *is_last { "  └─ " } else { "  ├─ " };
                let status_style = Style::default().fg(ws.status.color());
                let status = Span::styled(format!(" {} ", ws.status.icon()), status_style);

                let name_style = if is_selected {
                    Style::default().add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };

                let branch_style = Style::default().fg(Color::Green);
                let message = ws.message.clone().unwrap_or_default();
                let message_style = Style::default().fg(Color::Gray);

                Row::new(vec![
                    Line::from(status),
                    Line::from(vec![
                        Span::styled(tree_prefix, Style::default().fg(Color::DarkGray)),
                        Span::styled(ws.branch.clone(), name_style),
                    ]),
                    Line::from(Span::styled(ws.display_path(), branch_style)),
                    Line::from(Span::styled(message, message_style)),
                ])
                .height(1)
            } else {
                Row::new(vec![
                    Line::from(""),
                    Line::from("  └─ <invalid>"),
                    Line::from(""),
                    Line::from(""),
                ])
                .height(1)
            }
        }
    }
}

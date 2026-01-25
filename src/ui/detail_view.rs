use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::workspace::Workspace;

/// 詳細ビューを描画
pub fn render(frame: &mut Frame, area: Rect, workspace: &Workspace) {
    let popup_area = centered_rect(70, 50, area);

    frame.render_widget(Clear, popup_area);

    let details = vec![
        Line::from(vec![
            Span::styled("Repository: ", Style::default().fg(Color::Yellow)),
            Span::raw(&workspace.repo_name),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Branch:     ", Style::default().fg(Color::Yellow)),
            Span::styled(&workspace.branch, Style::default().fg(Color::Green)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Path:       ", Style::default().fg(Color::Yellow)),
            Span::raw(workspace.display_path()),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Status:     ", Style::default().fg(Color::Yellow)),
            Span::styled(
                format!("{} {:?}", workspace.status.icon(), workspace.status),
                Style::default().fg(workspace.status.color()),
            ),
        ]),
        Line::from(""),
        if let Some(ref session_id) = workspace.session_id {
            Line::from(vec![
                Span::styled("Session:    ", Style::default().fg(Color::Yellow)),
                Span::raw(session_id),
            ])
        } else {
            Line::from(vec![
                Span::styled("Session:    ", Style::default().fg(Color::Yellow)),
                Span::styled("Not connected", Style::default().fg(Color::DarkGray)),
            ])
        },
        Line::from(""),
        if let Some(pane_id) = workspace.pane_id {
            Line::from(vec![
                Span::styled("Pane ID:    ", Style::default().fg(Color::Yellow)),
                Span::raw(format!("{}", pane_id)),
            ])
        } else {
            Line::from(vec![
                Span::styled("Pane ID:    ", Style::default().fg(Color::Yellow)),
                Span::styled("N/A", Style::default().fg(Color::DarkGray)),
            ])
        },
        Line::from(""),
        if let Some(ref msg) = workspace.message {
            Line::from(vec![
                Span::styled("Message:    ", Style::default().fg(Color::Yellow)),
                Span::raw(msg),
            ])
        } else {
            Line::from("")
        },
    ];

    let detail = Paragraph::new(details)
        .block(
            Block::default()
                .title(format!(" {} ", workspace.repo_name))
                .title_alignment(Alignment::Center)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .alignment(Alignment::Left);

    frame.render_widget(detail, popup_area);
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
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

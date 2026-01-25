use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

/// ヘルプオーバーレイを描画
pub fn render(frame: &mut Frame, area: Rect) {
    // 中央に配置
    let popup_area = centered_rect(60, 70, area);

    // 背景をクリア
    frame.render_widget(Clear, popup_area);

    let help_text = vec![
        Line::from(vec![
            Span::styled("Navigation", Style::default().add_modifier(Modifier::BOLD)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  j/↓  ", Style::default().fg(Color::Yellow)),
            Span::raw("Move down"),
        ]),
        Line::from(vec![
            Span::styled("  k/↑  ", Style::default().fg(Color::Yellow)),
            Span::raw("Move up"),
        ]),
        Line::from(vec![
            Span::styled("  Enter", Style::default().fg(Color::Yellow)),
            Span::raw("  Focus workspace pane"),
        ]),
        Line::from(vec![
            Span::styled("  r    ", Style::default().fg(Color::Yellow)),
            Span::raw("  Refresh workspace list"),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Zellij Actions", Style::default().add_modifier(Modifier::BOLD)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  l    ", Style::default().fg(Color::Yellow)),
            Span::raw("  Launch lazygit"),
        ]),
        Line::from(vec![
            Span::styled("  g    ", Style::default().fg(Color::Yellow)),
            Span::raw("  Launch shell"),
        ]),
        Line::from(vec![
            Span::styled("  y    ", Style::default().fg(Color::Yellow)),
            Span::raw("  Launch yazi"),
        ]),
        Line::from(vec![
            Span::styled("  n    ", Style::default().fg(Color::Yellow)),
            Span::raw("  New Claude Code session"),
        ]),
        Line::from(vec![
            Span::styled("  x    ", Style::default().fg(Color::Yellow)),
            Span::raw("  Close workspace"),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Other", Style::default().add_modifier(Modifier::BOLD)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  ?    ", Style::default().fg(Color::Yellow)),
            Span::raw("  Toggle this help"),
        ]),
        Line::from(vec![
            Span::styled("  q    ", Style::default().fg(Color::Yellow)),
            Span::raw("  Quit"),
        ]),
    ];

    let help = Paragraph::new(help_text)
        .block(
            Block::default()
                .title(" Help ")
                .title_alignment(Alignment::Center)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .alignment(Alignment::Left);

    frame.render_widget(help, popup_area);
}

/// 中央配置用のRect計算
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

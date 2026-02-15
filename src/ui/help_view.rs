use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use super::centered_rect;

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
            Span::styled("  Space", Style::default().fg(Color::Yellow)),
            Span::raw("  Expand/collapse repo group"),
        ]),
        Line::from(vec![
            Span::styled("  r    ", Style::default().fg(Color::Yellow)),
            Span::raw("  Refresh workspace list"),
        ]),
        Line::from(vec![
            Span::styled("  v    ", Style::default().fg(Color::Yellow)),
            Span::raw("  Toggle view mode (Worktrees/+Branches/Running)"),
        ]),
        Line::from(vec![
            Span::styled("  /    ", Style::default().fg(Color::Yellow)),
            Span::raw("  Filter branches"),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Worktree Management", Style::default().add_modifier(Modifier::BOLD)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  c/a  ", Style::default().fg(Color::Yellow)),
            Span::raw("  Create new worktree"),
        ]),
        Line::from(vec![
            Span::styled("  d    ", Style::default().fg(Color::Yellow)),
            Span::raw("  Delete worktree"),
        ]),
        Line::from(vec![
            Span::styled("  D    ", Style::default().fg(Color::Yellow)),
            Span::raw("  Force delete worktree (submodules etc.)"),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Multiplexer Actions", Style::default().add_modifier(Modifier::BOLD)),
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
            Span::styled("  e    ", Style::default().fg(Color::Yellow)),
            Span::raw("  Open in editor"),
        ]),
        Line::from(vec![
            Span::styled("  ?    ", Style::default().fg(Color::Yellow)),
            Span::raw("  Toggle this help"),
        ]),
        Line::from(vec![
            Span::styled("  Esc  ", Style::default().fg(Color::Yellow)),
            Span::raw("  Close overlay / Back"),
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

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::app::AppState;

/// ステータスバーを描画
pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
    let active = state.active_count();
    let working = state.working_count();
    let total = state.workspaces.len();

    let left_content = if let Some(ref msg) = state.status_message {
        Span::styled(msg.clone(), Style::default().fg(Color::Cyan))
    } else {
        Span::styled(
            format!(" {} workspaces | {} active | {} working ", total, active, working),
            Style::default().fg(Color::Gray),
        )
    };

    let help_hint = Span::styled(
        " Press ? for help ",
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::ITALIC),
    );

    // 左右に分けて表示
    let left = Paragraph::new(Line::from(left_content));
    let right = Paragraph::new(Line::from(help_hint));

    // 左側
    let left_area = Rect {
        x: area.x,
        y: area.y,
        width: area.width.saturating_sub(20),
        height: area.height,
    };

    // 右側
    let right_area = Rect {
        x: area.x + area.width.saturating_sub(20),
        y: area.y,
        width: 20.min(area.width),
        height: area.height,
    };

    frame.render_widget(left, left_area);
    frame.render_widget(right, right_area);
}

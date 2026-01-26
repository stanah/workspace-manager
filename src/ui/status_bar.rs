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
    let mode_label = state.list_display_mode.label();

    let left_content = if let Some(ref msg) = state.status_message {
        Line::from(vec![
            Span::styled(msg.clone(), Style::default().fg(Color::Cyan)),
        ])
    } else {
        Line::from(vec![
            Span::styled(
                format!(" {} workspaces | {} active | {} working ", total, active, working),
                Style::default().fg(Color::Gray),
            ),
        ])
    };

    // フィルター状態を表示
    let filter_spans = if let Some(ref filter) = state.branch_filter {
        vec![
            Span::styled("[", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("/{}", filter), Style::default().fg(Color::Magenta)),
            Span::styled("] ", Style::default().fg(Color::DarkGray)),
        ]
    } else {
        vec![]
    };

    // 表示モードと'v'キーのヒント、ヘルプヒントを右側に
    let mut right_spans = filter_spans;
    right_spans.extend(vec![
        Span::styled("[", Style::default().fg(Color::DarkGray)),
        Span::styled(mode_label, Style::default().fg(Color::Yellow)),
        Span::styled("]", Style::default().fg(Color::DarkGray)),
        Span::styled(" v:view /:filter ", Style::default().fg(Color::DarkGray)),
        Span::styled("?:help ", Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC)),
    ]);
    let right_content = Line::from(right_spans);

    // 左右に分けて表示
    let left = Paragraph::new(left_content);
    let right = Paragraph::new(right_content);

    // 右側の幅を計算（フィルターがある場合は広めに）
    let right_width = if state.branch_filter.is_some() { 50 } else { 40 };

    // 左側
    let left_area = Rect {
        x: area.x,
        y: area.y,
        width: area.width.saturating_sub(right_width),
        height: area.height,
    };

    // 右側
    let right_area = Rect {
        x: area.x + area.width.saturating_sub(right_width),
        y: area.y,
        width: right_width.min(area.width),
        height: area.height,
    };

    frame.render_widget(left, left_area);
    frame.render_widget(right, right_area);
}

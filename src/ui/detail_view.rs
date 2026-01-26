use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use super::centered_rect;
use crate::app::AppState;
use crate::workspace::{Session, SessionStatus, Workspace};

/// 詳細ビューを描画
pub fn render(frame: &mut Frame, area: Rect, workspace: &Workspace, state: &AppState) {
    let popup_area = centered_rect(70, 60, area);

    frame.render_widget(Clear, popup_area);

    // このワークスペースのセッションを取得
    let workspace_index = state
        .workspaces
        .iter()
        .position(|w| w.id == workspace.id);
    let sessions: Vec<&Session> = workspace_index
        .map(|idx| {
            state
                .sessions_for_workspace(idx)
                .iter()
                .filter_map(|&si| state.sessions.get(si))
                .collect()
        })
        .unwrap_or_default();

    // 集約ステータス
    let aggregate_status = workspace_index
        .map(|idx| state.workspace_aggregate_status(idx))
        .unwrap_or(SessionStatus::Disconnected);

    let mut details = vec![
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
                format!("{} {:?}", aggregate_status.icon(), aggregate_status),
                Style::default().fg(aggregate_status.color()),
            ),
        ]),
        Line::from(""),
    ];

    // セッション情報
    if sessions.is_empty() {
        details.push(Line::from(vec![
            Span::styled("Sessions:   ", Style::default().fg(Color::Yellow)),
            Span::styled("No active sessions", Style::default().fg(Color::DarkGray)),
        ]));
    } else {
        details.push(Line::from(vec![
            Span::styled(
                format!("Sessions:   ({} active)", sessions.len()),
                Style::default().fg(Color::Yellow),
            ),
        ]));
        details.push(Line::from(""));

        for session in sessions {
            // ツール名とステータス
            details.push(Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled(
                    format!("{} ", session.tool.icon()),
                    Style::default().fg(session.tool.color()),
                ),
                Span::styled(
                    format!("{} ", session.status.icon()),
                    Style::default().fg(session.status.color()),
                ),
                Span::styled(
                    format!("{:?}", session.status),
                    Style::default().fg(session.status.color()),
                ),
            ]));

            // セッションID
            details.push(Line::from(vec![
                Span::styled("    ID: ", Style::default().fg(Color::DarkGray)),
                Span::styled(&session.external_id, Style::default().fg(Color::DarkGray)),
            ]));

            // サマリー
            if let Some(ref summary) = session.summary {
                details.push(Line::from(vec![
                    Span::styled("    ", Style::default()),
                    Span::raw(summary),
                ]));
            }

            // Pane ID (if available)
            if let Some(pane_id) = session.pane_id {
                details.push(Line::from(vec![
                    Span::styled("    Pane: ", Style::default().fg(Color::DarkGray)),
                    Span::raw(format!("{}", pane_id)),
                ]));
            }

            details.push(Line::from(""));
        }
    }

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

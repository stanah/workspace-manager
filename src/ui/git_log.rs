use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::app::{AppState, CommitDetail};

/// UNIXタイムスタンプを相対時間文字列に変換
fn relative_time(timestamp: i64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let diff = now - timestamp;
    if diff < 0 {
        return "just now".to_string();
    }
    let diff = diff as u64;
    match diff {
        0..=59 => "just now".to_string(),
        60..=3599 => {
            let m = diff / 60;
            if m == 1 { "1 min ago".to_string() } else { format!("{m} mins ago") }
        }
        3600..=86399 => {
            let h = diff / 3600;
            if h == 1 { "1 hour ago".to_string() } else { format!("{h} hours ago") }
        }
        86400..=2591999 => {
            let d = diff / 86400;
            if d == 1 { "1 day ago".to_string() } else { format!("{d} days ago") }
        }
        2592000..=31535999 => {
            let mo = diff / 2592000;
            if mo == 1 { "1 month ago".to_string() } else { format!("{mo} months ago") }
        }
        _ => {
            let y = diff / 31536000;
            if y == 1 { "1 year ago".to_string() } else { format!("{y} years ago") }
        }
    }
}

/// Git logペインを描画
pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
    let title = match state.selected_workspace_branch() {
        Some(branch) => format!(" Git Log ({branch}) "),
        None => " Git Log ".to_string(),
    };

    let entries = state
        .git_log_cache
        .as_ref()
        .map(|(_, entries)| entries.as_slice())
        .unwrap_or(&[]);

    if entries.is_empty() {
        let empty = Paragraph::new("No commits")
            .block(
                Block::default()
                    .title(title)
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray)),
            )
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(empty, area);
        return;
    }

    let visible_lines = area.height.saturating_sub(2) as usize;
    let scroll = state.git_log_scroll;

    let lines: Vec<Line> = entries
        .iter()
        .enumerate()
        .skip(scroll)
        .take(visible_lines)
        .map(|(i, entry)| {
            let rel = relative_time(entry.timestamp);
            let is_selected = state.git_log_selected == Some(i);
            let style = if is_selected {
                Style::default().bg(Color::DarkGray)
            } else {
                Style::default()
            };
            // hash - message (xx min ago) <author>
            Line::from(vec![
                Span::styled(
                    &entry.short_hash,
                    style.fg(Color::Yellow),
                ),
                Span::styled(" - ", style),
                Span::styled(&entry.subject, style),
                Span::styled(
                    format!(" ({rel})"),
                    style.fg(Color::Rgb(100, 100, 100)),
                ),
                Span::styled(
                    format!(" <{}>", entry.author),
                    style.fg(Color::Green).add_modifier(Modifier::BOLD),
                ),
            ])
        })
        .collect();

    let total = entries.len();
    let pos_info = if total > visible_lines {
        format!(" [{}-{}/{}] ", scroll + 1, (scroll + visible_lines).min(total), total)
    } else {
        String::new()
    };

    let paragraph = Paragraph::new(lines).block(
        Block::default()
            .title(title)
            .title_bottom(Line::from(pos_info).right_aligned())
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)),
    );

    frame.render_widget(paragraph, area);

    // コミット詳細をオーバーレイ表示（Enter/ダブルクリックで開く）
    if state.git_log_show_detail {
        if let Some(detail) = state.selected_commit_detail() {
            render_commit_detail(frame, frame.area(), &detail);
        }
    }
}

/// コミット詳細オーバーレイを描画
fn render_commit_detail(frame: &mut Frame, area: Rect, detail: &CommitDetail) {
    // 中央にポップアップ
    let popup_area = centered_rect_abs(area, 80, 70);
    frame.render_widget(Clear, popup_area);

    let inner = Layout::vertical([
        Constraint::Length(6), // ヘッダー情報
        Constraint::Min(1),   // ファイル一覧
    ])
    .split(Block::default().borders(Borders::ALL).inner(popup_area));

    let rel = relative_time(detail.timestamp);
    let header_lines = vec![
        Line::from(vec![
            Span::styled("commit ", Style::default().fg(Color::DarkGray)),
            Span::styled(&detail.hash, Style::default().fg(Color::Yellow)),
        ]),
        Line::from(vec![
            Span::styled("Author: ", Style::default().fg(Color::DarkGray)),
            Span::styled(&detail.author, Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::styled(format!("  ({rel})"), Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("    ", Style::default()),
            Span::raw(&detail.subject),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                format!(
                    " {} file{} changed, {} insertion{}(+), {} deletion{}(-)",
                    detail.files_changed,
                    if detail.files_changed != 1 { "s" } else { "" },
                    detail.insertions,
                    if detail.insertions != 1 { "s" } else { "" },
                    detail.deletions,
                    if detail.deletions != 1 { "s" } else { "" },
                ),
                Style::default().fg(Color::DarkGray),
            ),
        ]),
    ];

    let header = Paragraph::new(header_lines);
    frame.render_widget(header, inner[0]);

    let file_lines: Vec<Line> = detail
        .files
        .iter()
        .map(|(status, path)| {
            let color = match status {
                'A' => Color::Green,
                'D' => Color::Red,
                'M' => Color::Yellow,
                'R' => Color::Cyan,
                _ => Color::White,
            };
            Line::from(vec![
                Span::styled(format!(" {status} "), Style::default().fg(color).add_modifier(Modifier::BOLD)),
                Span::raw(path),
            ])
        })
        .collect();

    let files = Paragraph::new(file_lines).wrap(Wrap { trim: true });
    frame.render_widget(files, inner[1]);

    // 外枠
    let block = Block::default()
        .title(" Commit Detail (click elsewhere to close) ")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    frame.render_widget(block, popup_area);
}

/// 絶対サイズ指定の中央配置Rect
fn centered_rect_abs(area: Rect, max_width: u16, max_height_pct: u16) -> Rect {
    let height = (area.height as u32 * max_height_pct as u32 / 100) as u16;
    let width = max_width.min(area.width.saturating_sub(4));
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width, height)
}

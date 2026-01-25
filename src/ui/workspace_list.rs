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
    let rows: Vec<Row> = state
        .tree_items
        .iter()
        .enumerate()
        .map(|(idx, item)| create_tree_row(item, state, idx == state.selected_index))
        .collect();

    // 単一カラムレイアウト（狭いペイン対応）
    let widths = [Constraint::Min(10)];

    let table = Table::new(rows, widths)
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
        TreeItem::RepoGroup { name, worktree_count, .. } => {
            // リポジトリグループ行
            let name_style = Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD);
            let count_style = Style::default().fg(Color::DarkGray);

            Row::new(vec![
                Line::from(vec![
                    Span::styled(name.clone(), name_style),
                    Span::styled(format!(" ({})", worktree_count), count_style),
                ]),
            ])
            .height(1)
        }
        TreeItem::Worktree { workspace_index, is_last } => {
            // worktree行: ステータスアイコンをブランチ名の前に表示
            if let Some(ws) = state.workspaces.get(*workspace_index) {
                let tree_prefix = if *is_last { "└ " } else { "├ " };
                let is_open = state.is_workspace_open(&ws.repo_name, &ws.branch);

                // ステータスアイコンの色はAIツールのステータスを反映
                // Disconnected状態でZellijで開いている場合は緑に
                let status_color = if is_open && ws.status == crate::workspace::WorkspaceStatus::Disconnected {
                    Color::Green
                } else {
                    ws.status.color()
                };
                let status_style = Style::default().fg(status_color);
                let status_icon = format!("{} ", ws.status.icon());

                // ブランチ名のスタイル：開いていれば緑、選択中は太字
                let name_style = match (is_selected, is_open) {
                    (true, true) => Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
                    (true, false) => Style::default().add_modifier(Modifier::BOLD),
                    (false, true) => Style::default().fg(Color::Green),
                    (false, false) => Style::default(),
                };

                // AI解析結果があれば表示
                let ai_info = if let Some(ref summary) = ws.ai_summary {
                    let detail = ws.ai_state_detail.as_deref().unwrap_or("");
                    let time = ws.ai_last_activity.as_deref().unwrap_or("");
                    // フォーマット: [detail] summary (time)
                    let info = if !detail.is_empty() && !time.is_empty() {
                        format!(" [{}] {} ({})", detail, summary, time)
                    } else if !detail.is_empty() {
                        format!(" [{}] {}", detail, summary)
                    } else if !time.is_empty() {
                        format!(" {} ({})", summary, time)
                    } else {
                        format!(" {}", summary)
                    };
                    Some(info)
                } else {
                    None
                };

                let mut spans = vec![
                    Span::styled("  ", Style::default()),
                    Span::styled(tree_prefix, Style::default().fg(Color::DarkGray)),
                    Span::styled(status_icon, status_style),
                    Span::styled(ws.branch.clone(), name_style),
                ];

                // AI情報を追加
                if let Some(info) = ai_info {
                    spans.push(Span::styled(info, Style::default().fg(Color::DarkGray)));
                }

                Row::new(vec![Line::from(spans)])
                    .height(1)
            } else {
                Row::new(vec![Line::from("  └ <invalid>")])
                .height(1)
            }
        }
        TreeItem::Branch { name, is_local, is_last, .. } => {
            // ブランチ行（worktree未作成）- 控えめな暗い色で表示
            let tree_prefix = if *is_last { "└ " } else { "├ " };

            // リモートは "origin/..." 形式で表示
            let display_name = if *is_local {
                name.clone()
            } else {
                format!("origin/{}", name)
            };

            let name_style = if is_selected {
                Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            };

            Row::new(vec![
                Line::from(vec![
                    Span::styled("  ", Style::default()),
                    Span::styled(tree_prefix, Style::default().fg(Color::DarkGray)),
                    Span::styled("  ", Style::default()), // アイコン分のスペース
                    Span::styled(display_name, name_style),
                ]),
            ])
            .height(1)
        }
    }
}

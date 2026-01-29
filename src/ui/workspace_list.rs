use ratatui::{
    layout::{Constraint, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Row, Table},
    Frame,
};

use crate::app::{AppState, TreeItem};
use crate::workspace::SessionStatus;

/// ワークスペース一覧をツリー形式で描画
pub fn render(frame: &mut Frame, area: Rect, state: &mut AppState) {
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

    frame.render_stateful_widget(table, area, &mut state.table_state);
}

fn create_tree_row(item: &TreeItem, state: &AppState, is_selected: bool) -> Row<'static> {
    match item {
        TreeItem::RepoGroup {
            name,
            worktree_count,
            ..
        } => {
            // リポジトリグループ行
            let name_style = Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD);
            let count_style = Style::default().fg(Color::DarkGray);

            Row::new(vec![Line::from(vec![
                Span::styled(name.clone(), name_style),
                Span::styled(format!(" ({})", worktree_count), count_style),
            ])])
            .height(1)
        }
        TreeItem::Worktree {
            workspace_index,
            is_last,
        } => {
            // worktree行: ステータスアイコンをブランチ名の前に表示
            if let Some(ws) = state.workspaces.get(*workspace_index) {
                let tree_prefix = if *is_last { "└ " } else { "├ " };
                let is_open = state.is_workspace_open(&ws.repo_name, &ws.branch);

                // 集約ステータスを取得
                let aggregate_status = state.workspace_aggregate_status(*workspace_index);

                // ステータスアイコンの色はセッションのステータスを反映
                // Disconnected状態でZellijで開いている場合は緑に
                let status_color =
                    if is_open && aggregate_status == SessionStatus::Disconnected {
                        Color::Green
                    } else {
                        aggregate_status.color()
                    };
                let status_style = Style::default().fg(status_color);
                let status_icon = format!("{} ", aggregate_status.icon());

                // ブランチ名のスタイル：開いていれば緑、選択中は太字
                let name_style = match (is_selected, is_open) {
                    (true, true) => Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                    (true, false) => Style::default().add_modifier(Modifier::BOLD),
                    (false, true) => Style::default().fg(Color::Green),
                    (false, false) => Style::default(),
                };

                // セッション数を表示
                let session_count = state.sessions_for_workspace(*workspace_index).len();
                let session_info = if session_count > 0 {
                    Some(format!(" [{} session{}]", session_count, if session_count > 1 { "s" } else { "" }))
                } else {
                    None
                };

                let mut spans = vec![
                    Span::styled("  ", Style::default()),
                    Span::styled(tree_prefix, Style::default().fg(Color::DarkGray)),
                    Span::styled(status_icon, status_style),
                    Span::styled(ws.branch.clone(), name_style),
                ];

                // セッション数を追加
                if let Some(info) = session_info {
                    spans.push(Span::styled(info, Style::default().fg(Color::DarkGray)));
                }

                Row::new(vec![Line::from(spans)]).height(1)
            } else {
                Row::new(vec![Line::from("  └ <invalid>")]).height(1)
            }
        }
        TreeItem::Session {
            session_index,
            is_last,
            parent_is_last,
        } => {
            // セッション行: ツールアイコンとステータスを表示
            if let Some(session) = state.sessions.get(*session_index) {
                let continuation = if *parent_is_last { "  " } else { "│ " };
                let branch_char = if *is_last { "└ " } else { "├ " };
                let tree_prefix = format!("{}{}", continuation, branch_char);

                // ツールアイコンとステータス
                let tool_icon = session.tool.icon(state.use_nerd_font);
                let tool_color = session.tool.color();
                let status_color = session.status.color();
                let status_icon = session.status.icon();

                // セッション情報
                let info = session.display_info();

                let name_style = if is_selected {
                    Style::default().add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };

                let spans = vec![
                    Span::styled("  ", Style::default()),
                    Span::styled(tree_prefix, Style::default().fg(Color::DarkGray)),
                    Span::styled(format!("{} ", tool_icon), Style::default().fg(tool_color)),
                    Span::styled(format!("{} ", status_icon), Style::default().fg(status_color)),
                    Span::styled(info, name_style.fg(Color::DarkGray)),
                ];

                Row::new(vec![Line::from(spans)]).height(1)
            } else {
                Row::new(vec![Line::from("    └ <invalid session>")]).height(1)
            }
        }
        TreeItem::RemoteBranchGroup {
            expanded,
            count,
            is_last,
            ..
        } => {
            // リモートブランチグループ行
            let tree_prefix = if *is_last { "└ " } else { "├ " };
            let expand_icon = if *expanded { "▼" } else { "▶" };
            let label_style = Style::default().fg(Color::DarkGray);
            let count_style = Style::default().fg(Color::DarkGray);

            Row::new(vec![Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled(tree_prefix, Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{} ", expand_icon), label_style),
                Span::styled("Remote Branches", label_style),
                Span::styled(format!(" ({})", count), count_style),
            ])])
            .height(1)
        }
        TreeItem::Branch {
            name,
            is_local,
            is_last,
            ..
        } => {
            // ブランチ行（worktree未作成）- 控えめな暗い色で表示
            let tree_prefix = if *is_last { "└ " } else { "├ " };

            // リモートブランチはRemoteBranchGroupの子として追加インデント
            let indent = if *is_local { "  " } else { "    " };

            // リモートは "origin/..." 形式で表示
            let display_name = if *is_local {
                name.clone()
            } else {
                format!("origin/{}", name)
            };

            let name_style = if is_selected {
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            };

            Row::new(vec![Line::from(vec![
                Span::styled(indent, Style::default()),
                Span::styled(tree_prefix, Style::default().fg(Color::DarkGray)),
                Span::styled("  ", Style::default()), // アイコン分のスペース
                Span::styled(display_name, name_style),
            ])])
            .height(1)
        }
    }
}

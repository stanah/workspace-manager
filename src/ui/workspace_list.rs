use ratatui::{
    layout::{Constraint, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Row, Table},
    Frame,
};

use crate::app::{AppState, ListDisplayMode, TreeItem};

/// ワークスペース一覧をツリー形式で描画
pub fn render(frame: &mut Frame, area: Rect, state: &mut AppState) {
    // RunningOnly モードで表示するワークスペースがない場合のメッセージ
    if state.tree_items.is_empty() && state.list_display_mode == ListDisplayMode::RunningOnly {
        let message = Paragraph::new(Line::from(vec![
            Span::styled(
                "No running sessions. Press ",
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled("v", Style::default().fg(Color::Yellow)),
            Span::styled(
                " to switch view mode.",
                Style::default().fg(Color::DarkGray),
            ),
        ]))
        .block(
            Block::default()
                .title(" Workspaces ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .centered();
        frame.render_widget(message, area);
        return;
    }

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
            path,
            worktree_count,
            ..
        } => {
            // リポジトリグループ行
            let name_style = Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD);
            let count_style = Style::default().fg(Color::DarkGray);
            let is_favorite = state.favorite_repos.contains(path);

            let mut spans = Vec::new();
            if is_favorite {
                spans.push(Span::styled("★ ", Style::default().fg(Color::Yellow)));
            }
            spans.push(Span::styled(name.clone(), name_style));
            spans.push(Span::styled(format!(" ({})", worktree_count), count_style));

            Row::new(vec![Line::from(spans)]).height(1)
        }
        TreeItem::Worktree {
            workspace_index,
            is_last,
        } => {
            // worktree行: ステータスアイコンをブランチ名の前に表示
            if let Some(ws) = state.workspaces.get(*workspace_index) {
                let tree_prefix = if *is_last { "└ " } else { "├ " };
                let is_open = state.is_workspace_open(&ws.repo_name, &ws.branch);

                // ブランチアイコン
                let branch_icon = if state.use_nerd_font { "\u{E0A0} " } else { "⎇ " };

                // ブランチ名のスタイル：開いていれば緑、選択中は太字、下線で区別
                let name_style = match (is_selected, is_open) {
                    (true, true) => Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                    (true, false) => Style::default()
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                    (false, true) => Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::UNDERLINED),
                    (false, false) => Style::default()
                        .add_modifier(Modifier::UNDERLINED),
                };

                // ペイン数またはセッション数を表示
                let pane_count = state.panes_for_workspace(*workspace_index).len();
                let session_count = state.sessions_for_workspace(*workspace_index).len();
                let child_info = if pane_count > 0 {
                    Some(format!(" [{} pane{}]", pane_count, if pane_count > 1 { "s" } else { "" }))
                } else if session_count > 0 {
                    Some(format!(" [{} session{}]", session_count, if session_count > 1 { "s" } else { "" }))
                } else {
                    None
                };

                let mut spans = vec![
                    Span::styled("  ", Style::default()),
                    Span::styled(tree_prefix, Style::default().fg(Color::DarkGray)),
                    Span::styled(format!("{}{}", branch_icon, ws.branch), name_style),
                ];

                // セッション数/ペイン数を追加
                if let Some(info) = child_info {
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
        TreeItem::Pane {
            pane_index,
            is_last,
            parent_is_last,
        } => {
            if let Some(pane) = state.panes.get(*pane_index) {
                let continuation = if *parent_is_last { "  " } else { "│ " };
                let branch_char = if *is_last { "└ " } else { "├ " };
                let tree_prefix = format!("{}{}", continuation, branch_char);
                // 行頭をアクティブインジケータに使用
                let leading = if pane.is_active {
                    Span::styled("▶ ", Style::default().fg(Color::Green))
                } else {
                    Span::styled("  ", Style::default())
                };

                if pane.is_ai_pane() {
                    // AI ペイン: 従来の Session 表示と同じフォーマット
                    let ai = pane.ai_session.as_ref().unwrap();
                    let tool_icon = ai.tool.icon(state.use_nerd_font);
                    let tool_color = ai.tool.color();
                    let status_color = ai.status.color();
                    let status_icon = ai.status.icon();
                    let info = pane.display_info();

                    let name_style = if pane.is_active {
                        Style::default().add_modifier(Modifier::BOLD).fg(Color::White)
                    } else if is_selected {
                        Style::default().add_modifier(Modifier::BOLD).fg(Color::Gray)
                    } else {
                        Style::default().fg(Color::Gray)
                    };

                    let spans = vec![
                        leading,
                        Span::styled(tree_prefix, Style::default().fg(Color::DarkGray)),
                        Span::styled(format!("{} ", tool_icon), Style::default().fg(tool_color)),
                        Span::styled(format!("{} ", status_icon), Style::default().fg(status_color)),
                        Span::styled(info, name_style),
                    ];

                    Row::new(vec![Line::from(spans)]).height(1)
                } else {
                    // 通常ペイン: コマンド名
                    let name_style = if pane.is_active {
                        Style::default().add_modifier(Modifier::BOLD).fg(Color::White)
                    } else if is_selected {
                        Style::default().fg(Color::Gray).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::Gray)
                    };

                    let spans = vec![
                        leading,
                        Span::styled(tree_prefix, Style::default().fg(Color::DarkGray)),
                        Span::styled(pane.command.clone(), name_style),
                    ];

                    Row::new(vec![Line::from(spans)]).height(1)
                }
            } else {
                Row::new(vec![Line::from("    └ <invalid pane>")]).height(1)
            }
        }
        TreeItem::Separator => {
            Row::new(vec![Line::from(vec![
                Span::styled("────────────────────────────────", Style::default().fg(Color::DarkGray)),
            ])])
            .height(1)
        }
    }
}

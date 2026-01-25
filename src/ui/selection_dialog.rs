use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
    Frame,
};

use super::centered_rect;

/// 選択ダイアログの種類
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectionDialogKind {
    /// Zellijセッション選択
    SelectSession,
    /// レイアウト選択
    SelectLayout,
}

/// 選択ダイアログの状態
#[derive(Debug, Clone)]
pub struct SelectionDialog {
    pub kind: SelectionDialogKind,
    pub items: Vec<String>,
    pub selected_index: usize,
    pub title: String,
    /// 選択結果を格納するコンテキスト（ワークスペース情報など）
    pub context: Option<SelectionContext>,
}

/// 選択ダイアログのコンテキスト情報
#[derive(Debug, Clone)]
pub struct SelectionContext {
    /// 対象ワークスペースのパス
    pub workspace_path: String,
    /// 対象ワークスペースのリポジトリ名
    pub repo_name: String,
    /// 対象ワークスペースのブランチ名
    pub branch_name: String,
}

impl SelectionDialog {
    /// セッション選択ダイアログを作成
    pub fn new_session_select(sessions: Vec<String>, context: SelectionContext) -> Self {
        Self {
            kind: SelectionDialogKind::SelectSession,
            items: sessions,
            selected_index: 0,
            title: " Select Zellij Session ".to_string(),
            context: Some(context),
        }
    }

    /// レイアウト選択ダイアログを作成
    pub fn new_layout_select(layouts: Vec<String>, context: SelectionContext) -> Self {
        Self {
            kind: SelectionDialogKind::SelectLayout,
            items: layouts,
            selected_index: 0,
            title: " Select Layout ".to_string(),
            context: Some(context),
        }
    }

    /// 選択を上に移動
    pub fn move_up(&mut self) {
        if !self.items.is_empty() && self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    /// 選択を下に移動
    pub fn move_down(&mut self) {
        if !self.items.is_empty() && self.selected_index < self.items.len() - 1 {
            self.selected_index += 1;
        }
    }

    /// 選択中のアイテムを取得
    pub fn selected_item(&self) -> Option<&str> {
        self.items.get(self.selected_index).map(|s| s.as_str())
    }
}

/// 選択ダイアログを描画
pub fn render(frame: &mut Frame, area: Rect, dialog: &SelectionDialog) {
    let popup_area = centered_rect(50, 60, area);
    frame.render_widget(Clear, popup_area);

    let hint = match dialog.kind {
        SelectionDialogKind::SelectSession => "j/k: move | Enter: select | Esc: cancel",
        SelectionDialogKind::SelectLayout => "j/k: move | Enter: select | Esc: cancel",
    };

    let inner_area = popup_area.inner(ratatui::layout::Margin {
        vertical: 1,
        horizontal: 1,
    });

    let chunks = Layout::vertical([
        Constraint::Min(3),    // List
        Constraint::Length(1), // Hint
    ])
    .split(inner_area);

    // リストアイテムを作成
    let list_items: Vec<ListItem> = dialog
        .items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let style = if i == dialog.selected_index {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            let prefix = if i == dialog.selected_index {
                "▶ "
            } else {
                "  "
            };

            ListItem::new(Line::from(vec![
                Span::styled(prefix, style),
                Span::styled(item.clone(), style),
            ]))
        })
        .collect();

    // リストを描画
    let list = List::new(list_items)
        .block(Block::default())
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        );

    let mut list_state = ListState::default();
    list_state.select(Some(dialog.selected_index));
    frame.render_stateful_widget(list, chunks[0], &mut list_state);

    // ヒント
    let hint_widget = Paragraph::new(hint)
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    frame.render_widget(hint_widget, chunks[1]);

    // 外枠
    let block = Block::default()
        .title(dialog.title.as_str())
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    frame.render_widget(block, popup_area);
}

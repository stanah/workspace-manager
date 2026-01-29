use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Style},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use super::centered_rect;

/// 入力ダイアログの種類
#[derive(Debug, Clone)]
pub enum InputDialogKind {
    /// 新規worktree作成（ブランチ名入力）
    CreateWorktree,
    /// worktree削除確認
    DeleteWorktree { path: String, force: bool },
    /// ブランチフィルター
    FilterBranches,
}

/// 入力ダイアログの状態
#[derive(Debug, Clone)]
pub struct InputDialog {
    pub kind: InputDialogKind,
    pub input: String,
    pub cursor_position: usize,
    pub error_message: Option<String>,
}

impl InputDialog {
    pub fn new_create_worktree() -> Self {
        Self {
            kind: InputDialogKind::CreateWorktree,
            input: String::new(),
            cursor_position: 0,
            error_message: None,
        }
    }

    pub fn new_delete_worktree(path: String, force: bool) -> Self {
        Self {
            kind: InputDialogKind::DeleteWorktree { path, force },
            input: String::new(),
            cursor_position: 0,
            error_message: None,
        }
    }

    pub fn new_filter_branches(current_filter: Option<String>) -> Self {
        let input = current_filter.unwrap_or_default();
        let cursor_position = input.len();
        Self {
            kind: InputDialogKind::FilterBranches,
            input,
            cursor_position,
            error_message: None,
        }
    }

    /// 文字を入力
    pub fn insert_char(&mut self, c: char) {
        self.input.insert(self.cursor_position, c);
        self.cursor_position += 1;
        self.error_message = None;
    }

    /// バックスペース
    pub fn backspace(&mut self) {
        if self.cursor_position > 0 {
            self.cursor_position -= 1;
            self.input.remove(self.cursor_position);
            self.error_message = None;
        }
    }

    /// Delete
    pub fn delete(&mut self) {
        if self.cursor_position < self.input.len() {
            self.input.remove(self.cursor_position);
            self.error_message = None;
        }
    }

    /// カーソルを左に移動
    pub fn move_cursor_left(&mut self) {
        if self.cursor_position > 0 {
            self.cursor_position -= 1;
        }
    }

    /// カーソルを右に移動
    pub fn move_cursor_right(&mut self) {
        if self.cursor_position < self.input.len() {
            self.cursor_position += 1;
        }
    }

    /// エラーメッセージを設定
    pub fn set_error(&mut self, message: String) {
        self.error_message = Some(message);
    }
}

/// 入力ダイアログを描画
pub fn render(frame: &mut Frame, area: Rect, dialog: &InputDialog) {
    let popup_area = centered_rect(60, 30, area);
    frame.render_widget(Clear, popup_area);

    let (title, prompt, hint) = match &dialog.kind {
        InputDialogKind::CreateWorktree => (
            " Create Worktree ".to_string(),
            "Branch name:".to_string(),
            "Enter: create | Esc: cancel".to_string(),
        ),
        InputDialogKind::DeleteWorktree { path, force } => (
            if *force { " Force Delete Worktree " } else { " Delete Worktree " }.to_string(),
            format!("{}Delete {}?", if *force { "[FORCE] " } else { "" }, path),
            "y: confirm | n/Esc: cancel".to_string(),
        ),
        InputDialogKind::FilterBranches => (
            " Filter Branches ".to_string(),
            "Filter:".to_string(),
            "Enter: apply | Esc: clear & close".to_string(),
        ),
    };

    let inner_area = popup_area.inner(ratatui::layout::Margin {
        vertical: 1,
        horizontal: 1,
    });

    let chunks = Layout::vertical([
        Constraint::Length(2), // Prompt
        Constraint::Length(3), // Input
        Constraint::Length(1), // Error
        Constraint::Length(1), // Hint
    ])
    .split(inner_area);

    // プロンプト
    let prompt_widget = Paragraph::new(prompt.as_str())
        .style(Style::default().fg(Color::Yellow));
    frame.render_widget(prompt_widget, chunks[0]);

    // 入力フィールド
    let input_style = if dialog.error_message.is_some() {
        Style::default().fg(Color::Red)
    } else {
        Style::default().fg(Color::White)
    };

    let input_widget = Paragraph::new(dialog.input.as_str())
        .style(input_style)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        );
    frame.render_widget(input_widget, chunks[1]);

    // カーソル位置を設定
    frame.set_cursor_position((
        chunks[1].x + dialog.cursor_position as u16 + 1,
        chunks[1].y + 1,
    ));

    // エラーメッセージ
    if let Some(ref error) = dialog.error_message {
        let error_widget = Paragraph::new(error.as_str())
            .style(Style::default().fg(Color::Red));
        frame.render_widget(error_widget, chunks[2]);
    }

    // ヒント
    let hint_widget = Paragraph::new(hint.as_str())
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    frame.render_widget(hint_widget, chunks[3]);

    // 外枠
    let block = Block::default()
        .title(title.as_str())
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    frame.render_widget(block, popup_area);
}

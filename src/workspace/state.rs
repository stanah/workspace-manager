use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// ワークスペースの状態を表すenum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum WorkspaceStatus {
    /// 待機中（Claude Codeがアイドル状態）
    #[default]
    Idle,
    /// 作業中（Claude Codeがタスク実行中）
    Working,
    /// 入力待ち（ユーザーの入力を待っている）
    NeedsInput,
    /// 成功完了
    Success,
    /// エラー発生
    Error,
    /// 接続なし（セッション終了）
    Disconnected,
}

impl WorkspaceStatus {
    /// Parse status from string
    ///
    /// Maps string values to WorkspaceStatus:
    /// - "working" -> Working (blue)
    /// - "idle" -> NeedsInput (yellow, indicates user action needed)
    /// - "success" -> Success (green)
    /// - "error" -> Error (red)
    /// - others -> NeedsInput (yellow, default for unknown states)
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "working" => WorkspaceStatus::Working,
            "idle" => WorkspaceStatus::NeedsInput,
            "success" => WorkspaceStatus::Success,
            "error" => WorkspaceStatus::Error,
            _ => WorkspaceStatus::NeedsInput,
        }
    }

    /// 状態を表すアイコンを返す
    pub fn icon(&self) -> &'static str {
        match self {
            WorkspaceStatus::Idle => "○",
            WorkspaceStatus::Working => "●",
            WorkspaceStatus::NeedsInput => "●",  // 黄色い●（色で区別）
            WorkspaceStatus::Success => "✓",
            WorkspaceStatus::Error => "✗",
            WorkspaceStatus::Disconnected => "◌",
        }
    }

    /// 状態の色を返す（ratatui Color用）
    pub fn color(&self) -> ratatui::style::Color {
        use ratatui::style::Color;
        match self {
            WorkspaceStatus::Idle => Color::Gray,
            WorkspaceStatus::Working => Color::Blue,
            WorkspaceStatus::NeedsInput => Color::Yellow,
            WorkspaceStatus::Success => Color::Green,
            WorkspaceStatus::Error => Color::Red,
            WorkspaceStatus::Disconnected => Color::DarkGray,
        }
    }
}

/// ワークスペース情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    /// 一意識別子
    pub id: Uuid,
    /// セッションID（Claude Codeから受け取る）
    pub session_id: Option<String>,
    /// プロジェクトパス
    pub project_path: String,
    /// リポジトリ名
    pub repo_name: String,
    /// ブランチ名
    pub branch: String,
    /// 現在の状態
    pub status: WorkspaceStatus,
    /// 状態メッセージ
    pub message: Option<String>,
    /// Zellij pane ID
    pub pane_id: Option<u32>,
    /// 最終更新時刻
    pub updated_at: std::time::SystemTime,

    // Rich status from AI analysis
    /// AI解析による作業サマリー（最大50文字）
    #[serde(default)]
    pub ai_summary: Option<String>,
    /// AI解析による現在のタスク
    #[serde(default)]
    pub ai_current_task: Option<String>,
    /// AI解析による詳細ステート
    #[serde(default)]
    pub ai_state_detail: Option<String>,
    /// AI解析による最終アクティビティ時刻
    #[serde(default)]
    pub ai_last_activity: Option<String>,
}

impl Workspace {
    /// 新規ワークスペースを作成
    pub fn new(project_path: String, repo_name: String, branch: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            session_id: None,
            project_path,
            repo_name,
            branch,
            status: WorkspaceStatus::Disconnected,
            message: None,
            pane_id: None,
            updated_at: std::time::SystemTime::now(),
            ai_summary: None,
            ai_current_task: None,
            ai_state_detail: None,
            ai_last_activity: None,
        }
    }

    /// AI解析結果を更新
    pub fn update_from_ai_status(&mut self, status: &crate::logwatch::SessionStatus) {
        // Update summary (truncate to 50 chars)
        self.ai_summary = status.display_summary();

        // Update current task
        self.ai_current_task = status.current_task.clone();

        // Update state detail label
        self.ai_state_detail = Some(status.state_detail.label().to_string());

        // Update last activity time string
        self.ai_last_activity = status.time_since_activity();

        // Also update the basic status based on AI analysis
        self.status = match status.status {
            crate::logwatch::StatusState::Working => WorkspaceStatus::Working,
            crate::logwatch::StatusState::Waiting => WorkspaceStatus::NeedsInput,
            crate::logwatch::StatusState::Completed => WorkspaceStatus::Success,
            crate::logwatch::StatusState::Error => WorkspaceStatus::Error,
            crate::logwatch::StatusState::Idle => WorkspaceStatus::Idle,
            crate::logwatch::StatusState::Disconnected => WorkspaceStatus::Disconnected,
        };

        // Update message with AI summary if available
        if let Some(ref summary) = self.ai_summary {
            self.message = Some(summary.clone());
        }

        self.updated_at = std::time::SystemTime::now();
    }

    /// 状態を更新
    pub fn update_status(&mut self, status: WorkspaceStatus, message: Option<String>) {
        self.status = status;
        self.message = message;
        self.updated_at = std::time::SystemTime::now();
    }

    /// 表示用の短縮パスを返す
    pub fn display_path(&self) -> String {
        if let Some(home) = dirs::home_dir() {
            if let Some(stripped) = self.project_path.strip_prefix(home.to_string_lossy().as_ref())
            {
                return format!("~{}", stripped);
            }
        }
        self.project_path.clone()
    }
}

mod dirs {
    pub fn home_dir() -> Option<std::path::PathBuf> {
        std::env::var_os("HOME").map(std::path::PathBuf::from)
    }
}

//! AI CLI invocation for log analysis
//!
//! Uses Claude Code or Kiro CLI in non-interactive mode to analyze logs
//! and extract structured status information.

use anyhow::{Context, Result};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::time::timeout;
use tracing::{debug, warn};

use super::collector::LogContent;
use super::schema::SessionStatus;

/// Configuration for the log analyzer
#[derive(Debug, Clone)]
pub struct AnalyzerConfig {
    /// Which CLI tool to use for analysis ("claude" or "kiro")
    pub analyzer_tool: String,
    /// Timeout for AI analysis (seconds)
    pub timeout_secs: u64,
    /// Maximum log content length to send (chars)
    pub max_content_length: usize,
}

impl Default for AnalyzerConfig {
    fn default() -> Self {
        Self {
            analyzer_tool: "claude".to_string(),
            timeout_secs: 30,
            max_content_length: 50000,
        }
    }
}

/// Log analyzer that uses AI CLI tools
pub struct LogAnalyzer {
    config: AnalyzerConfig,
}

impl LogAnalyzer {
    /// Create a new analyzer with the given configuration
    pub fn new(config: AnalyzerConfig) -> Self {
        Self { config }
    }

    /// Analyze log content and return structured status
    pub async fn analyze(&self, log: &LogContent) -> Result<SessionStatus> {
        let prompt = self.build_prompt(log);

        debug!(
            "Analyzing log from {} ({} chars)",
            log.source.display(),
            prompt.len()
        );

        let result = timeout(
            Duration::from_secs(self.config.timeout_secs),
            self.invoke_cli(&prompt),
        )
        .await
        .context("Analysis timed out")?
        .context("Failed to invoke CLI")?;

        self.parse_response(&result, log)
    }

    /// Build the analysis prompt
    fn build_prompt(&self, log: &LogContent) -> String {
        let mut content = log.lines.join("\n");

        // Truncate if too long
        if content.len() > self.config.max_content_length {
            let start = content.len() - self.config.max_content_length;
            content = format!("...[truncated]...\n{}", &content[start..]);
        }

        format!(
            r#"Analyze this CLI session log. Output ONLY a JSON object, no other text.

{content}

JSON format: {{"status":"<working|waiting|completed|error|idle|disconnected>","state_detail":"<thinking|executing_tool|writing_code|user_input|confirmation|success|api_error|tool_error|inactive|session_ended>","summary":"<brief 50 char max>"}}

Rules: working+thinking=AI responding, working+executing_tool=tool in progress, waiting+user_input=needs input, completed+success=done, error=failed, disconnected+session_ended=ended"#
        )
    }

    /// JSON schema for structured output
    fn json_schema() -> &'static str {
        r#"{
            "type": "object",
            "properties": {
                "status": {
                    "type": "string",
                    "enum": ["working", "waiting", "completed", "error", "idle", "disconnected"]
                },
                "state_detail": {
                    "type": "string",
                    "enum": ["thinking", "executing_tool", "writing_code", "user_input", "confirmation", "success", "partial", "api_error", "tool_error", "inactive", "session_ended"]
                },
                "summary": {
                    "type": ["string", "null"],
                    "maxLength": 50
                },
                "current_task": {
                    "type": ["string", "null"]
                },
                "error": {
                    "type": ["string", "null"]
                }
            },
            "required": ["status", "state_detail"]
        }"#
    }

    /// Invoke the CLI tool and get the response
    async fn invoke_cli(&self, prompt: &str) -> Result<String> {
        let tool = &self.config.analyzer_tool;

        // Build command based on tool
        let mut cmd = Command::new(tool);
        cmd.arg("--print")
            .arg("-")  // Read from stdin
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // Add model, output format, and json-schema for claude
        if tool == "claude" {
            cmd.arg("--model").arg("haiku");
            cmd.arg("--output-format").arg("json");
            cmd.arg("--json-schema").arg(Self::json_schema());
        }

        debug!("Invoking {} for log analysis", tool);

        let mut child = cmd.spawn().context(format!("Failed to spawn {}", tool))?;

        // Write prompt to stdin
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(prompt.as_bytes())
                .await
                .context("Failed to write to stdin")?;
        }

        let output = child
            .wait_with_output()
            .await
            .context("Failed to wait for command")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!("CLI command failed: {}", stderr);
            anyhow::bail!("CLI exited with status {}: {}", output.status, stderr);
        }

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        Ok(stdout)
    }

    /// Parse the CLI response into SessionStatus
    fn parse_response(&self, response: &str, log: &LogContent) -> Result<SessionStatus> {
        // Handle --output-format json wrapper from claude CLI
        let mut status: SessionStatus = if let Ok(wrapper) = serde_json::from_str::<serde_json::Value>(response) {
            // Try structured_output first (from --json-schema), then result field
            if let Some(structured) = wrapper.get("structured_output") {
                serde_json::from_value(structured.clone())
                    .context("Failed to parse structured_output")?
            } else if let Some(result) = wrapper.get("result").and_then(|r| r.as_str()) {
                let json_str = extract_json(result)?;
                serde_json::from_str(&json_str).context("Failed to parse result JSON")?
            } else {
                anyhow::bail!("No structured_output or result in response")
            }
        } else {
            // Fallback: try to extract JSON directly
            let json_str = extract_json(response)?;
            serde_json::from_str(&json_str).context("Failed to parse JSON response")?
        };

        // Fill in project path if not set
        if status.project_path.is_none() {
            status.project_path = log.project_path.clone();
        }

        // Fill in tool if not set
        if status.tool.is_none() {
            status.tool = Some(log.tool.clone());
        }

        Ok(status)
    }

    /// Check if the analyzer CLI tool is available
    pub async fn is_available(&self) -> bool {
        let result = Command::new(&self.config.analyzer_tool)
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;

        result.map(|s| s.success()).unwrap_or(false)
    }
}

/// Extract JSON from a response that might have extra text
fn extract_json(response: &str) -> Result<String> {
    let trimmed = response.trim();

    // If it starts with {, try to find the matching }
    if trimmed.starts_with('{') {
        // Find the last } that makes valid JSON
        for i in (0..trimmed.len()).rev() {
            if trimmed.as_bytes()[i] == b'}' {
                let candidate = &trimmed[..=i];
                if serde_json::from_str::<serde_json::Value>(candidate).is_ok() {
                    return Ok(candidate.to_string());
                }
            }
        }
    }

    // Try to find JSON in markdown code block
    if let Some(start) = trimmed.find("```json") {
        let content = &trimmed[start + 7..];
        if let Some(end) = content.find("```") {
            return Ok(content[..end].trim().to_string());
        }
    }

    // Try to find JSON between ``` markers
    if let Some(start) = trimmed.find("```") {
        let content = &trimmed[start + 3..];
        if let Some(end) = content.find("```") {
            let inner = content[..end].trim();
            // Skip language identifier if present
            let json_content = if inner.starts_with('{') {
                inner.to_string()
            } else if let Some(nl) = inner.find('\n') {
                inner[nl + 1..].trim().to_string()
            } else {
                inner.to_string()
            };
            if serde_json::from_str::<serde_json::Value>(&json_content).is_ok() {
                return Ok(json_content);
            }
        }
    }

    // Try to find embedded JSON object in text
    if let Some(start) = trimmed.find('{') {
        let substring = &trimmed[start..];
        // Find matching closing brace
        for i in (0..substring.len()).rev() {
            if substring.as_bytes()[i] == b'}' {
                let candidate = &substring[..=i];
                if serde_json::from_str::<serde_json::Value>(candidate).is_ok() {
                    return Ok(candidate.to_string());
                }
            }
        }
    }

    // Last resort: try the whole thing
    if serde_json::from_str::<serde_json::Value>(trimmed).is_ok() {
        return Ok(trimmed.to_string());
    }

    anyhow::bail!("Could not extract valid JSON from response")
}

/// Simple fallback status extractor without AI
/// Used when AI analysis is unavailable or fails
pub fn extract_status_heuristic(log: &LogContent) -> SessionStatus {
    // For JSONL logs (Claude Code), parse the last few entries
    let mut last_type = String::new();
    let mut last_tool = String::new();
    let mut last_text = String::new();
    let mut has_error = false;

    // Parse last N lines as JSON
    for line in log.lines.iter().rev().take(20) {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
            // Check message type
            if let Some(msg) = json.get("message") {
                if let Some(content) = msg.get("content").and_then(|c| c.as_array()) {
                    for item in content {
                        if let Some(t) = item.get("type").and_then(|t| t.as_str()) {
                            if last_type.is_empty() {
                                last_type = t.to_string();
                            }
                            match t {
                                "tool_use" => {
                                    if let Some(name) = item.get("name").and_then(|n| n.as_str()) {
                                        if last_tool.is_empty() {
                                            last_tool = name.to_string();
                                        }
                                    }
                                }
                                "text" => {
                                    if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                                        if last_text.is_empty() && text.len() < 100 {
                                            last_text = text.chars().take(50).collect();
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }

            // Check for errors
            if let Some(t) = json.get("type").and_then(|t| t.as_str()) {
                if t == "error" {
                    has_error = true;
                }
            }
        }
    }

    // Determine status based on parsed content
    let (status, detail, summary) = if has_error {
        (
            super::schema::StatusState::Error,
            super::schema::StatusDetail::ToolError,
            Some("Error occurred".to_string()),
        )
    } else if last_type == "tool_use" {
        let summary = if !last_tool.is_empty() {
            format!("Running: {}", last_tool)
        } else {
            "Executing tool".to_string()
        };
        (
            super::schema::StatusState::Working,
            super::schema::StatusDetail::ExecutingTool,
            Some(summary),
        )
    } else if last_type == "thinking" {
        (
            super::schema::StatusState::Working,
            super::schema::StatusDetail::Thinking,
            Some("Thinking...".to_string()),
        )
    } else if last_type == "text" {
        let summary = if !last_text.is_empty() {
            last_text
        } else {
            "Processing".to_string()
        };
        (
            super::schema::StatusState::Working,
            super::schema::StatusDetail::WritingCode,
            Some(summary),
        )
    } else {
        // Fallback to plain text analysis
        let content = log.lines.join("\n").to_lowercase();
        if content.contains("error") || content.contains("failed") {
            (
                super::schema::StatusState::Error,
                super::schema::StatusDetail::ToolError,
                Some("Error detected".to_string()),
            )
        } else {
            (
                super::schema::StatusState::Idle,
                super::schema::StatusDetail::Inactive,
                None,
            )
        }
    };

    SessionStatus {
        project_path: log.project_path.clone(),
        tool: Some(log.tool.clone()),
        status,
        state_detail: detail,
        summary,
        last_activity: Some(chrono::Utc::now()),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_json_direct() {
        let response = r#"{"status": "working", "state_detail": "thinking"}"#;
        let json = extract_json(response).unwrap();
        assert!(json.contains("working"));
    }

    #[test]
    fn test_extract_json_with_text() {
        let response = r#"Here is the analysis:
{"status": "working", "state_detail": "thinking"}
That's the status."#;
        let json = extract_json(response).unwrap();
        assert!(json.contains("working"));
    }

    #[test]
    fn test_extract_json_markdown() {
        let response = r#"```json
{"status": "completed", "state_detail": "success"}
```"#;
        let json = extract_json(response).unwrap();
        assert!(json.contains("completed"));
    }

    #[test]
    fn test_heuristic_error_detection() {
        // Test plain text error detection (fallback)
        let log = LogContent {
            source: std::path::PathBuf::from("/test"),
            project_path: Some("/project".to_string()),
            tool: "claude".to_string(),
            lines: vec!["Error: command failed".to_string()],
            collected_at: std::time::SystemTime::now(),
        };

        let status = extract_status_heuristic(&log);
        assert_eq!(status.status, super::super::schema::StatusState::Error);
    }

    #[test]
    fn test_heuristic_jsonl_tool_use() {
        // Test JSONL tool_use detection
        let log = LogContent {
            source: std::path::PathBuf::from("/test"),
            project_path: Some("/project".to_string()),
            tool: "claude".to_string(),
            lines: vec![
                r#"{"message":{"content":[{"type":"tool_use","name":"Read","input":{}}]}}"#.to_string()
            ],
            collected_at: std::time::SystemTime::now(),
        };

        let status = extract_status_heuristic(&log);
        assert_eq!(status.status, super::super::schema::StatusState::Working);
        assert_eq!(status.state_detail, super::super::schema::StatusDetail::ExecutingTool);
        assert!(status.summary.unwrap().contains("Read"));
    }
}

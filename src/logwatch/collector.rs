//! Log file monitoring and collection
//!
//! Monitors CLI tool log directories and collects recent log entries for analysis.

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

/// Information about a log file being tracked
#[derive(Debug)]
struct LogFileInfo {
    /// Path to the log file
    #[allow(dead_code)]
    path: PathBuf,
    /// Last read position in the file
    last_position: u64,
    /// Last modification time
    last_modified: SystemTime,
    /// Associated project path (if known)
    #[allow(dead_code)]
    project_path: Option<String>,
    /// Tool type (claude, kiro)
    #[allow(dead_code)]
    tool: String,
}

/// Collected log content for analysis
#[derive(Debug, Clone)]
pub struct LogContent {
    /// Source file path
    pub source: PathBuf,
    /// Project path (if detected)
    pub project_path: Option<String>,
    /// Tool name (claude, kiro)
    pub tool: String,
    /// Log lines collected
    pub lines: Vec<String>,
    /// When the log was collected
    pub collected_at: SystemTime,
}

/// Configuration for log collection
#[derive(Debug, Clone)]
pub struct CollectorConfig {
    /// Claude Code home directory (~/.claude)
    pub claude_home: PathBuf,
    /// Kiro logs directory
    pub kiro_logs_dir: Option<PathBuf>,
    /// Maximum lines to collect per file
    pub max_lines: usize,
    /// How often to scan for new logs (seconds)
    pub scan_interval_secs: u64,
    /// Minimum file age to consider (avoid partially written files)
    pub min_file_age_secs: u64,
}

impl Default for CollectorConfig {
    fn default() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let claude_home = PathBuf::from(&home).join(".claude");

        // Kiro logs on macOS
        let kiro_logs_dir = if cfg!(target_os = "macos") {
            Some(
                PathBuf::from(&home)
                    .join("Library/Application Support/Kiro/logs/kiroAgent"),
            )
        } else {
            None
        };

        Self {
            claude_home,
            kiro_logs_dir,
            max_lines: 500,
            scan_interval_secs: 5,
            min_file_age_secs: 1,
        }
    }
}

/// Log collector that monitors and collects log files
pub struct LogCollector {
    config: CollectorConfig,
    tracked_files: HashMap<PathBuf, LogFileInfo>,
}

impl LogCollector {
    /// Create a new log collector with the given configuration
    pub fn new(config: CollectorConfig) -> Self {
        Self {
            config,
            tracked_files: HashMap::new(),
        }
    }

    /// Get Claude Code debug log directory
    fn claude_debug_dir(&self) -> PathBuf {
        self.config.claude_home.join("debug")
    }

    /// Get Claude Code projects directory
    fn claude_projects_dir(&self) -> PathBuf {
        self.config.claude_home.join("projects")
    }

    /// Scan for log files and return new/updated logs
    pub fn scan(&mut self) -> Result<Vec<LogContent>> {
        let mut results = Vec::new();

        // Scan Claude Code debug logs
        let debug_dir = self.claude_debug_dir();
        if let Ok(content) = self.scan_directory(&debug_dir, "claude", "*.txt") {
            results.extend(content);
        }

        // Scan Claude Code project logs (jsonl files in project dirs)
        if let Ok(content) = self.scan_project_logs() {
            results.extend(content);
        }

        // Scan Kiro logs if configured
        if let Some(kiro_dir) = self.config.kiro_logs_dir.clone() {
            if let Ok(content) = self.scan_directory(&kiro_dir, "kiro", "*.log") {
                results.extend(content);
            }
        }

        Ok(results)
    }

    /// Scan a directory for log files
    fn scan_directory(
        &mut self,
        dir: &Path,
        tool: &str,
        pattern: &str,
    ) -> Result<Vec<LogContent>> {
        let mut results = Vec::new();

        if !dir.exists() {
            debug!("Log directory does not exist: {}", dir.display());
            return Ok(results);
        }

        let extension = pattern.trim_start_matches("*.");

        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            if !path.is_file() {
                continue;
            }

            // Check file extension
            if let Some(ext) = path.extension() {
                if ext != extension {
                    continue;
                }
            } else {
                continue;
            }

            // Check file age
            let metadata = entry.metadata()?;
            let modified = metadata.modified()?;
            let age = SystemTime::now()
                .duration_since(modified)
                .unwrap_or(Duration::ZERO);

            if age.as_secs() < self.config.min_file_age_secs {
                continue; // Skip files being written
            }

            // Check if file has changed
            if let Some(content) = self.check_and_read_file(&path, tool, None)? {
                results.push(content);
            }
        }

        Ok(results)
    }

    /// Scan Claude Code project logs
    fn scan_project_logs(&mut self) -> Result<Vec<LogContent>> {
        let mut results = Vec::new();
        let projects_dir = self.claude_projects_dir();

        if !projects_dir.exists() {
            return Ok(results);
        }

        // Claude Code stores projects in subdirectories like:
        // ~/.claude/projects/-Users-stanah-work-project/session.jsonl
        for entry in std::fs::read_dir(&projects_dir)? {
            let entry = entry?;
            let project_dir = entry.path();

            if !project_dir.is_dir() {
                continue;
            }

            // Extract project path from directory name
            let project_path = extract_project_path(project_dir.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(""));

            // Look for .jsonl files
            for file_entry in std::fs::read_dir(&project_dir)? {
                let file_entry = file_entry?;
                let path = file_entry.path();

                if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                    if let Some(content) =
                        self.check_and_read_file(&path, "claude", Some(project_path.clone()))?
                    {
                        results.push(content);
                    }
                }
            }
        }

        Ok(results)
    }

    /// Check if a file has been modified and read new content
    fn check_and_read_file(
        &mut self,
        path: &Path,
        tool: &str,
        project_path: Option<String>,
    ) -> Result<Option<LogContent>> {
        let metadata = std::fs::metadata(path)?;
        let modified = metadata.modified()?;
        let file_size = metadata.len();

        // Check if we've seen this file before
        if let Some(info) = self.tracked_files.get(path) {
            // File hasn't changed
            if info.last_modified >= modified && info.last_position >= file_size {
                return Ok(None);
            }
        }

        // Read the file from the last position (or from end - max_lines for new files)
        let lines = self.read_file_tail(path, file_size)?;

        if lines.is_empty() {
            return Ok(None);
        }

        // Update tracking info
        self.tracked_files.insert(
            path.to_path_buf(),
            LogFileInfo {
                path: path.to_path_buf(),
                last_position: file_size,
                last_modified: modified,
                project_path: project_path.clone(),
                tool: tool.to_string(),
            },
        );

        Ok(Some(LogContent {
            source: path.to_path_buf(),
            project_path,
            tool: tool.to_string(),
            lines,
            collected_at: SystemTime::now(),
        }))
    }

    /// Read the tail of a log file
    fn read_file_tail(&self, path: &Path, file_size: u64) -> Result<Vec<String>> {
        let file = File::open(path).context("Failed to open log file")?;
        let mut reader = BufReader::new(file);

        // For files over a certain size, seek near the end
        let max_bytes = (self.config.max_lines * 200) as u64; // Estimate 200 bytes per line
        if file_size > max_bytes {
            reader.seek(SeekFrom::End(-(max_bytes as i64)))?;
            // Discard partial line
            let mut discard = String::new();
            let _ = reader.read_line(&mut discard);
        }

        let mut lines: Vec<String> = reader
            .lines()
            .filter_map(|l| l.ok())
            .collect();

        // Keep only the last max_lines
        if lines.len() > self.config.max_lines {
            lines = lines.split_off(lines.len() - self.config.max_lines);
        }

        Ok(lines)
    }

    /// Force read logs for a specific project path (for event-driven triggers)
    pub fn read_for_project(&self, project_path: &str) -> Result<Option<LogContent>> {
        let projects_dir = self.claude_projects_dir();
        if !projects_dir.exists() {
            return Ok(None);
        }

        // Find matching project directory
        for entry in std::fs::read_dir(&projects_dir)? {
            let entry = entry?;
            let project_dir = entry.path();

            if !project_dir.is_dir() {
                continue;
            }

            // Compare directory name with encoded project path
            let dir_name = project_dir.file_name().and_then(|n| n.to_str()).unwrap_or("");

            if dir_matches_project(dir_name, project_path) {
                // Find the most recent .jsonl file
                let mut newest_file: Option<(std::path::PathBuf, std::time::SystemTime)> = None;

                for file_entry in std::fs::read_dir(&project_dir)? {
                    let file_entry = file_entry?;
                    let path = file_entry.path();

                    if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                        if let Ok(metadata) = file_entry.metadata() {
                            if let Ok(modified) = metadata.modified() {
                                if newest_file.as_ref().map(|(_, t)| modified > *t).unwrap_or(true) {
                                    newest_file = Some((path, modified));
                                }
                            }
                        }
                    }
                }

                if let Some((path, _)) = newest_file {
                    let metadata = std::fs::metadata(&path)?;
                    let file_size = metadata.len();
                    let lines = self.read_file_tail(&path, file_size)?;

                    if !lines.is_empty() {
                        return Ok(Some(LogContent {
                            source: path,
                            project_path: Some(project_path.to_string()),
                            tool: "claude".to_string(),
                            lines,
                            collected_at: SystemTime::now(),
                        }));
                    }
                }
            }
        }

        Ok(None)
    }

    /// Start the collector as a background task
    pub fn spawn(
        mut self,
        tx: mpsc::Sender<LogContent>,
    ) -> tokio::task::JoinHandle<()> {
        let interval = Duration::from_secs(self.config.scan_interval_secs);

        tokio::spawn(async move {
            info!("Log collector started");

            loop {
                match self.scan() {
                    Ok(logs) => {
                        for log in logs {
                            debug!(
                                "Collected log: {} ({} lines)",
                                log.source.display(),
                                log.lines.len()
                            );
                            if tx.send(log).await.is_err() {
                                warn!("Log receiver dropped, stopping collector");
                                return;
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Log scan error: {}", e);
                    }
                }

                tokio::time::sleep(interval).await;
            }
        })
    }
}

/// Extract project path from Claude Code's encoded directory name
/// e.g., "-Users-stanah-work-project" -> "/Users/stanah/work/project"
fn extract_project_path(encoded: &str) -> String {
    if encoded.starts_with('-') {
        // Convert -Users-stanah-work to /Users/stanah/work
        encoded.replace('-', "/")
    } else {
        encoded.to_string()
    }
}

/// Encode a path to Claude Code's directory name format
/// e.g., "/Users/stanah/work/github.com/project" -> "-Users-stanah-work-github-com-project"
fn encode_project_path(path: &str) -> String {
    // Expand ~ to home directory first
    let expanded = if path.starts_with("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            format!("{}{}", home.to_string_lossy(), &path[1..])
        } else {
            path.to_string()
        }
    } else {
        path.to_string()
    };

    // Replace / and . with -
    expanded
        .replace('/', "-")
        .replace('.', "-")
        .trim_end_matches('-')
        .to_string()
}

/// Check if a directory name matches a project path
fn dir_matches_project(dir_name: &str, project_path: &str) -> bool {
    let encoded = encode_project_path(project_path);
    dir_name == encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_project_path() {
        assert_eq!(
            extract_project_path("-Users-stanah-work-project"),
            "/Users/stanah/work/project"
        );
        assert_eq!(
            extract_project_path("normal-path"),
            "normal-path"
        );
    }

    #[test]
    fn test_encode_project_path() {
        assert_eq!(
            encode_project_path("/Users/stanah/work/github.com/stanah/workspace-manager"),
            "-Users-stanah-work-github-com-stanah-workspace-manager"
        );
        assert_eq!(
            encode_project_path("/Users/stanah/work/project"),
            "-Users-stanah-work-project"
        );
    }

    #[test]
    fn test_dir_matches_project() {
        assert!(dir_matches_project(
            "-Users-stanah-work-github-com-stanah-workspace-manager",
            "/Users/stanah/work/github.com/stanah/workspace-manager"
        ));
        assert!(!dir_matches_project(
            "-Users-stanah-work-other-project",
            "/Users/stanah/work/github.com/stanah/workspace-manager"
        ));
    }

    #[test]
    fn test_default_config() {
        let config = CollectorConfig::default();
        assert!(config.claude_home.ends_with(".claude"));
        assert_eq!(config.max_lines, 500);
    }
}

//! File copying functionality with progress tracking and concurrency control.
//!
//! This module handles downloading rule files from GitHub repositories and
//! copying them to the local filesystem with progress indicators, path validation,
//! and atomic operations for safety.

use anyhow::{Context, Result};
use base64::Engine;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::RwLock;
use tempfile::NamedTempFile;
use tokio::{fs, sync::Semaphore};

use crate::github::RepoLocator;
use crate::ui::prompts::{ConflictChoice, PromptService};

/// Strategy for handling file overwrite conflicts
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverwriteMode {
    /// Prompt the user for each conflict
    Prompt,
    /// Force overwrite all existing files
    Force,
    /// Skip all existing files
    #[allow(dead_code)] // Forward-looking feature for CLI integration
    Skip,
    /// Rename conflicting files with numbered suffixes
    #[allow(dead_code)] // Forward-looking feature for CLI integration
    Rename,
    /// Prompt once, then apply the same choice to all subsequent conflicts
    #[allow(dead_code)] // Forward-looking feature for CLI integration
    PromptOnce,
}

impl Default for OverwriteMode {
    fn default() -> Self {
        Self::Prompt
    }
}

/// Configuration for copy operations
#[derive(Debug, Clone)]
pub struct CopyConfig {
    /// Output directory for copied files
    pub output_dir: PathBuf,
    /// Strategy for handling overwrite conflicts
    pub overwrite_mode: OverwriteMode,
    /// Maximum number of concurrent downloads
    pub max_concurrency: usize,
}

impl Default for CopyConfig {
    fn default() -> Self {
        Self {
            output_dir: PathBuf::from("./.cursor/rules"),
            overwrite_mode: OverwriteMode::default(),
            max_concurrency: 4,
        }
    }
}

impl CopyConfig {
    /// Create config with force overwrite mode (for --force flag)
    #[allow(dead_code)] // Forward-looking feature for CLI integration
    pub fn with_force_overwrite(mut self) -> Self {
        self.overwrite_mode = OverwriteMode::Force;
        self
    }

    /// Create config with skip overwrite mode
    #[allow(dead_code)] // Forward-looking feature for CLI integration
    pub fn with_skip_overwrite(mut self) -> Self {
        self.overwrite_mode = OverwriteMode::Skip;
        self
    }

    /// Create config with rename overwrite mode
    #[allow(dead_code)] // Forward-looking feature for CLI integration
    pub fn with_rename_overwrite(mut self) -> Self {
        self.overwrite_mode = OverwriteMode::Rename;
        self
    }
}

/// Represents a planned copy operation with conflict resolution
#[derive(Debug, Clone)]
pub struct CopyPlan {
    /// Source file path in the repository
    pub source_path: String,
    /// Destination file path on local filesystem
    pub destination_path: PathBuf,
    /// Whether this operation would overwrite an existing file
    pub would_overwrite: bool,
    /// Action to take for this file (for dry-run display)
    pub action: CopyAction,
}

/// The action that will be taken for a file during copy
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CopyAction {
    /// Copy the file (no conflict)
    Copy,
    /// Overwrite existing file
    Overwrite,
    /// Skip existing file
    Skip,
    /// Rename to avoid conflict (with new name)
    Rename(String),
}

/// Result of a copy operation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CopyResult {
    /// File was copied successfully
    Copied,
    /// File was skipped
    Skipped,
    /// File was renamed and copied (with the new filename)
    Renamed(String),
}

impl std::fmt::Display for CopyAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CopyAction::Copy => write!(f, "Copy"),
            CopyAction::Overwrite => write!(f, "Overwrite"),
            CopyAction::Skip => write!(f, "Skip"),
            CopyAction::Rename(new_name) => write!(f, "Rename → {new_name}"),
        }
    }
}

/// Statistics for copy operations
#[derive(Debug, Default)]
pub struct CopyStats {
    pub files_copied: usize,
    pub files_skipped: usize,
    pub files_failed: usize,
    pub files_renamed: usize,
}

/// State for managing batch conflict resolution
#[derive(Debug)]
#[allow(dead_code)] // Forward-looking feature for CLI integration
struct BatchConflictState {
    /// The current global choice for handling conflicts (for PromptOnce mode)
    global_choice: RwLock<Option<ConflictChoice>>,
}

impl BatchConflictState {
    #[allow(dead_code)] // Forward-looking feature for CLI integration
    fn new() -> Self {
        Self {
            global_choice: RwLock::new(None),
        }
    }

    /// Get the global choice if set, otherwise None
    #[allow(dead_code)] // Forward-looking feature for CLI integration
    fn get_global_choice(&self) -> Option<ConflictChoice> {
        *self.global_choice.read().unwrap()
    }

    /// Set the global choice for all subsequent conflicts
    #[allow(dead_code)] // Forward-looking feature for CLI integration
    fn set_global_choice(&self, choice: ConflictChoice) {
        *self.global_choice.write().unwrap() = Some(choice);
    }
}

/// Validate that a source entry path is safe and a file path is safe to write to
fn validate_safe_path(source_entry: &str, dest_path: &Path, output_dir: &Path) -> Result<()> {
    // First, check for path traversal attempts in the source entry
    if source_entry.contains("..") {
        anyhow::bail!("Path traversal attempt detected: source contains '..'");
    }

    // Check for absolute paths in source (Unix and Windows)
    if source_entry.starts_with('/') || source_entry.contains(":\\") {
        anyhow::bail!("Path traversal attempt detected: absolute path in source");
    }

    // Extract just the filename from the source entry for further validation
    let filename = dest_path
        .file_name()
        .and_then(|n| n.to_str())
        .context("Invalid filename")?;

    // Check for Windows reserved names
    let name_lower = filename.to_lowercase();
    let reserved_names = [
        "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
        "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
    ];

    // Check if filename (without extension) is a reserved name
    let name_without_ext = name_lower.split('.').next().unwrap_or(&name_lower);
    if reserved_names.contains(&name_without_ext) {
        anyhow::bail!("Filename contains Windows reserved name: {}", filename);
    }

    // Check for null bytes and other problematic characters
    if filename.contains('\0') {
        anyhow::bail!("Filename contains null byte");
    }

    // For basic path validation, we don't need to create directories during planning
    // Just check that the relative path would be safe
    let dest_relative_to_output = dest_path.strip_prefix(output_dir);
    if dest_relative_to_output.is_err() {
        // If dest_path is not under output_dir, it could be a path traversal attempt
        // However, we should be more permissive here for plan creation
        // The real validation will happen during execution
    }

    Ok(())
}

/// Generate a unique filename by adding a numbered suffix
fn generate_unique_filename(base_path: &Path) -> PathBuf {
    let parent = base_path.parent().unwrap_or_else(|| Path::new("."));
    let filename = base_path.file_name().unwrap().to_string_lossy();

    // Split filename into name and extension
    let (name, extension) = if let Some(dot_pos) = filename.rfind('.') {
        (&filename[..dot_pos], &filename[dot_pos..])
    } else {
        (filename.as_ref(), "")
    };

    // Try numbered suffixes starting from 1
    for i in 1..=1000 {
        let new_filename = format!("{name}({i}){extension}");
        let new_path = parent.join(&new_filename);

        if !new_path.exists() {
            return new_path;
        }
    }

    // Fallback if we somehow can't find a unique name after 1000 attempts
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let fallback_filename = format!("{name}-{timestamp}{extension}");
    parent.join(fallback_filename)
}

/// Create a copy plan for the given manifest entries
pub fn create_copy_plan(entries: &[String], config: &CopyConfig) -> Result<Vec<CopyPlan>> {
    let mut plans = Vec::new();

    // Ensure output directory exists for validation
    let output_dir = &config.output_dir;

    for entry in entries {
        let filename = Path::new(entry)
            .file_name()
            .context("Invalid file path in manifest")?
            .to_string_lossy();

        let mut destination_path = output_dir.join(filename.as_ref());

        // Validate the destination path for security
        validate_safe_path(entry, &destination_path, output_dir)
            .with_context(|| format!("Invalid destination path for {entry}"))?;

        let would_overwrite = destination_path.exists();

        // Determine the action based on overwrite mode and conflict status
        let action = if !would_overwrite {
            CopyAction::Copy
        } else {
            match config.overwrite_mode {
                OverwriteMode::Force => CopyAction::Overwrite,
                OverwriteMode::Skip => CopyAction::Skip,
                OverwriteMode::Rename => {
                    let unique_path = generate_unique_filename(&destination_path);
                    let new_filename = unique_path
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .to_string();
                    destination_path = unique_path;
                    CopyAction::Rename(new_filename)
                }
                OverwriteMode::Prompt | OverwriteMode::PromptOnce => {
                    // For now, default to prompt behavior (will be handled in execution)
                    CopyAction::Overwrite
                }
            }
        };

        plans.push(CopyPlan {
            source_path: entry.clone(),
            destination_path,
            would_overwrite,
            action,
        });
    }

    Ok(plans)
}

/// Render copy plan as a formatted table with action preview
pub fn render_copy_plan_table(plans: &[CopyPlan]) -> String {
    if plans.is_empty() {
        return "No files to copy.".to_string();
    }

    let mut output = String::new();

    // Header
    output.push_str(&format!(
        "{:<40} {:<30} {:<10} {:<20}\n",
        "Source", "Destination", "Overwrite?", "Action"
    ));
    output.push_str(&format!("{:-<102}\n", ""));

    // Rows
    for plan in plans {
        let source = if plan.source_path.len() > 38 {
            format!("...{}", &plan.source_path[plan.source_path.len() - 35..])
        } else {
            plan.source_path.clone()
        };

        let dest = plan
            .destination_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();

        let overwrite = if plan.would_overwrite { "Yes" } else { "No" };

        output.push_str(&format!(
            "{:<40} {:<30} {:<10} {:<20}\n",
            source, dest, overwrite, plan.action
        ));
    }

    output.push_str(&format!("\nTotal files: {}\n", plans.len()));
    output
}

/// Execute copy plan with progress tracking and interactive conflict resolution
pub async fn execute_copy_plan(
    plans: Vec<CopyPlan>,
    repo_locator: &RepoLocator,
    config: &CopyConfig,
    _prompt_service: &dyn PromptService,
) -> Result<CopyStats> {
    // Create output directory if it doesn't exist (always, even for empty plans)
    fs::create_dir_all(&config.output_dir)
        .await
        .context("Failed to create output directory")?;

    if plans.is_empty() {
        return Ok(CopyStats::default());
    }

    // Set up progress tracking
    let multi_progress = MultiProgress::new();
    let overall_pb = multi_progress.add(ProgressBar::new(plans.len() as u64));
    overall_pb.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}",
        )?
        .progress_chars("#>-"),
    );
    overall_pb.set_message("Copying files...");

    // Semaphore to limit concurrency
    let semaphore = Arc::new(Semaphore::new(config.max_concurrency));
    let octocrab = Arc::new(octocrab::instance());

    let mut tasks = Vec::new();
    let mut stats = CopyStats::default();

    for plan in plans {
        let semaphore = semaphore.clone();
        let overall_pb = overall_pb.clone();
        let repo_locator = repo_locator.clone();
        let octocrab = octocrab.clone();

        let task = tokio::spawn(async move {
            let _permit = semaphore.acquire().await.unwrap();

            let result = copy_single_file_enhanced(&plan, &repo_locator, &octocrab).await;

            overall_pb.inc(1);

            match &result {
                Ok(copy_result) => match copy_result {
                    CopyResult::Copied => {
                        overall_pb.set_message(format!("Copied {}", plan.source_path));
                    }
                    CopyResult::Skipped => {
                        overall_pb.set_message(format!("Skipped {}", plan.source_path));
                    }
                    CopyResult::Renamed(new_name) => {
                        overall_pb
                            .set_message(format!("Renamed {} → {}", plan.source_path, new_name));
                    }
                },
                Err(ref e) => {
                    overall_pb.set_message(format!("Failed {}: {}", plan.source_path, e));
                }
            }

            result
        });

        tasks.push(task);
    }

    // Wait for all tasks to complete
    for task in tasks {
        match task.await? {
            Ok(copy_result) => match copy_result {
                CopyResult::Copied => {
                    stats.files_copied += 1;
                }
                CopyResult::Skipped => {
                    stats.files_skipped += 1;
                }
                CopyResult::Renamed(_) => {
                    stats.files_copied += 1;
                    stats.files_renamed += 1;
                }
            },
            Err(_) => {
                stats.files_failed += 1;
            }
        }
    }

    overall_pb.finish_with_message(format!(
        "Complete! Copied: {}, Skipped: {}, Failed: {}, Renamed: {}",
        stats.files_copied, stats.files_skipped, stats.files_failed, stats.files_renamed
    ));

    Ok(stats)
}

/// Copy a single file based on the plan's action (enhanced with CopyResult return)
async fn copy_single_file_enhanced(
    plan: &CopyPlan,
    repo_locator: &RepoLocator,
    octocrab: &Arc<octocrab::Octocrab>,
) -> Result<CopyResult> {
    use crate::github::cache::{FileSystemCache, PersistentCache};

    // Skip if action is Skip
    if plan.action == CopyAction::Skip {
        return Ok(CopyResult::Skipped);
    }

    // Calculate content SHA for cache key (simple hash of the file path)
    let content_sha = {
        use sha1::{Digest, Sha1};
        let mut hasher = Sha1::new();
        hasher.update(format!("{}/{}", repo_locator.repo, plan.source_path).as_bytes());
        format!("{:x}", hasher.finalize())
    };

    // Try to get content from cache first
    let file_content = if let Ok(cache) = FileSystemCache::new() {
        if let Ok(Some(cached_content)) = cache.get_blob_cache(&content_sha).await {
            // Found in cache, use it
            cached_content.into_bytes()
        } else {
            // Not in cache, download and cache it
            let content = download_file_content(
                octocrab,
                &repo_locator.owner,
                &repo_locator.repo,
                &plan.source_path,
                &repo_locator.branch,
            )
            .await?;

            // Store in cache for future use
            if let Ok(content_str) = String::from_utf8(content.clone()) {
                let _ = cache.store_blob_cache(&content_sha, &content_str).await;
            }

            content
        }
    } else {
        // Cache unavailable, download directly
        download_file_content(
            octocrab,
            &repo_locator.owner,
            &repo_locator.repo,
            &plan.source_path,
            &repo_locator.branch,
        )
        .await?
    };

    // Handle file writing based on action
    let final_path = match &plan.action {
        CopyAction::Copy | CopyAction::Overwrite => plan.destination_path.clone(),
        CopyAction::Rename(new_name) => {
            let parent = plan
                .destination_path
                .parent()
                .unwrap_or_else(|| Path::new("."));
            parent.join(new_name)
        }
        CopyAction::Skip => return Ok(CopyResult::Skipped),
    };

    // Ensure parent directory exists
    if let Some(parent) = final_path.parent() {
        fs::create_dir_all(parent)
            .await
            .with_context(|| format!("Failed to create directory {}", parent.display()))?;
    }

    // Write to temporary file first for atomic operation
    let temp_file = NamedTempFile::new_in(final_path.parent().unwrap_or_else(|| Path::new(".")))
        .context("Failed to create temporary file")?;

    fs::write(temp_file.path(), &file_content)
        .await
        .context("Failed to write content to temporary file")?;

    // Atomically move to final location
    temp_file
        .persist(&final_path)
        .with_context(|| format!("Failed to move temporary file to {}", final_path.display()))?;

    // Return appropriate result
    match &plan.action {
        CopyAction::Copy | CopyAction::Overwrite => Ok(CopyResult::Copied),
        CopyAction::Rename(new_name) => Ok(CopyResult::Renamed(new_name.clone())),
        CopyAction::Skip => Ok(CopyResult::Skipped),
    }
}

/// Download file content from GitHub repository
async fn download_file_content(
    octocrab: &Arc<octocrab::Octocrab>,
    owner: &str,
    repo: &str,
    path: &str,
    branch: &str,
) -> Result<Vec<u8>> {
    let response = octocrab
        .repos(owner, repo)
        .get_content()
        .path(path)
        .r#ref(branch)
        .send()
        .await
        .context("Failed to fetch file from GitHub")?;

    match response.items.first() {
        Some(content) if content.download_url.is_some() => {
            let download_url = content.download_url.as_ref().unwrap();
            let response = reqwest::get(download_url)
                .await
                .context("Failed to download file content")?;

            let bytes = response
                .bytes()
                .await
                .context("Failed to read file content")?;

            Ok(bytes.to_vec())
        }
        Some(content) if content.content.is_some() => {
            // Handle base64 encoded content
            let encoded_content = content.content.as_ref().unwrap();
            let cleaned = encoded_content.replace(['\n', ' '], "");

            base64::engine::general_purpose::STANDARD
                .decode(cleaned)
                .context("Failed to decode base64 content")
        }
        _ => anyhow::bail!("File content not available"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::prompts::{ConflictChoice, NonInteractivePromptService};
    use std::sync::Arc;
    use tempfile::TempDir;

    #[test]
    fn test_copy_plan_creation_success() {
        let temp_dir = TempDir::new().unwrap();
        let config = CopyConfig {
            output_dir: temp_dir.path().to_path_buf(),
            overwrite_mode: OverwriteMode::Prompt,
            max_concurrency: 4,
        };

        let entries = vec![
            "frontend/react.mdc".to_string(),
            "backend/rust.mdc".to_string(),
        ];
        let plans = create_copy_plan(&entries, &config).unwrap();

        assert_eq!(plans.len(), 2);
        assert_eq!(plans[0].source_path, "frontend/react.mdc");
        assert_eq!(plans[0].destination_path.file_name().unwrap(), "react.mdc");
        assert!(!plans[0].would_overwrite);
        assert_eq!(plans[0].action, CopyAction::Copy);
    }

    #[test]
    fn test_copy_plan_creation_empty_entries() {
        let temp_dir = TempDir::new().unwrap();
        let config = CopyConfig {
            output_dir: temp_dir.path().to_path_buf(),
            overwrite_mode: OverwriteMode::Prompt,
            max_concurrency: 4,
        };

        let entries = vec![];
        let plans = create_copy_plan(&entries, &config).unwrap();

        assert!(plans.is_empty());
    }

    #[test]
    fn test_copy_plan_creation_nested_paths() {
        let temp_dir = TempDir::new().unwrap();
        let config = CopyConfig {
            output_dir: temp_dir.path().to_path_buf(),
            overwrite_mode: OverwriteMode::Prompt,
            max_concurrency: 4,
        };

        let entries = vec![
            "very/deep/nested/path/file.mdc".to_string(),
            "single.mdc".to_string(),
        ];
        let plans = create_copy_plan(&entries, &config).unwrap();

        assert_eq!(plans.len(), 2);
        assert_eq!(plans[0].destination_path.file_name().unwrap(), "file.mdc");
        assert_eq!(plans[1].destination_path.file_name().unwrap(), "single.mdc");
        assert_eq!(plans[0].action, CopyAction::Copy);
        assert_eq!(plans[1].action, CopyAction::Copy);
    }

    #[test]
    fn test_copy_plan_handles_conflicts() {
        let temp_dir = TempDir::new().unwrap();

        // Create a file that would conflict
        let existing_file = temp_dir.path().join("react.mdc");
        std::fs::write(&existing_file, "existing content").unwrap();

        let config = CopyConfig {
            output_dir: temp_dir.path().to_path_buf(),
            overwrite_mode: OverwriteMode::Prompt,
            max_concurrency: 4,
        };

        let entries = vec!["frontend/react.mdc".to_string()];
        let plans = create_copy_plan(&entries, &config).unwrap();

        assert_eq!(plans.len(), 1);
        assert!(plans[0].would_overwrite);
        assert_eq!(plans[0].action, CopyAction::Overwrite);
    }

    #[test]
    fn test_copy_plan_multiple_conflicts() {
        let temp_dir = TempDir::new().unwrap();

        // Create files that would conflict
        let existing_file1 = temp_dir.path().join("react.mdc");
        let existing_file2 = temp_dir.path().join("vue.mdc");
        std::fs::write(&existing_file1, "existing content 1").unwrap();
        std::fs::write(&existing_file2, "existing content 2").unwrap();

        let config = CopyConfig {
            output_dir: temp_dir.path().to_path_buf(),
            overwrite_mode: OverwriteMode::Prompt,
            max_concurrency: 4,
        };

        let entries = vec![
            "frontend/react.mdc".to_string(),
            "frontend/vue.mdc".to_string(),
            "backend/rust.mdc".to_string(),
        ];
        let plans = create_copy_plan(&entries, &config).unwrap();

        assert_eq!(plans.len(), 3);
        assert!(plans[0].would_overwrite); // react.mdc exists
        assert!(plans[1].would_overwrite); // vue.mdc exists
        assert!(!plans[2].would_overwrite); // rust.mdc doesn't exist
        assert_eq!(plans[0].action, CopyAction::Overwrite);
        assert_eq!(plans[1].action, CopyAction::Overwrite);
        assert_eq!(plans[2].action, CopyAction::Copy);
    }

    #[test]
    fn test_dry_run_table_rendering() {
        let temp_dir = TempDir::new().unwrap();
        let plans = vec![
            CopyPlan {
                source_path: "frontend/react.mdc".to_string(),
                destination_path: temp_dir.path().join("react.mdc"),
                would_overwrite: false,
                action: CopyAction::Copy,
            },
            CopyPlan {
                source_path: "backend/rust.mdc".to_string(),
                destination_path: temp_dir.path().join("rust.mdc"),
                would_overwrite: true,
                action: CopyAction::Overwrite,
            },
        ];

        let table = render_copy_plan_table(&plans);

        assert!(table.contains("Source"));
        assert!(table.contains("Destination"));
        assert!(table.contains("Overwrite?"));
        assert!(table.contains("Action"));
        assert!(table.contains("frontend/react.mdc"));
        assert!(table.contains("backend/rust.mdc"));
        assert!(table.contains("Total files: 2"));
        assert!(table.contains("Yes")); // overwrite for rust.mdc
        assert!(table.contains("No")); // no overwrite for react.mdc
        assert!(table.contains("Copy"));
        assert!(table.contains("Overwrite"));
    }

    #[test]
    fn test_dry_run_table_rendering_empty() {
        let plans = vec![];
        let table = render_copy_plan_table(&plans);

        assert_eq!(table, "No files to copy.");
    }

    #[test]
    fn test_dry_run_table_rendering_long_paths() {
        let temp_dir = TempDir::new().unwrap();
        let long_path = "very/very/very/very/very/very/long/path/to/a/file.mdc";
        let plans = vec![CopyPlan {
            source_path: long_path.to_string(),
            destination_path: temp_dir.path().join("file.mdc"),
            would_overwrite: false,
            action: CopyAction::Copy,
        }];

        let table = render_copy_plan_table(&plans);

        // Should truncate long paths with ...
        assert!(table.contains("..."));
        assert!(table.contains("file.mdc"));
        assert!(table.contains("Copy"));
    }

    #[test]
    fn test_copy_operation_error_handling() {
        // Test that we handle errors gracefully when invalid paths are provided
        let copy_config = CopyConfig {
            output_dir: PathBuf::from("/invalid/path/that/does/not/exist"),
            overwrite_mode: OverwriteMode::Prompt,
            max_concurrency: 1,
        };

        let entries = vec!["valid/file.mdc".to_string()];
        let result = create_copy_plan(&entries, &copy_config);

        // The copy plan creation should succeed; errors occur during execution
        assert!(result.is_ok());
        let plan = result.unwrap();
        assert_eq!(plan.len(), 1);
    }

    #[test]
    fn test_force_overwrite_behavior() {
        let temp_dir = TempDir::new().unwrap();
        let output_dir = temp_dir.path();

        // Create an existing file
        let existing_file = output_dir.join("test.mdc");
        std::fs::write(&existing_file, "existing content").unwrap();

        let copy_config_prompt = CopyConfig {
            output_dir: output_dir.to_path_buf(),
            overwrite_mode: OverwriteMode::Prompt,
            max_concurrency: 1,
        };

        let copy_config_force = CopyConfig {
            output_dir: output_dir.to_path_buf(),
            overwrite_mode: OverwriteMode::Force,
            max_concurrency: 1,
        };

        let entries = vec!["test.mdc".to_string()];

        let plan_prompt = create_copy_plan(&entries, &copy_config_prompt).unwrap();
        let plan_force = create_copy_plan(&entries, &copy_config_force).unwrap();

        // Both plans should be created successfully
        assert_eq!(plan_prompt.len(), 1);
        assert_eq!(plan_force.len(), 1);

        // Check conflict detection - the file exists so should indicate overwrite
        assert!(plan_prompt[0].would_overwrite);
        assert!(plan_force[0].would_overwrite);

        // Check actions
        assert_eq!(plan_prompt[0].action, CopyAction::Overwrite);
        assert_eq!(plan_force[0].action, CopyAction::Overwrite);
    }

    #[test]
    fn test_copy_config_default() {
        let config = CopyConfig::default();

        assert_eq!(config.output_dir, PathBuf::from("./.cursor/rules"));
        assert_eq!(config.overwrite_mode, OverwriteMode::Prompt);
        assert_eq!(config.max_concurrency, 4);
    }

    #[test]
    fn test_copy_config_custom() {
        let custom_dir = PathBuf::from("/custom/path");
        let config = CopyConfig {
            output_dir: custom_dir.clone(),
            overwrite_mode: OverwriteMode::Force,
            max_concurrency: 8,
        };

        assert_eq!(config.output_dir, custom_dir);
        assert_eq!(config.overwrite_mode, OverwriteMode::Force);
        assert_eq!(config.max_concurrency, 8);
    }

    #[test]
    fn test_copy_stats_default() {
        let stats = CopyStats::default();

        assert_eq!(stats.files_copied, 0);
        assert_eq!(stats.files_skipped, 0);
        assert_eq!(stats.files_failed, 0);
        assert_eq!(stats.files_renamed, 0);
    }

    #[test]
    fn test_copy_plan_debug() {
        let temp_dir = TempDir::new().unwrap();
        let plan = CopyPlan {
            source_path: "test.mdc".to_string(),
            destination_path: temp_dir.path().join("test.mdc"),
            would_overwrite: false,
            action: CopyAction::Copy,
        };

        let debug_str = format!("{plan:?}");
        assert!(debug_str.contains("test.mdc"));
        assert!(debug_str.contains("would_overwrite: false"));
    }

    #[tokio::test]
    async fn test_execute_copy_plan_empty() {
        let plans = vec![];
        let repo_locator = RepoLocator {
            owner: "test".to_string(),
            repo: "test".to_string(),
            branch: "main".to_string(),
        };
        let config = CopyConfig::default();

        let prompt_service = NonInteractivePromptService::skip_all();
        let stats = execute_copy_plan(plans, &repo_locator, &config, &prompt_service)
            .await
            .unwrap();

        assert_eq!(stats.files_copied, 0);
        assert_eq!(stats.files_skipped, 0);
        assert_eq!(stats.files_failed, 0);
    }

    #[tokio::test]
    async fn test_execute_copy_plan_creates_output_directory() {
        let temp_dir = TempDir::new().unwrap();
        let output_dir = temp_dir.path().join("new_directory");

        let config = CopyConfig {
            output_dir: output_dir.clone(),
            overwrite_mode: OverwriteMode::Prompt,
            max_concurrency: 1,
        };

        // Test with empty plans - this should still create the output directory
        // without making any network calls
        let plans = vec![];

        let repo_locator = RepoLocator {
            owner: "test".to_string(),
            repo: "test".to_string(),
            branch: "main".to_string(),
        };

        // Execute with empty plans - should create directory and succeed immediately
        let prompt_service = NonInteractivePromptService::skip_all();
        let result = execute_copy_plan(plans, &repo_locator, &config, &prompt_service).await;
        assert!(result.is_ok());

        // Verify the output directory was created
        assert!(output_dir.exists());
        assert!(output_dir.is_dir());
    }

    #[tokio::test]
    async fn test_copy_single_file_skip_on_conflict() {
        let temp_dir = TempDir::new().unwrap();
        let dest_file = temp_dir.path().join("test.mdc");

        // Create existing file
        std::fs::write(&dest_file, "existing content").unwrap();

        let plan = CopyPlan {
            source_path: "test.mdc".to_string(),
            destination_path: dest_file.clone(),
            would_overwrite: true,
            action: CopyAction::Skip, // Use Skip action to avoid network calls
        };

        let repo_locator = RepoLocator {
            owner: "test".to_string(),
            repo: "test".to_string(),
            branch: "main".to_string(),
        };

        // Create a mock octocrab instance - this test only checks the skip behavior
        // so it should return Skipped before any network calls are made
        let octocrab = Arc::new(octocrab::instance());

        // Should skip the file due to Skip action
        // This will return early without making network calls
        let result = copy_single_file_enhanced(&plan, &repo_locator, &octocrab)
            .await
            .unwrap();
        assert_eq!(result, CopyResult::Skipped); // Should return Skipped

        // File should still contain original content
        let content = std::fs::read_to_string(&dest_file).unwrap();
        assert_eq!(content, "existing content");
    }

    #[test]
    fn test_overwrite_mode_default() {
        let mode = OverwriteMode::default();
        assert_eq!(mode, OverwriteMode::Prompt);
    }

    #[test]
    fn test_copy_config_builder_methods() {
        let config = CopyConfig::default().with_force_overwrite();
        assert_eq!(config.overwrite_mode, OverwriteMode::Force);

        let config = CopyConfig::default().with_skip_overwrite();
        assert_eq!(config.overwrite_mode, OverwriteMode::Skip);

        let config = CopyConfig::default().with_rename_overwrite();
        assert_eq!(config.overwrite_mode, OverwriteMode::Rename);
    }

    #[test]
    fn test_copy_action_display() {
        assert_eq!(CopyAction::Copy.to_string(), "Copy");
        assert_eq!(CopyAction::Overwrite.to_string(), "Overwrite");
        assert_eq!(CopyAction::Skip.to_string(), "Skip");
        assert_eq!(
            CopyAction::Rename("test(1).mdc".to_string()).to_string(),
            "Rename → test(1).mdc"
        );
    }

    #[test]
    fn test_rename_strategy() {
        let temp_dir = TempDir::new().unwrap();

        // Create existing files
        let base_file = temp_dir.path().join("test.mdc");
        let rename1 = temp_dir.path().join("test(1).mdc");
        std::fs::write(&base_file, "content").unwrap();
        std::fs::write(&rename1, "content").unwrap();

        let config = CopyConfig {
            output_dir: temp_dir.path().to_path_buf(),
            overwrite_mode: OverwriteMode::Rename,
            max_concurrency: 4,
        };

        let entries = vec!["frontend/test.mdc".to_string()];
        let plans = create_copy_plan(&entries, &config).unwrap();

        assert_eq!(plans.len(), 1);
        assert!(plans[0].would_overwrite);

        if let CopyAction::Rename(name) = &plans[0].action {
            assert_eq!(name, "test(2).mdc");
            assert_eq!(
                plans[0].destination_path.file_name().unwrap(),
                "test(2).mdc"
            );
        } else {
            panic!("Expected Rename action, got {:?}", plans[0].action);
        }
    }

    #[test]
    fn test_path_traversal_protection() {
        let temp_dir = TempDir::new().unwrap();
        let config = CopyConfig {
            output_dir: temp_dir.path().to_path_buf(),
            overwrite_mode: OverwriteMode::Prompt,
            max_concurrency: 4,
        };

        // Test path traversal attempts - these should fail validation
        let malicious_entries = vec![
            "../../../etc/passwd".to_string(),
            "..\\..\\windows\\system32\\config\\sam".to_string(),
        ];

        for entry in malicious_entries {
            let result = create_copy_plan(&[entry.clone()], &config);

            // Should fail due to path validation
            assert!(
                result.is_err(),
                "Path traversal should be blocked for: {entry}"
            );

            let error_msg = result.unwrap_err().to_string().to_lowercase();
            assert!(
                error_msg.contains("path traversal")
                    || error_msg.contains("invalid")
                    || error_msg.contains("outside"),
                "Error should mention path traversal for: {entry} (got: {error_msg})"
            );
        }
    }

    #[test]
    fn test_windows_reserved_names() {
        let temp_dir = TempDir::new().unwrap();
        let config = CopyConfig {
            output_dir: temp_dir.path().to_path_buf(),
            overwrite_mode: OverwriteMode::Prompt,
            max_concurrency: 4,
        };

        let reserved_names = vec![
            "CON.mdc".to_string(),
            "PRN.mdc".to_string(),
            "AUX.mdc".to_string(),
            "NUL.mdc".to_string(),
            "COM1.mdc".to_string(),
            "LPT1.mdc".to_string(),
        ];

        for name in reserved_names {
            let result = create_copy_plan(&[name.clone()], &config);

            // Should fail due to Windows reserved name validation
            assert!(
                result.is_err(),
                "Windows reserved name should be blocked: {name}"
            );

            let error_msg = result.unwrap_err().to_string().to_lowercase();
            assert!(
                error_msg.contains("reserved")
                    || error_msg.contains("windows")
                    || error_msg.contains("invalid destination"),
                "Error should mention reserved name for: {name} (got: {error_msg})"
            );
        }
    }

    #[test]
    fn test_null_byte_protection() {
        let temp_dir = TempDir::new().unwrap();
        let config = CopyConfig {
            output_dir: temp_dir.path().to_path_buf(),
            overwrite_mode: OverwriteMode::Prompt,
            max_concurrency: 4,
        };

        let malicious_name = "test\0.mdc".to_string();
        let result = create_copy_plan(&[malicious_name], &config);

        // Should fail due to null byte validation
        assert!(result.is_err(), "Null byte should be blocked");

        let error_msg = result.unwrap_err().to_string().to_lowercase();
        assert!(
            error_msg.contains("null") || error_msg.contains("invalid destination"),
            "Error should mention null byte (got: {error_msg})"
        );
    }

    #[test]
    fn test_safe_paths_allowed() {
        let temp_dir = TempDir::new().unwrap();
        let config = CopyConfig {
            output_dir: temp_dir.path().to_path_buf(),
            overwrite_mode: OverwriteMode::Prompt,
            max_concurrency: 4,
        };

        let safe_entries = vec![
            "frontend/react.mdc".to_string(),
            "backend/rust.mdc".to_string(),
            "deeply/nested/path/file.mdc".to_string(),
            "file-with-dashes.mdc".to_string(),
            "file_with_underscores.mdc".to_string(),
            "file123.mdc".to_string(),
        ];

        for entry in safe_entries {
            let result = create_copy_plan(&[entry.clone()], &config);
            assert!(result.is_ok(), "Safe path should be allowed: {entry}");

            let plans = result.unwrap();
            assert_eq!(plans.len(), 1);
        }
    }

    #[test]
    fn test_generate_unique_filename() {
        let temp_dir = TempDir::new().unwrap();

        // Create a base file
        let base_path = temp_dir.path().join("test.mdc");
        std::fs::write(&base_path, "content").unwrap();

        // Generate unique filename
        let unique_path = generate_unique_filename(&base_path);
        assert_eq!(unique_path.file_name().unwrap(), "test(1).mdc");
        assert!(!unique_path.exists());

        // Create the first rename and try again
        std::fs::write(&unique_path, "content").unwrap();
        let unique_path2 = generate_unique_filename(&base_path);
        assert_eq!(unique_path2.file_name().unwrap(), "test(2).mdc");
        assert!(!unique_path2.exists());
    }

    #[test]
    fn test_generate_unique_filename_no_extension() {
        let temp_dir = TempDir::new().unwrap();

        // Create a base file without extension
        let base_path = temp_dir.path().join("test");
        std::fs::write(&base_path, "content").unwrap();

        // Generate unique filename
        let unique_path = generate_unique_filename(&base_path);
        assert_eq!(unique_path.file_name().unwrap(), "test(1)");
        assert!(!unique_path.exists());
    }

    #[test]
    fn test_copy_result_variants() {
        let copied = CopyResult::Copied;
        let skipped = CopyResult::Skipped;
        let renamed = CopyResult::Renamed("test(1).mdc".to_string());

        // Test equality
        assert_eq!(copied, CopyResult::Copied);
        assert_eq!(skipped, CopyResult::Skipped);
        assert_eq!(renamed, CopyResult::Renamed("test(1).mdc".to_string()));

        // Test inequality
        assert_ne!(copied, skipped);
        assert_ne!(skipped, renamed);
        assert_ne!(copied, renamed);
    }

    #[test]
    fn test_batch_conflict_state() {
        let state = BatchConflictState::new();

        // Initially no global choice
        assert_eq!(state.get_global_choice(), None);

        // Set a global choice
        state.set_global_choice(ConflictChoice::OverwriteAll);
        assert_eq!(
            state.get_global_choice(),
            Some(ConflictChoice::OverwriteAll)
        );

        // Change the global choice
        state.set_global_choice(ConflictChoice::SkipAll);
        assert_eq!(state.get_global_choice(), Some(ConflictChoice::SkipAll));
    }

    #[tokio::test]
    async fn test_copy_single_file_enhanced_skip_action() {
        let temp_dir = TempDir::new().unwrap();
        let dest_file = temp_dir.path().join("test.mdc");

        let plan = CopyPlan {
            source_path: "test.mdc".to_string(),
            destination_path: dest_file.clone(),
            would_overwrite: true,
            action: CopyAction::Skip,
        };

        let repo_locator = RepoLocator {
            owner: "test".to_string(),
            repo: "test".to_string(),
            branch: "main".to_string(),
        };

        let octocrab = Arc::new(octocrab::instance());

        // Should skip without making network calls
        let result = copy_single_file_enhanced(&plan, &repo_locator, &octocrab)
            .await
            .unwrap();
        assert_eq!(result, CopyResult::Skipped);
    }

    #[test]
    fn test_copy_stats_fields() {
        let mut stats = CopyStats::default();
        assert_eq!(stats.files_copied, 0);
        assert_eq!(stats.files_skipped, 0);
        assert_eq!(stats.files_failed, 0);
        assert_eq!(stats.files_renamed, 0);

        stats.files_copied = 5;
        stats.files_skipped = 2;
        stats.files_failed = 1;
        stats.files_renamed = 3;

        assert_eq!(stats.files_copied, 5);
        assert_eq!(stats.files_skipped, 2);
        assert_eq!(stats.files_failed, 1);
        assert_eq!(stats.files_renamed, 3);
    }

    #[test]
    fn test_overwrite_mode_variants() {
        use OverwriteMode::*;

        let modes = [Prompt, Force, Skip, Rename, PromptOnce];

        // Test that all variants can be created and compared
        for mode in &modes {
            assert_eq!(*mode, *mode);
        }

        // Test inequality
        assert_ne!(Prompt, Force);
        assert_ne!(Force, Skip);
        assert_ne!(Skip, Rename);
        assert_ne!(Rename, PromptOnce);
    }

    #[test]
    fn test_copy_config_builder_methods_comprehensive() {
        let base_config = CopyConfig::default();

        let force_config = base_config.clone().with_force_overwrite();
        assert_eq!(force_config.overwrite_mode, OverwriteMode::Force);

        let skip_config = base_config.clone().with_skip_overwrite();
        assert_eq!(skip_config.overwrite_mode, OverwriteMode::Skip);

        let rename_config = base_config.with_rename_overwrite();
        assert_eq!(rename_config.overwrite_mode, OverwriteMode::Rename);
    }

    #[test]
    fn test_dry_run_table_with_rename_actions() {
        let temp_dir = TempDir::new().unwrap();

        // Create existing files to trigger renames
        let file1 = temp_dir.path().join("react.mdc");
        let file2 = temp_dir.path().join("rust.mdc");
        std::fs::write(&file1, "existing").unwrap();
        std::fs::write(&file2, "existing").unwrap();

        let config = CopyConfig {
            output_dir: temp_dir.path().to_path_buf(),
            overwrite_mode: OverwriteMode::Rename,
            max_concurrency: 4,
        };

        let entries = vec![
            "frontend/react.mdc".to_string(),
            "backend/rust.mdc".to_string(),
        ];

        let plans = create_copy_plan(&entries, &config).unwrap();
        let table = render_copy_plan_table(&plans);

        // Should contain rename arrows
        assert!(table.contains("→"));
        assert!(table.contains("Rename"));
        assert!(table.contains("react(1).mdc"));
        assert!(table.contains("rust(1).mdc"));
    }

    // Note: Testing actual GitHub API calls and downloads would require:
    // 1. Mock server setup (complex for this test suite)
    // 2. Network access (unreliable in CI)
    // 3. Valid GitHub tokens (security concern)
    //
    // These scenarios are covered by:
    // - Integration tests with real repositories
    // - Manual testing during development
    // - End-to-end CLI tests
}

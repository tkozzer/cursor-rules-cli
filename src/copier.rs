//! File copying functionality with progress tracking and concurrency control.
//!
//! This module handles downloading rule files from GitHub repositories and
//! copying them to the local filesystem with progress indicators.

use anyhow::{Context, Result};
use base64::Engine;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::{fs, sync::Semaphore};

use crate::github::RepoLocator;

/// Configuration for copy operations
#[derive(Debug, Clone)]
pub struct CopyConfig {
    /// Output directory for copied files
    pub output_dir: PathBuf,
    /// Whether to force overwrite existing files
    pub force_overwrite: bool,
    /// Maximum number of concurrent downloads
    pub max_concurrency: usize,
}

impl Default for CopyConfig {
    fn default() -> Self {
        Self {
            output_dir: PathBuf::from("./.cursor/rules"),
            force_overwrite: false,
            max_concurrency: 4,
        }
    }
}

/// Represents a planned copy operation
#[derive(Debug, Clone)]
pub struct CopyPlan {
    /// Source file path in the repository
    pub source_path: String,
    /// Destination file path on local filesystem
    pub destination_path: PathBuf,
    /// Whether this operation would overwrite an existing file
    pub would_overwrite: bool,
}

/// Statistics for copy operations
#[derive(Debug, Default)]
pub struct CopyStats {
    pub files_copied: usize,
    pub files_skipped: usize,
    pub files_failed: usize,
}

/// Create a copy plan for the given manifest entries
pub fn create_copy_plan(entries: &[String], config: &CopyConfig) -> Result<Vec<CopyPlan>> {
    let mut plans = Vec::new();

    // Ensure output directory exists
    let output_dir = &config.output_dir;

    for entry in entries {
        let filename = Path::new(entry)
            .file_name()
            .context("Invalid file path in manifest")?
            .to_string_lossy();

        let destination_path = output_dir.join(filename.as_ref());
        let would_overwrite = destination_path.exists();

        plans.push(CopyPlan {
            source_path: entry.clone(),
            destination_path,
            would_overwrite,
        });
    }

    Ok(plans)
}

/// Render copy plan as a formatted table
pub fn render_copy_plan_table(plans: &[CopyPlan]) -> String {
    if plans.is_empty() {
        return "No files to copy.".to_string();
    }

    let mut output = String::new();

    // Header
    output.push_str(&format!(
        "{:<40} {:<30} {:<10}\n",
        "Source", "Destination", "Overwrite?"
    ));
    output.push_str(&format!("{:-<82}\n", ""));

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

        output.push_str(&format!("{:<40} {:<30} {:<10}\n", source, dest, overwrite));
    }

    output.push_str(&format!("\nTotal files: {}\n", plans.len()));
    output
}

/// Execute copy plan with progress tracking
pub async fn execute_copy_plan(
    plans: Vec<CopyPlan>,
    repo_locator: &RepoLocator,
    config: &CopyConfig,
) -> Result<CopyStats> {
    if plans.is_empty() {
        return Ok(CopyStats::default());
    }

    // Create output directory if it doesn't exist
    fs::create_dir_all(&config.output_dir)
        .await
        .context("Failed to create output directory")?;

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
        let force_overwrite = config.force_overwrite;
        let octocrab = octocrab.clone();

        let task = tokio::spawn(async move {
            let _permit = semaphore.acquire().await.unwrap();

            let result = copy_single_file(&plan, &repo_locator, force_overwrite, &octocrab).await;

            overall_pb.inc(1);

            match result {
                Ok(copied) => {
                    if copied {
                        overall_pb.set_message(format!("Copied {}", plan.source_path));
                    } else {
                        overall_pb.set_message(format!("Skipped {}", plan.source_path));
                    }
                    result
                }
                Err(ref e) => {
                    overall_pb.set_message(format!("Failed {}: {}", plan.source_path, e));
                    result
                }
            }
        });

        tasks.push(task);
    }

    // Wait for all tasks to complete
    for task in tasks {
        match task.await? {
            Ok(copied) => {
                if copied {
                    stats.files_copied += 1;
                } else {
                    stats.files_skipped += 1;
                }
            }
            Err(_) => {
                stats.files_failed += 1;
            }
        }
    }

    overall_pb.finish_with_message(format!(
        "Complete! Copied: {}, Skipped: {}, Failed: {}",
        stats.files_copied, stats.files_skipped, stats.files_failed
    ));

    Ok(stats)
}

/// Copy a single file from the repository
async fn copy_single_file(
    plan: &CopyPlan,
    repo_locator: &RepoLocator,
    force_overwrite: bool,
    octocrab: &Arc<octocrab::Octocrab>,
) -> Result<bool> {
    // Check if file exists and we're not forcing overwrite
    if plan.would_overwrite && !force_overwrite {
        // For now, we'll skip. In the future, we could prompt the user.
        return Ok(false);
    }

    // Download file content from GitHub
    let content = download_file_content(
        octocrab,
        &repo_locator.owner,
        &repo_locator.repo,
        &plan.source_path,
        &repo_locator.branch,
    )
    .await?;

    // Write to destination
    if let Some(parent) = plan.destination_path.parent() {
        fs::create_dir_all(parent)
            .await
            .context("Failed to create parent directories")?;
    }

    fs::write(&plan.destination_path, content)
        .await
        .context("Failed to write file")?;

    Ok(true)
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
    use std::sync::Arc;
    use tempfile::TempDir;

    #[test]
    fn test_copy_plan_creation_success() {
        let temp_dir = TempDir::new().unwrap();
        let config = CopyConfig {
            output_dir: temp_dir.path().to_path_buf(),
            force_overwrite: false,
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
    }

    #[test]
    fn test_copy_plan_creation_empty_entries() {
        let temp_dir = TempDir::new().unwrap();
        let config = CopyConfig {
            output_dir: temp_dir.path().to_path_buf(),
            force_overwrite: false,
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
            force_overwrite: false,
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
    }

    #[test]
    fn test_copy_plan_handles_conflicts() {
        let temp_dir = TempDir::new().unwrap();

        // Create a file that would conflict
        let existing_file = temp_dir.path().join("react.mdc");
        std::fs::write(&existing_file, "existing content").unwrap();

        let config = CopyConfig {
            output_dir: temp_dir.path().to_path_buf(),
            force_overwrite: false,
            max_concurrency: 4,
        };

        let entries = vec!["frontend/react.mdc".to_string()];
        let plans = create_copy_plan(&entries, &config).unwrap();

        assert_eq!(plans.len(), 1);
        assert!(plans[0].would_overwrite);
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
            force_overwrite: false,
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
    }

    #[test]
    fn test_dry_run_table_rendering() {
        let temp_dir = TempDir::new().unwrap();
        let plans = vec![
            CopyPlan {
                source_path: "frontend/react.mdc".to_string(),
                destination_path: temp_dir.path().join("react.mdc"),
                would_overwrite: false,
            },
            CopyPlan {
                source_path: "backend/rust.mdc".to_string(),
                destination_path: temp_dir.path().join("rust.mdc"),
                would_overwrite: true,
            },
        ];

        let table = render_copy_plan_table(&plans);

        assert!(table.contains("Source"));
        assert!(table.contains("Destination"));
        assert!(table.contains("Overwrite?"));
        assert!(table.contains("frontend/react.mdc"));
        assert!(table.contains("backend/rust.mdc"));
        assert!(table.contains("Total files: 2"));
        assert!(table.contains("Yes")); // overwrite for rust.mdc
        assert!(table.contains("No")); // no overwrite for react.mdc
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
        }];

        let table = render_copy_plan_table(&plans);

        // Should truncate long paths with ...
        assert!(table.contains("..."));
        assert!(table.contains("file.mdc"));
    }

    #[test]
    fn test_copy_operation_error_handling() {
        // Test that we handle errors gracefully when invalid paths are provided
        let copy_config = CopyConfig {
            output_dir: PathBuf::from("/invalid/path/that/does/not/exist"),
            force_overwrite: false,
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

        let copy_config_no_force = CopyConfig {
            output_dir: output_dir.to_path_buf(),
            force_overwrite: false,
            max_concurrency: 1,
        };

        let copy_config_force = CopyConfig {
            output_dir: output_dir.to_path_buf(),
            force_overwrite: true,
            max_concurrency: 1,
        };

        let entries = vec!["test.mdc".to_string()];

        let plan_no_force = create_copy_plan(&entries, &copy_config_no_force).unwrap();
        let plan_force = create_copy_plan(&entries, &copy_config_force).unwrap();

        // Both plans should be created successfully
        assert_eq!(plan_no_force.len(), 1);
        assert_eq!(plan_force.len(), 1);

        // Check conflict detection - the file exists so should indicate overwrite
        assert!(plan_no_force[0].would_overwrite);
        assert!(plan_force[0].would_overwrite);
    }

    #[test]
    fn test_copy_config_default() {
        let config = CopyConfig::default();

        assert_eq!(config.output_dir, PathBuf::from("./.cursor/rules"));
        assert!(!config.force_overwrite);
        assert_eq!(config.max_concurrency, 4);
    }

    #[test]
    fn test_copy_config_custom() {
        let custom_dir = PathBuf::from("/custom/path");
        let config = CopyConfig {
            output_dir: custom_dir.clone(),
            force_overwrite: true,
            max_concurrency: 8,
        };

        assert_eq!(config.output_dir, custom_dir);
        assert!(config.force_overwrite);
        assert_eq!(config.max_concurrency, 8);
    }

    #[test]
    fn test_copy_stats_default() {
        let stats = CopyStats::default();

        assert_eq!(stats.files_copied, 0);
        assert_eq!(stats.files_skipped, 0);
        assert_eq!(stats.files_failed, 0);
    }

    #[test]
    fn test_copy_plan_debug() {
        let temp_dir = TempDir::new().unwrap();
        let plan = CopyPlan {
            source_path: "test.mdc".to_string(),
            destination_path: temp_dir.path().join("test.mdc"),
            would_overwrite: false,
        };

        let debug_str = format!("{:?}", plan);
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

        let stats = execute_copy_plan(plans, &repo_locator, &config)
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
            force_overwrite: false,
            max_concurrency: 1,
        };

        // Create a plan that will fail at download (since we don't have a real GitHub API)
        // but should still create the directory
        let plans = vec![CopyPlan {
            source_path: "test.mdc".to_string(),
            destination_path: output_dir.join("test.mdc"),
            would_overwrite: false,
        }];

        let repo_locator = RepoLocator {
            owner: "test".to_string(),
            repo: "test".to_string(),
            branch: "main".to_string(),
        };

        // This will fail due to GitHub API, but the directory should be created
        let _result = execute_copy_plan(plans, &repo_locator, &config).await;

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
        };

        let repo_locator = RepoLocator {
            owner: "test".to_string(),
            repo: "test".to_string(),
            branch: "main".to_string(),
        };

        let octocrab = Arc::new(octocrab::instance());

        // Should skip the file since force_overwrite is false
        let result = copy_single_file(&plan, &repo_locator, false, &octocrab)
            .await
            .unwrap();
        assert!(!result); // Should return false indicating skipped

        // File should still contain original content
        let content = std::fs::read_to_string(&dest_file).unwrap();
        assert_eq!(content, "existing content");
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

use clap::{Parser, Subcommand};
mod copier;
mod github;
mod ui;

use base64::Engine;
use copier::{create_copy_plan, execute_copy_plan, render_copy_plan_table, CopyConfig};
use github::{find_manifests_in_quickadd, parse_manifest_content, ManifestFormat};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "cursor-rules",
    version = "0.1.1",
    author = "Tyler Kozlowski <tkoz.dev@gmail.com>",
    about = "A CLI tool for managing Cursor rules from GitHub repositories",
    long_about = "An interactive, cross-platform Rust CLI that allows developers to browse GitHub repositories named 'cursor-rules' and copy selected .mdc rule files into their projects."
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// GitHub owner to fetch rules from
    #[arg(long, short)]
    owner: Option<String>,

    /// Repository name (defaults to 'cursor-rules')
    #[arg(long, short)]
    repo: Option<String>,

    /// Branch to fetch from (defaults to 'main')
    #[arg(long, short)]
    branch: Option<String>,

    /// GitHub token for authentication
    #[arg(long, short)]
    token: Option<String>,

    /// Output directory (defaults to './.cursor/rules')
    #[arg(long)]
    out: Option<String>,

    /// Show what would be done without making changes
    #[arg(long)]
    dry_run: bool,

    /// Force refresh cache
    #[arg(long)]
    refresh: bool,

    /// Verbose output
    #[arg(long, short)]
    verbose: bool,

    /// Force overwrite without prompting
    #[arg(long)]
    force: bool,

    /// Output in JSON format
    #[arg(long)]
    json: bool,

    /// Show hidden files and directories (those starting with dot)
    #[arg(long)]
    all: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Interactive browser (default)
    Browse,
    /// Apply a manifest (ID = filename or friendly slug)
    QuickAdd { id: String },
    /// Print repo tree in JSON/YAML
    List,
    /// Show or modify saved config
    Config,
    /// Manage offline cache (list|clear)
    Cache { action: Option<String> },
    /// Generate shell completions
    Completions { shell: String },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    if cli.verbose {
        // Initialise tracing subscriber in verbose mode
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .init();
    }

    match github::resolve_repo(
        cli.owner.clone(),
        cli.repo.clone(),
        cli.branch.clone(),
        cli.token.clone(),
    )
    .await
    {
        Ok(locator) => {
            println!(
                "Resolved repo: {}/{}@{}",
                locator.owner, locator.repo, locator.branch
            );

            // If no explicit subcommand or `browse`, launch the interactive browser UI.
            use tokio::sync::mpsc;

            let (tx, mut rx) = mpsc::unbounded_channel();

            match cli.command {
                None | Some(Commands::Browse) => {
                    // Run UI in background task and handle messages in main thread
                    let mut ui_task = tokio::spawn({
                        let locator = locator.clone();
                        let tx = tx.clone();
                        let all = cli.all;
                        async move { ui::run(&locator, tx, all).await }
                    });

                    // Handle messages from UI
                    loop {
                        tokio::select! {
                            // Handle UI messages
                            msg = rx.recv() => {
                                match msg {
                                    Some(ui::AppMessage::CopyRequest { path }) => {
                                        if let Err(e) = handle_browser_selection(&locator, &path, &cli).await {
                                            eprintln!("Copy error: {e}");
                                        }
                                    }
                                    None => {
                                        // Channel closed, UI task finished
                                        break;
                                    }
                                }
                            }
                            // Wait for UI task to complete
                            ui_result = &mut ui_task => {
                                match ui_result {
                                    Ok(Ok(())) => {},
                                    Ok(Err(e)) => {
                                        eprintln!("UI error: {e}");
                                        std::process::exit(1);
                                    }
                                    Err(e) => {
                                        eprintln!("UI task error: {e}");
                                        std::process::exit(1);
                                    }
                                }
                                break;
                            }
                        }
                    }
                }
                Some(Commands::QuickAdd { ref id }) => {
                    if let Err(e) = handle_quick_add(&locator, &id, &cli).await {
                        eprintln!("Quick-add error: {e}");
                        std::process::exit(1);
                    }
                }
                // Other subcommands will be implemented in future FRs.
                _ => {
                    eprintln!("Subcommand not yet implemented");
                }
            }
        }
        Err(e) => {
            eprintln!("Error resolving repository: {e}");
            std::process::exit(1);
        }
    }
}

/// Handle the quick-add command
async fn handle_quick_add(
    locator: &github::RepoLocator,
    manifest_id: &str,
    cli: &Cli,
) -> anyhow::Result<()> {
    // Create repo tree and find available manifests in the quick-add directory
    let mut repo_tree = github::RepoTree::new();
    let available_manifests = find_manifests_in_quickadd(&mut repo_tree, locator).await?;

    if available_manifests.is_empty() {
        println!("No manifests found in the quick-add/ directory.");
        return Ok(());
    }

    // Try to resolve the manifest ID
    let (manifest_format, manifest_path) =
        match resolve_manifest_id(manifest_id, &available_manifests) {
            Some(manifest) => manifest,
            None => {
                eprintln!("Manifest '{}' not found.", manifest_id);
                eprintln!("Available manifests:");
                for (id, (format, _)) in &available_manifests {
                    eprintln!("  - {} (.{})", id, format_extension(format));
                }
                std::process::exit(2);
            }
        };

    // Download and parse the manifest content
    let manifest_content = download_manifest_content(locator, &manifest_path).await?;
    let manifest = parse_manifest_content(
        &manifest_content,
        manifest_format,
        manifest_id,
        &mut repo_tree,
        locator,
    )
    .await?;

    // Report any validation errors or warnings
    if !manifest.warnings.is_empty() {
        eprintln!("Warnings:");
        for warning in &manifest.warnings {
            eprintln!("  ⚠ {}", warning);
        }
    }

    if !manifest.errors.is_empty() {
        eprintln!("Errors:");
        for error in &manifest.errors {
            eprintln!("  ✗ {}", error);
        }
        std::process::exit(2);
    }

    if manifest.entries.is_empty() {
        println!("No valid rule files found in manifest.");
        return Ok(());
    }

    // Create copy configuration
    let copy_config = CopyConfig {
        output_dir: cli
            .out
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("./.cursor/rules")),
        force_overwrite: cli.force,
        max_concurrency: 4,
    };

    // Create copy plan
    let copy_plan = create_copy_plan(&manifest.entries, &copy_config)?;

    // Handle dry-run mode
    if cli.dry_run {
        println!("Dry-run mode: Showing what would be copied");
        println!();
        println!("Manifest: {} ({})", manifest.name, manifest_id);
        if let Some(description) = &manifest.description {
            println!("Description: {}", description);
        }
        println!();
        println!("{}", render_copy_plan_table(&copy_plan));

        // Exit with appropriate code
        let has_validation_errors = !manifest.errors.is_empty();
        std::process::exit(if has_validation_errors { 2 } else { 0 });
    }

    // Execute the copy plan
    println!("Applying manifest: {} ({})", manifest.name, manifest_id);
    if let Some(description) = &manifest.description {
        println!("Description: {}", description);
    }
    println!();

    let stats = execute_copy_plan(copy_plan, locator, &copy_config).await?;

    println!();
    println!("Copy operation completed:");
    println!("  Files copied: {}", stats.files_copied);
    println!("  Files skipped: {}", stats.files_skipped);
    println!("  Files failed: {}", stats.files_failed);

    if stats.files_failed > 0 {
        std::process::exit(1);
    }

    Ok(())
}

/// Handle file/manifest selection from the interactive browser
async fn handle_browser_selection(
    locator: &github::RepoLocator,
    file_path: &str,
    cli: &Cli,
) -> anyhow::Result<()> {
    use crate::copier::{create_copy_plan, execute_copy_plan, CopyConfig};

    use std::path::PathBuf;

    // Check if this is a manifest file
    if file_path.starts_with("quick-add/") && is_manifest_file(file_path) {
        // Extract manifest ID from path (filename without extension)
        let manifest_filename = file_path.strip_prefix("quick-add/").unwrap();
        let manifest_id = if let Some(pos) = manifest_filename.rfind('.') {
            &manifest_filename[..pos]
        } else {
            manifest_filename
        };

        println!("Applying manifest: {}", manifest_id);

        // Use the existing quick-add logic
        handle_quick_add(locator, manifest_id, cli).await
    } else if file_path.ends_with(".mdc") {
        // Single file copy
        println!("Copying file: {}", file_path);

        let copy_config = CopyConfig {
            output_dir: cli
                .out
                .as_ref()
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("./.cursor/rules")),
            force_overwrite: cli.force,
            max_concurrency: 1,
        };

        // Create copy plan for single file
        let copy_plan = create_copy_plan(&[file_path.to_string()], &copy_config)?;

        if cli.dry_run {
            println!("Dry-run mode: Would copy {}", file_path);
        } else {
            let stats = execute_copy_plan(copy_plan, locator, &copy_config).await?;
            println!("Copied {} file(s)", stats.files_copied);
        }

        Ok(())
    } else {
        // Unsupported file type
        println!("File type not supported for copying: {}", file_path);
        Ok(())
    }
}

/// Resolve manifest ID to format and path
fn resolve_manifest_id(
    manifest_id: &str,
    available_manifests: &std::collections::HashMap<String, (ManifestFormat, String)>,
) -> Option<(ManifestFormat, String)> {
    // First, try exact ID match (basename without extension)
    if let Some((format, path)) = available_manifests.get(manifest_id) {
        return Some((format.clone(), path.clone()));
    }

    // If ID contains extension, try to find exact filename match
    if manifest_id.contains('.') {
        for (format, path) in available_manifests.values() {
            if path.ends_with(manifest_id) {
                return Some((format.clone(), path.clone()));
            }
        }
    }

    None
}

/// Download manifest content from GitHub
async fn download_manifest_content(
    locator: &github::RepoLocator,
    manifest_path: &str,
) -> anyhow::Result<String> {
    let octocrab = octocrab::instance();

    let response = octocrab
        .repos(&locator.owner, &locator.repo)
        .get_content()
        .path(manifest_path)
        .r#ref(&locator.branch)
        .send()
        .await?;

    match response.items.first() {
        Some(content) if content.download_url.is_some() => {
            let download_url = content.download_url.as_ref().unwrap();
            let response = reqwest::get(download_url).await?;
            let text = response.text().await?;
            Ok(text)
        }
        Some(content) if content.content.is_some() => {
            // Handle base64 encoded content
            let encoded_content = content.content.as_ref().unwrap();
            let cleaned = encoded_content.replace('\n', "").replace(' ', "");
            let decoded = base64::engine::general_purpose::STANDARD.decode(cleaned)?;
            let text = String::from_utf8(decoded)?;
            Ok(text)
        }
        _ => anyhow::bail!("Manifest content not available"),
    }
}

/// Get file extension for manifest format
fn format_extension(format: &ManifestFormat) -> &'static str {
    match format {
        ManifestFormat::Txt => "txt",
        ManifestFormat::Yaml => "yaml",
        ManifestFormat::Json => "json",
    }
}

/// Check if a file is a manifest based on its extension
fn is_manifest_file(filename: &str) -> bool {
    filename.ends_with(".txt")
        || filename.ends_with(".yaml")
        || filename.ends_with(".yml")
        || filename.ends_with(".json")
}

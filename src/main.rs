use clap::{Parser, Subcommand};
mod config;
mod copier;
mod github;
mod ui;

use base64::Engine;
use config::{
    delete_config_value, load_config, resolve_github_token, update_config_value, Config,
    KeyringStore, SecretStore,
};
use copier::{create_copy_plan, execute_copy_plan, render_copy_plan_table, CopyConfig};
use github::{find_manifests_in_quickadd, parse_manifest_content, ManifestFormat};
use inquire::Confirm;
use is_terminal::IsTerminal;
use std::io;
use std::path::PathBuf;
use ui::prompts::{InteractivePromptService, NonInteractivePromptService, PromptService};

#[derive(Parser)]
#[command(
    name = "cursor-rules",
    version = env!("CARGO_PKG_VERSION"),
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
    Config {
        #[command(subcommand)]
        action: Option<ConfigAction>,
    },
    /// Manage offline cache (list|clear)
    Cache { action: Option<String> },
    /// Generate shell completions
    Completions { shell: String },
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Set a configuration value
    Set { key: String, value: String },
    /// Delete a configuration value
    Delete { key: String },
    /// Show current configuration
    Show,
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

    // Load config and resolve token using priority system
    let config = match load_config() {
        Ok(config) => config,
        Err(e) => {
            if cli.verbose {
                eprintln!("Warning: Failed to load config: {}", e);
            }
            Config::default()
        }
    };

    let secret_store = KeyringStore;
    let resolved_token = match resolve_github_token(cli.token.as_deref(), &secret_store) {
        Ok(token) => token,
        Err(e) => {
            if cli.verbose {
                eprintln!("Warning: Failed to resolve token: {}", e);
            }
            None
        }
    };

    // Apply config defaults where CLI args are not provided
    let owner = cli.owner.clone().or(config.owner);
    let repo = cli.repo.clone().or(config.repo);
    let out_dir = cli.out.clone().or(config.out_dir);

    match github::resolve_repo(
        owner.clone(),
        repo.clone(),
        cli.branch.clone(),
        resolved_token.clone(),
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
                                        if let Err(e) = handle_browser_selection(&locator, &path, &cli, out_dir.as_deref()).await {
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
                    if let Err(e) = handle_quick_add(&locator, id, &cli, out_dir.as_deref()).await {
                        eprintln!("Quick-add error: {e}");
                        std::process::exit(1);
                    }
                }
                Some(Commands::Config { ref action }) => {
                    if let Err(e) = handle_config_command(action.as_ref()).await {
                        eprintln!("Config error: {e}");
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

/// Handle config subcommands
async fn handle_config_command(action: Option<&ConfigAction>) -> anyhow::Result<()> {
    let secret_store = KeyringStore;

    match action {
        None | Some(ConfigAction::Show) => {
            // Show current configuration
            let config = load_config().map_err(anyhow::Error::from)?;
            let token = resolve_github_token(None, &secret_store).map_err(anyhow::Error::from)?;

            println!("Current configuration:");
            println!();
            println!(
                "{:<12} {}",
                "owner:",
                config.owner.unwrap_or_else(|| "unset".to_string())
            );
            println!(
                "{:<12} {}",
                "repo:",
                config.repo.unwrap_or_else(|| "unset".to_string())
            );
            println!(
                "{:<12} {}",
                "out_dir:",
                config.out_dir.unwrap_or_else(|| "unset".to_string())
            );
            println!(
                "{:<12} {}",
                "telemetry:",
                config
                    .telemetry
                    .map_or("unset".to_string(), |t| t.to_string())
            );
            println!(
                "{:<12} {}",
                "token:",
                if token.is_some() {
                    "✓ stored in keyring"
                } else {
                    "✗ not set"
                }
            );

            // Show config file path
            let config_path = config::config_file_path().map_err(anyhow::Error::from)?;
            println!();
            println!("Config file: {}", config_path.display());
        }
        Some(ConfigAction::Set { key, value }) => {
            if key == "token" {
                // Special handling for token - store in keyring
                let confirmation = if io::stdin().is_terminal() {
                    Confirm::new("Store GitHub token in secure keyring?")
                        .with_default(true)
                        .prompt()
                        .unwrap_or(false)
                } else {
                    true // Non-interactive mode, assume yes
                };

                if confirmation {
                    secret_store.set_token(value).map_err(anyhow::Error::from)?;
                    println!("GitHub token stored securely in keyring.");

                    // Validate token by making a test API call
                    match validate_github_token(value).await {
                        Ok(()) => println!("✓ Token validation successful."),
                        Err(e) => {
                            eprintln!("⚠ Warning: Token validation failed: {}", e);
                            eprintln!("The token has been stored but may not be valid.");
                        }
                    }
                } else {
                    println!("Token not stored.");
                }
            } else {
                // Regular config value
                update_config_value(key, value).map_err(anyhow::Error::from)?;
                println!("Set {} = {}", key, value);
            }
        }
        Some(ConfigAction::Delete { key }) => {
            if key == "token" {
                // Special handling for token - delete from keyring
                let confirmation = if io::stdin().is_terminal() {
                    Confirm::new("Delete GitHub token from keyring?")
                        .with_default(false)
                        .prompt()
                        .unwrap_or(false)
                } else {
                    false // Non-interactive mode, don't delete without explicit confirmation
                };

                if confirmation {
                    secret_store.delete_token().map_err(anyhow::Error::from)?;
                    println!("GitHub token deleted from keyring.");
                } else {
                    println!("Token not deleted.");
                }
            } else {
                // Regular config value
                delete_config_value(key).map_err(anyhow::Error::from)?;
                println!("Deleted {}", key);
            }
        }
    }

    Ok(())
}

/// Validate a GitHub token by making a test API call
async fn validate_github_token(token: &str) -> anyhow::Result<()> {
    let octocrab = octocrab::Octocrab::builder()
        .personal_token(token.to_string())
        .build()?;

    // Make a simple API call to validate the token
    let _user = octocrab.current().user().await?;
    Ok(())
}

/// Handle the quick-add command
async fn handle_quick_add(
    locator: &github::RepoLocator,
    manifest_id: &str,
    cli: &Cli,
    out_dir: Option<&str>,
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
        output_dir: out_dir
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("./.cursor/rules")),
        overwrite_mode: if cli.force {
            copier::OverwriteMode::Force
        } else {
            copier::OverwriteMode::Prompt
        },
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

    // Create appropriate prompt service based on CLI flags
    let prompt_service: Box<dyn PromptService> = if cli.force {
        Box::new(NonInteractivePromptService::overwrite_all())
    } else {
        Box::new(InteractivePromptService::new())
    };

    let stats =
        execute_copy_plan(copy_plan, locator, &copy_config, prompt_service.as_ref()).await?;

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
    out_dir: Option<&str>,
) -> anyhow::Result<()> {
    use crate::copier::{create_copy_plan, execute_copy_plan, CopyConfig};
    use crate::ui::prompts::{
        InteractivePromptService, NonInteractivePromptService, PromptService,
    };

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
        handle_quick_add(locator, manifest_id, cli, out_dir).await
    } else if file_path.ends_with(".mdc") {
        // Single file copy
        println!("Copying file: {}", file_path);

        let copy_config = CopyConfig {
            output_dir: out_dir
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("./.cursor/rules")),
            overwrite_mode: if cli.force {
                copier::OverwriteMode::Force
            } else {
                copier::OverwriteMode::Prompt
            },
            max_concurrency: 1,
        };

        // Create copy plan for single file
        let copy_plan = create_copy_plan(&[file_path.to_string()], &copy_config)?;

        if cli.dry_run {
            println!("Dry-run mode: Would copy {}", file_path);
        } else {
            // Create appropriate prompt service based on CLI flags
            let prompt_service: Box<dyn PromptService> = if cli.force {
                Box::new(NonInteractivePromptService::overwrite_all())
            } else {
                Box::new(InteractivePromptService::new())
            };

            let stats =
                execute_copy_plan(copy_plan, locator, &copy_config, prompt_service.as_ref())
                    .await?;
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
            let cleaned = encoded_content.replace(['\n', ' '], "");
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

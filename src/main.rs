use clap::{Parser, Subcommand};
mod github;
mod ui;

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

            let (tx, _rx) = mpsc::unbounded_channel();

            match cli.command {
                None | Some(Commands::Browse) => {
                    if let Err(e) = ui::run(&locator, tx.clone(), cli.all).await {
                        eprintln!("UI error: {e}");
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

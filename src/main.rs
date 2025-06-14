use clap::{Parser, Subcommand};

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

fn main() {
    let cli = Cli::parse();

    if cli.verbose {
        println!("Verbose mode enabled");
    }

    match &cli.command {
        Some(Commands::Browse) => {
            println!("Browse mode - not implemented yet");
        }
        Some(Commands::QuickAdd { id }) => {
            println!("Quick-add mode with ID: {} - not implemented yet", id);
        }
        Some(Commands::List) => {
            println!("List mode - not implemented yet");
        }
        Some(Commands::Config) => {
            println!("Config mode - not implemented yet");
        }
        Some(Commands::Cache { action }) => {
            println!("Cache mode with action: {:?} - not implemented yet", action);
        }
        Some(Commands::Completions { shell }) => {
            println!("Completions for shell: {} - not implemented yet", shell);
        }
        None => {
            // Default to browse mode when no subcommand is provided
            println!("Interactive browse mode - not implemented yet");
            println!("Owner: {:?}", cli.owner);
            println!("Repo: {:?}", cli.repo);
            println!("Branch: {:?}", cli.branch);
            println!("Output: {:?}", cli.out);
            println!("Dry run: {}", cli.dry_run);
            println!("Force: {}", cli.force);
        }
    }
}

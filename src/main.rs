mod commands;
mod models;
mod paths;
mod store;
mod youtube;

use anyhow::Result;
use clap::{Parser, Subcommand};
use colored::Colorize;

#[derive(Parser)]
#[command(name = "ytq", version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Add a video to the queue
    #[command(alias = "a")]
    Add {
        /// Video URL, short link, or video ID
        input: String,
    },

    /// Watch the next video and remove it from the queue
    #[command(
        visible_alias = "play",
        visible_alias = "watch",
        visible_alias = "open"
    )]
    Next {
        /// Video ID or URL to open a specific video (uses queue/stack mode if omitted)
        target: Option<String>,
    },

    /// List the current queue
    #[command(alias = "ls")]
    List,

    /// Look at the next few videos without watching
    Peek {
        /// How many videos to show
        #[arg(default_value_t = 1)]
        n: usize,
    },

    /// Remove a video by ID or URL
    #[command(visible_alias = "rm", visible_alias = "delete")]
    Remove {
        /// The ID or URL to remove
        target: String,
    },

    /// Show statistics about your queue history
    Stats,

    /// Update a configuration value
    Config {
        /// Configuration key (mode, offline)
        key: String,
        /// New value
        value: String,
    },

    /// Show data file locations
    Info,

    /// Pop and watch a random video from the queue
    #[command(alias = "lucky")]
    Random,
}

fn main() {
    if let Err(e) = run() {
        eprintln!("{} {e:#}", "error:".red());
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Add { input } => commands::add(&input),
        Commands::Next { target } => commands::next(target.as_deref()),
        Commands::List => commands::list(),
        Commands::Peek { n } => commands::peek(n),
        Commands::Remove { target } => commands::remove(&target),
        Commands::Stats => commands::stats(),
        Commands::Config { key, value } => commands::config(&key, &value),
        Commands::Info => commands::info(),
        Commands::Random => commands::random(),
    }
}

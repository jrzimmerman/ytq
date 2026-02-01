mod commands;
mod models;
mod paths;
mod store;
mod youtube;

use clap::{Parser, Subcommand};
use anyhow::Result;

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
        /// YouTube URL, Short Link, or Video ID
        input: String
    },
    /// Watch the next video and remove it from the queue
    #[command(visible_alias = "play", visible_alias = "watch")]
    Next,

    /// List the current queue
    #[command(alias = "ls")]
    List,

    /// Look at the next few videos without watching
    Peek {
        /// How many videos to show
        #[arg(default_value_t = 1)]
        n: usize
    },

    /// Remove a video by ID or URL (Strictly)
    #[command(visible_alias = "rm", visible_alias = "delete")]
    Remove {
        /// The ID or URL to remove
        target: String
    },

    Stats,

    Config {
        key: String,
        value: String
    },

    Info,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Add { input } => commands::add(input),
        Commands::Next => commands::next(),
        Commands::List => commands::list(),
        Commands::Peek { n } => commands::peek(n),
        Commands::Remove { target } => commands::remove(target),
        Commands::Stats => commands::stats(),
        Commands::Config { key, value } => commands::config(key, value),
        Commands::Info => commands::info(),
    }
}

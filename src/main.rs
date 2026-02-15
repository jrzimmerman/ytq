mod commands;
mod models;
mod paths;
mod stats;
mod store;
mod youtube;
mod youtube_api;

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
        alias = "n",
        alias = "p",
        alias = "w",
        alias = "o",
        visible_alias = "play",
        visible_alias = "watch",
        visible_alias = "open"
    )]
    Next {
        /// Video ID or URL to open a specific video (uses queue/stack mode if omitted)
        target: Option<String>,
    },

    /// List the current queue
    #[command(alias = "l", alias = "ls")]
    List,

    /// Look at the next few videos without watching
    #[command(alias = "k")]
    Peek {
        /// How many videos to show
        #[arg(default_value_t = 1)]
        n: usize,
    },

    /// Remove a video by ID or URL
    #[command(alias = "d", visible_alias = "rm", visible_alias = "delete")]
    Remove {
        /// The ID or URL to remove
        target: String,
    },

    /// Show statistics about your queue history
    #[command(alias = "s")]
    Stats {
        /// Show full "wrapped" deep-dive statistics
        #[arg(long)]
        wrapped: bool,

        /// Show stats for all time instead of just the current year
        #[arg(long)]
        all: bool,

        /// Filter to last 7 days
        #[arg(long, conflicts_with_all = ["month", "year", "from", "to"])]
        week: bool,

        /// Last 30 days, or a specific month (YYYY-MM)
        #[arg(long, num_args = 0..=1, default_missing_value = "", value_name = "YYYY-MM")]
        month: Option<String>,

        /// Last 365 days, or a specific year (YYYY)
        #[arg(long, num_args = 0..=1, default_missing_value = "", value_name = "YYYY")]
        year: Option<String>,

        /// Start date for custom range (YYYY-MM-DD)
        #[arg(long, conflicts_with_all = ["week", "month", "year"], value_name = "DATE")]
        from: Option<String>,

        /// End date for custom range (YYYY-MM-DD)
        #[arg(long, conflicts_with_all = ["week", "month", "year"], value_name = "DATE")]
        to: Option<String>,
    },

    /// Update a configuration value
    #[command(alias = "c")]
    Config {
        /// Configuration key (mode, offline, youtube_api_key)
        key: String,
        /// New value
        value: String,
    },

    /// Show data file locations
    #[command(alias = "i")]
    Info,

    /// Fetch video metadata from YouTube Data API v3
    #[command(alias = "f")]
    Fetch {
        /// Video ID(s), URL(s), or comma-separated list to fetch/refresh
        target: Option<String>,

        /// Fetch for queue videos only (default when no flags given)
        #[arg(long)]
        queue: bool,

        /// Fetch for history videos only
        #[arg(long)]
        history: bool,

        /// Fetch for all videos (queue + history)
        #[arg(long)]
        all: bool,

        /// Maximum number of videos to fetch (useful for testing)
        #[arg(long)]
        limit: Option<usize>,

        /// Force re-fetch metadata, including previously unavailable videos
        #[arg(long)]
        force: bool,

        /// Force refresh video categories
        #[arg(long)]
        refresh_categories: bool,
    },

    /// Pop and watch a random video from the queue
    #[command(alias = "r", alias = "lucky")]
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
        Commands::Stats {
            wrapped,
            all,
            week,
            month,
            year,
            from,
            to,
        } => commands::stats(wrapped, all, week, month, year, from, to),
        Commands::Config { key, value } => commands::config(&key, &value),
        Commands::Info => commands::info(),
        Commands::Fetch {
            target,
            queue,
            history,
            all,
            limit,
            force,
            refresh_categories,
        } => commands::fetch(
            target.as_deref(),
            queue,
            history,
            all,
            limit,
            force,
            refresh_categories,
        ),
        Commands::Random => commands::random(),
    }
}

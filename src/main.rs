mod commands;
mod db;
mod models;
mod output;
mod paths;
mod selection;
mod stats;
mod store;
mod youtube;
mod youtube_api;

use std::num::NonZeroUsize;

use anyhow::Result;
use clap::{Parser, Subcommand};
use colored::Colorize;

#[derive(Parser)]
#[command(name = "ytq", version, about, long_about = None)]
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

    /// Open the next matching video and remove it from the queue
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

        #[command(flatten)]
        selection: selection::SelectionArgs,
    },

    /// Search queued video IDs, cached titles, and channel names without network access
    Search {
        /// Literal case-insensitive substring; omit to browse using filters
        query: Option<String>,

        #[command(flatten)]
        selection: selection::SelectionArgs,

        /// Maximum results to display
        #[arg(long, default_value = "20", conflicts_with = "all")]
        limit: NonZeroUsize,

        /// Display every matching video
        #[arg(long)]
        all: bool,
    },

    /// List the current queue
    #[command(alias = "l", alias = "ls")]
    List,

    /// Look at the next few videos without opening them
    #[command(alias = "k")]
    Peek {
        /// How many videos to show
        #[arg(default_value = "1")]
        n: NonZeroUsize,
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
        #[arg(long, conflicts_with_all = ["week", "month", "year", "from", "to"])]
        all: bool,

        /// Filter to last 7 days
        #[arg(long, conflicts_with_all = ["all", "month", "year", "from", "to"])]
        week: bool,

        /// Last 30 days, or a specific month (YYYY-MM)
        #[arg(
            long,
            num_args = 0..=1,
            default_missing_value = "",
            value_name = "YYYY-MM",
            conflicts_with_all = ["all", "week", "year", "from", "to"]
        )]
        month: Option<String>,

        /// Last 365 days, or a specific year (YYYY)
        #[arg(
            long,
            num_args = 0..=1,
            default_missing_value = "",
            value_name = "YYYY",
            conflicts_with_all = ["all", "week", "month", "from", "to"]
        )]
        year: Option<String>,

        /// Start date for custom range (YYYY-MM-DD)
        #[arg(
            long,
            conflicts_with_all = ["all", "week", "month", "year"],
            value_name = "DATE"
        )]
        from: Option<String>,

        /// End date for custom range (YYYY-MM-DD)
        #[arg(
            long,
            conflicts_with_all = ["all", "week", "month", "year"],
            value_name = "DATE"
        )]
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
        #[arg(conflicts_with_all = ["queue", "history", "all"])]
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
        limit: Option<NonZeroUsize>,

        /// Force re-fetch metadata, including previously unavailable videos
        #[arg(long)]
        force: bool,

        /// Force refresh video categories
        #[arg(long)]
        refresh_categories: bool,
    },

    /// Open a random matching video and remove it from the queue
    #[command(alias = "r", alias = "lucky")]
    Random {
        #[command(flatten)]
        selection: selection::SelectionArgs,
    },
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
        Commands::Next { target, selection } => commands::next(target.as_deref(), &selection),
        Commands::Search {
            query,
            selection,
            limit,
            all,
        } => commands::search(query.as_deref(), &selection, (!all).then_some(limit.get())),
        Commands::List => commands::list(),
        Commands::Peek { n } => commands::peek(n.get()),
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
            limit.map(NonZeroUsize::get),
            force,
            refresh_categories,
        ),
        Commands::Random { selection } => commands::random(&selection),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_rejects_conflicting_periods() {
        assert!(Cli::try_parse_from(["ytq", "stats", "--all", "--week"]).is_err());
        assert!(
            Cli::try_parse_from(["ytq", "stats", "--month", "2025-01", "--year", "2025"]).is_err()
        );
    }

    #[test]
    fn fetch_rejects_target_with_scope() {
        assert!(Cli::try_parse_from(["ytq", "fetch", "dQw4w9WgXcQ", "--history"]).is_err());
    }

    #[test]
    fn positive_counts_are_required() {
        assert!(Cli::try_parse_from(["ytq", "peek", "0"]).is_err());
        assert!(Cli::try_parse_from(["ytq", "fetch", "--limit", "0"]).is_err());
    }
}

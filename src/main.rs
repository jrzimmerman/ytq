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

use anyhow::Result;
use chrono::NaiveDate;
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
        #[arg(value_parser = selection::parse_text)]
        query: Option<String>,

        #[command(flatten)]
        selection: selection::SelectionArgs,

        #[command(flatten)]
        count: selection::CountArgs,
    },

    /// List the current queue, oldest first
    #[command(alias = "l", alias = "ls")]
    List {
        #[command(flatten)]
        selection: selection::SelectionArgs,

        #[command(flatten)]
        count: selection::CountArgs,
    },

    /// Look at the next few videos without opening them
    #[command(alias = "k")]
    Peek {
        /// How many videos to show, without a notice about the rest [default: 1]
        #[arg(value_parser = selection::parse_count, conflicts_with_all = ["limit", "all"])]
        n: Option<usize>,

        #[command(flatten)]
        selection: selection::SelectionArgs,

        #[command(flatten)]
        count: selection::CountArgs,
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
        all_time: bool,

        /// Deprecated spelling of --all-time
        #[arg(long, hide = true, conflicts_with_all = ["week", "month", "year", "from", "to"])]
        all: bool,

        /// Filter to last 7 days
        #[arg(long, conflicts_with_all = ["all", "all_time", "month", "year", "from", "to"])]
        week: bool,

        /// Last 30 days, or a specific month (YYYY-MM)
        #[arg(
            long,
            value_name = "YYYY-MM",
            value_parser = stats::parse_month,
            conflicts_with_all = ["all", "all_time", "week", "year", "from", "to"]
        )]
        month: Option<Option<(i32, u32)>>,

        /// Last 365 days, or a specific year (YYYY)
        #[arg(
            long,
            value_name = "YYYY",
            value_parser = stats::parse_year,
            conflicts_with_all = ["all", "all_time", "week", "month", "from", "to"]
        )]
        year: Option<Option<i32>>,

        /// Start date for custom range (YYYY-MM-DD)
        #[arg(
            long,
            value_parser = stats::parse_date,
            conflicts_with_all = ["all", "all_time", "week", "month", "year"],
            value_name = "DATE"
        )]
        from: Option<NaiveDate>,

        /// End date for custom range (YYYY-MM-DD)
        #[arg(
            long,
            value_parser = stats::parse_date,
            conflicts_with_all = ["all", "all_time", "week", "month", "year"],
            value_name = "DATE"
        )]
        to: Option<NaiveDate>,
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

        /// Fetch for queue videos (default when no flags given)
        #[arg(long)]
        queue: bool,

        /// Fetch for history videos; combine with --queue for both
        #[arg(long)]
        history: bool,

        /// Deprecated spelling of --queue --history
        #[arg(long, hide = true)]
        all: bool,

        /// Maximum number of videos to fetch (useful for testing)
        #[arg(long, value_name = "N", value_parser = selection::parse_count)]
        limit: Option<usize>,

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

fn warn_deprecated(old: &str, new: &str) {
    eprintln!(
        "{} `{old}` is deprecated; use `{new}`.",
        "Warning:".yellow()
    );
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Add { input } => commands::add(&input),
        Commands::Next { target, selection } => commands::next(target.as_deref(), &selection),
        Commands::Search {
            query,
            selection,
            count,
        } => commands::search(query.as_deref(), &selection, &count),
        Commands::List { selection, count } => commands::list(&selection, &count),
        Commands::Peek {
            n,
            selection,
            count,
        } => commands::peek(n, &selection, &count),
        Commands::Remove { target } => commands::remove(&target),
        Commands::Stats {
            wrapped,
            all_time,
            all,
            week,
            month,
            year,
            from,
            to,
        } => {
            if all {
                warn_deprecated("ytq stats --all", "ytq stats --all-time");
            }
            commands::stats(wrapped, all_time || all, week, month, year, from, to)
        }
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
        } => {
            if all {
                warn_deprecated("ytq fetch --all", "ytq fetch --queue --history");
            }
            commands::fetch(
                target.as_deref(),
                queue || all,
                history || all,
                limit,
                force,
                refresh_categories,
            )
        }
        Commands::Random { selection } => commands::random(&selection),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_rejects_conflicting_periods() {
        assert!(Cli::try_parse_from(["ytq", "stats", "--all", "--week"]).is_err());
        assert!(Cli::try_parse_from(["ytq", "stats", "--all-time", "--week"]).is_err());
        assert!(Cli::try_parse_from(["ytq", "stats", "--all-time", "--year", "2025"]).is_err());
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

    #[test]
    fn list_and_peek_accept_filters_and_count_flags() {
        assert!(Cli::try_parse_from(["ytq", "list", "--max-duration", "10m"]).is_ok());
        assert!(Cli::try_parse_from(["ytq", "peek", "3", "--max-duration", "10m"]).is_ok());
        assert!(Cli::try_parse_from(["ytq", "peek", "--max-duration", "10m", "3"]).is_ok());
        assert!(Cli::try_parse_from(["ytq", "list", "--category", "tech"]).is_ok());
        assert!(Cli::try_parse_from(["ytq", "peek", "--channel", "talks"]).is_ok());
        assert!(Cli::try_parse_from(["ytq", "list", "--limit", "3"]).is_ok());
        assert!(Cli::try_parse_from(["ytq", "peek", "--limit", "3"]).is_ok());
        assert!(Cli::try_parse_from(["ytq", "peek", "--all"]).is_ok());
    }

    #[test]
    fn peek_shorthand_conflicts_with_count_flags() {
        assert!(Cli::try_parse_from(["ytq", "peek", "3", "--limit", "3"]).is_err());
        assert!(Cli::try_parse_from(["ytq", "peek", "3", "--all"]).is_err());
    }
}

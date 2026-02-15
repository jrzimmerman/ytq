use std::collections::HashMap;

use crate::models::{Action, Event, Mode, Video, VideoMeta};
use crate::stats::DateRange;
use crate::{paths, stats, store, youtube, youtube_api};

use anyhow::{Result, bail};
use chrono::{DateTime, Datelike, Local, NaiveDate, Utc};
use colored::Colorize;
use rand::RngExt;

pub fn add(input: &str) -> Result<()> {
    let paths = paths::AppPaths::init()?;

    // Normalize input before acquiring lock
    let id = youtube::extract_video_id(input)?;
    let url = youtube::build_canonical_url(&id);

    let added = store::with_queue(&paths, |queue| {
        // Deduplicate
        if queue.iter().any(|v| v.id == id) {
            return Ok(false);
        }

        let video = Video {
            id: id.clone(),
            url: url.clone(),
            added_at: Utc::now(),
        };

        queue.push(video);
        Ok(true)
    })?;

    if added {
        let event = Event {
            timestamp: Utc::now(),
            action: Action::Queued,
            video_id: id.clone(),
            time_in_queue_sec: None,
        };
        store::log_event(&paths.history_dir, &event)?;

        println!("{} {id}", "Added:".green());

        // Hint about fetching metadata when online features are enabled
        let cfg = store::load_config(&paths.config_file);
        if !cfg.offline {
            println!("  Run {} to get video metadata.", "`ytq fetch`".bold());
        }
    } else {
        println!("{} {input}", "Video already in queue:".yellow());
    }

    Ok(())
}

pub fn next(target: Option<&str>) -> Result<()> {
    let paths = paths::AppPaths::init()?;
    let cfg = store::load_config(&paths.config_file);

    // If a specific target is provided, parse it before acquiring the lock
    let target_id = target.map(youtube::extract_video_id).transpose()?;

    // Remove the video from queue while holding the lock
    let video = store::with_queue(&paths, |queue| {
        if queue.is_empty() {
            return Ok(None);
        }

        let video = match &target_id {
            // Specific video requested - find by ID
            Some(id) => {
                let idx = queue
                    .iter()
                    .position(|v| v.id == *id)
                    .ok_or_else(|| anyhow::anyhow!("video with ID '{id}' not found in queue"))?;
                queue.remove(idx)
            }
            // No target - use mode-based selection
            None => match cfg.mode {
                Mode::Queue => queue.remove(0),
                Mode::Stack => queue.pop().expect("queue verified non-empty"),
            },
        };

        Ok(Some(video))
    })?;

    let Some(video) = video else {
        println!("{}", "Queue is empty.".yellow());
        return Ok(());
    };

    // Log event and open video (outside the lock)
    let duration = Utc::now().signed_duration_since(video.added_at);
    let sec_in_queue = duration.num_seconds();

    let event = Event {
        timestamp: Utc::now(),
        action: Action::Watched,
        video_id: video.id.clone(),
        time_in_queue_sec: Some(sec_in_queue),
    };

    store::log_event(&paths.history_dir, &event)?;

    println!("{} {}", "Opening:".blue(), video.url);
    open::that(&video.url)?;

    Ok(())
}

pub fn remove(target: &str) -> Result<()> {
    let paths = paths::AppPaths::init()?;

    // Extract ID from input before acquiring lock
    let target_id = youtube::extract_video_id(target)?;

    let video = store::with_queue(&paths, |queue| {
        if queue.is_empty() {
            return Ok(None);
        }

        // Find by ID
        let idx = queue
            .iter()
            .position(|v| v.id == target_id)
            .ok_or_else(|| anyhow::anyhow!("video with ID '{target_id}' not found in queue"))?;

        Ok(Some(queue.remove(idx)))
    })?;

    let Some(video) = video else {
        println!("{}", "Queue is empty.".yellow());
        return Ok(());
    };

    let event = Event {
        timestamp: Utc::now(),
        action: Action::Skipped,
        video_id: video.id.clone(),
        time_in_queue_sec: None,
    };
    store::log_event(&paths.history_dir, &event)?;

    println!("{} {}", "Removed:".red(), video.id);
    Ok(())
}

pub fn list() -> Result<()> {
    let paths = paths::AppPaths::init()?;
    let cfg = store::load_config(&paths.config_file);

    // Load metadata if online mode is enabled
    let metadata = if !cfg.offline {
        store::load_metadata(&paths.metadata_file)
    } else {
        HashMap::new()
    };

    store::with_queue_read(&paths, |queue| {
        if queue.is_empty() {
            println!("{}", "Queue is empty.".yellow());
            return;
        }

        println!("{} videos in queue:", queue.len());

        if cfg.offline {
            print_list_offline(queue);
        } else {
            print_list_online(queue, &metadata);
        }
    })
}

fn print_list_offline(queue: &[Video]) {
    // Header
    println!("  {:<4} {:<13} Added", "#", "ID");
    for (i, v) in queue.iter().enumerate() {
        let local_time: DateTime<Local> = DateTime::from(v.added_at);
        println!(
            "  {:<4} {:<13} {}",
            i + 1,
            v.id,
            local_time.format("%Y-%m-%d %H:%M")
        );
    }
}

fn print_list_online(queue: &[Video], metadata: &HashMap<String, VideoMeta>) {
    let hint_fetch = "(run `ytq fetch`)";
    let hint_unavailable = "(unavailable - consider `ytq rm`)";

    // Compute dynamic column widths based on content
    let title_width = queue
        .iter()
        .map(|v| match metadata.get(&v.id) {
            Some(m) if m.unavailable => hint_unavailable.len(),
            Some(m) => m.title.chars().count(),
            None => hint_fetch.len(),
        })
        .max()
        .unwrap_or(5)
        .min(50); // cap at 50 chars

    let channel_width = queue
        .iter()
        .filter_map(|v| metadata.get(&v.id))
        .filter(|m| !m.unavailable)
        .map(|m| m.channel.chars().count())
        .max()
        .unwrap_or(7)
        .min(25); // cap at 25 chars

    // Header
    println!(
        "  {:<4} {:<13} {:<title_w$}  {:<chan_w$}  {:<8}  Added",
        "#",
        "ID",
        "Title",
        "Channel",
        "Duration",
        title_w = title_width,
        chan_w = channel_width,
    );

    for (i, v) in queue.iter().enumerate() {
        let local_time: DateTime<Local> = DateTime::from(v.added_at);
        let added = local_time.format("%Y-%m-%d %H:%M").to_string();

        match metadata.get(&v.id) {
            Some(meta) if meta.unavailable => {
                println!(
                    "  {:<4} {:<13} {:<title_w$}  {:<chan_w$}  {:<8}  {}",
                    i + 1,
                    v.id,
                    hint_unavailable,
                    "",
                    "",
                    added,
                    title_w = title_width,
                    chan_w = channel_width,
                );
            }
            Some(meta) => {
                let title = truncate(&meta.title, title_width);
                let channel = truncate(&meta.channel, channel_width);
                let duration = youtube_api::format_duration(meta.duration_seconds);

                println!(
                    "  {:<4} {:<13} {:<title_w$}  {:<chan_w$}  {:<8}  {}",
                    i + 1,
                    v.id,
                    title,
                    channel,
                    duration,
                    added,
                    title_w = title_width,
                    chan_w = channel_width,
                );
            }
            None => {
                println!(
                    "  {:<4} {:<13} {:<title_w$}  {:<chan_w$}  {:<8}  {}",
                    i + 1,
                    v.id,
                    hint_fetch,
                    "",
                    "",
                    added,
                    title_w = title_width,
                    chan_w = channel_width,
                );
            }
        }
    }
}

/// Truncates a string to a maximum character width, appending "..." if truncated.
fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max.saturating_sub(3)).collect();
        format!("{truncated}...")
    }
}

pub fn peek(n: usize) -> Result<()> {
    let paths = paths::AppPaths::init()?;
    let cfg = store::load_config(&paths.config_file);

    let metadata = if !cfg.offline {
        store::load_metadata(&paths.metadata_file)
    } else {
        HashMap::new()
    };

    store::with_queue_read(&paths, |queue| {
        if queue.is_empty() {
            println!("{}", "Queue is empty.".yellow());
            return;
        }

        // Collect the slice based on mode
        let slice: Vec<&Video> = match cfg.mode {
            Mode::Queue => queue.iter().take(n).collect(),
            Mode::Stack => queue.iter().rev().take(n).collect(),
        };

        let actual = slice.len();
        println!("Next {actual} video(s) ({:?} mode):", cfg.mode);

        // Reuse the same tabular format as list
        let videos: Vec<Video> = slice.into_iter().cloned().collect();
        if cfg.offline {
            print_list_offline(&videos);
        } else {
            print_list_online(&videos, &metadata);
        }
    })
}

pub fn stats(
    wrapped: bool,
    all: bool,
    week: bool,
    month: Option<String>,
    year: Option<String>,
    from: Option<String>,
    to: Option<String>,
) -> Result<()> {
    let paths = paths::AppPaths::init()?;

    // Resolve date range from flags
    let range = resolve_date_range(all, week, month, year, from, to)?;

    // Load events and filter by date range
    let all_events = store::stream_history(&paths.history_dir);
    let filtered = stats::filter_events(&all_events, &range);

    // Load metadata opportunistically (no network requests)
    let metadata = store::load_metadata(&paths.metadata_file);
    let categories = store::load_categories(&paths.categories_file);

    // Get current queue video IDs for queue profile stats
    let queue_ids = store::with_queue_read(&paths, |queue| {
        queue.iter().map(|v| v.id.clone()).collect::<Vec<_>>()
    })?;

    // Check whether we have any useful metadata (for queue or watched videos)
    let has_metadata = queue_ids
        .iter()
        .any(|id| metadata.get(id).is_some_and(|m| !m.unavailable))
        || filtered
            .iter()
            .filter(|e| matches!(e.action, Action::Watched))
            .any(|e| metadata.get(&e.video_id).is_some_and(|m| !m.unavailable));

    if wrapped {
        let report = stats::compute_wrapped(&filtered, &queue_ids, &metadata, &categories, &range);
        stats::print_wrapped(&report, &range, has_metadata);
    } else {
        let report = stats::compute_basic(&filtered, &queue_ids, &metadata);
        stats::print_basic(&report, &range, has_metadata);
    }

    Ok(())
}

/// Resolves CLI flags into a DateRange. Flags are already mutually exclusive
/// via clap's `conflicts_with_all`, so at most one period flag is set.
///
/// When no date flags are given, defaults to the current year unless `--all`
/// is passed.
fn resolve_date_range(
    all: bool,
    week: bool,
    month: Option<String>,
    year: Option<String>,
    from: Option<String>,
    to: Option<String>,
) -> Result<DateRange> {
    if week {
        return Ok(DateRange::last_days(7));
    }

    if let Some(val) = month {
        if val.is_empty() {
            // --month with no value => last 30 days
            return Ok(DateRange::last_days(30));
        }
        // --month YYYY-MM
        let parts: Vec<&str> = val.split('-').collect();
        if parts.len() != 2 {
            bail!("invalid month format '{val}': expected YYYY-MM");
        }
        let y: i32 = parts[0]
            .parse()
            .map_err(|_| anyhow::anyhow!("invalid year in '{val}'"))?;
        let m: u32 = parts[1]
            .parse()
            .map_err(|_| anyhow::anyhow!("invalid month in '{val}'"))?;
        return DateRange::specific_month(y, m)
            .ok_or_else(|| anyhow::anyhow!("invalid month: {val}"));
    }

    if let Some(val) = year {
        if val.is_empty() {
            // --year with no value => last 365 days
            return Ok(DateRange::last_days(365));
        }
        // --year YYYY
        let y: i32 = val
            .parse()
            .map_err(|_| anyhow::anyhow!("invalid year: '{val}'"))?;
        return DateRange::specific_year(y).ok_or_else(|| anyhow::anyhow!("invalid year: {val}"));
    }

    if from.is_some() || to.is_some() {
        let from_date = from
            .map(|s| {
                NaiveDate::parse_from_str(&s, "%Y-%m-%d")
                    .map_err(|_| anyhow::anyhow!("invalid --from date '{s}': expected YYYY-MM-DD"))
            })
            .transpose()?;
        let to_date = to
            .map(|s| {
                NaiveDate::parse_from_str(&s, "%Y-%m-%d")
                    .map_err(|_| anyhow::anyhow!("invalid --to date '{s}': expected YYYY-MM-DD"))
            })
            .transpose()?;
        return Ok(DateRange::custom(from_date, to_date));
    }

    if all {
        return Ok(DateRange::all_time());
    }

    // Default: current year (in local time)
    let current_year = Local::now().year();
    DateRange::specific_year(current_year)
        .ok_or_else(|| anyhow::anyhow!("failed to build date range for current year"))
}

pub fn config(key: &str, value: &str) -> Result<()> {
    let paths = paths::AppPaths::init()?;
    let mut cfg = store::load_config(&paths.config_file);

    match key {
        "mode" => match value.to_lowercase().as_str() {
            "stack" => cfg.mode = Mode::Stack,
            "queue" => cfg.mode = Mode::Queue,
            _ => bail!("invalid mode '{value}': use 'stack' or 'queue'"),
        },
        "offline" => match value.to_lowercase().as_str() {
            "true" => cfg.offline = true,
            "false" => cfg.offline = false,
            _ => bail!("invalid offline value '{value}': use 'true' or 'false'"),
        },
        "youtube_api_key" => {
            cfg.youtube_api_key = Some(value.to_string());
        }
        _ => bail!(
            "unknown config key '{key}': available keys are 'mode', 'offline', 'youtube_api_key'"
        ),
    }

    store::save_config(&paths.config_file, &cfg)?;
    println!("{}", "Config updated.".green());
    Ok(())
}

pub fn info() -> Result<()> {
    let paths = paths::AppPaths::init()?;

    println!("{}", "Data Paths".bold());
    println!("---------------");
    println!("Config:     {}", paths.config_file.display());
    println!("Queue:      {}", paths.queue_file.display());
    println!("Metadata:   {}", paths.metadata_file.display());
    println!("Categories: {}", paths.categories_file.display());
    println!("History:    {}", paths.history_dir.display());

    let queue_exists = paths.queue_file.exists();
    println!("Queue File Exists? {queue_exists}");

    Ok(())
}

pub fn fetch(
    target: Option<&str>,
    queue_flag: bool,
    history_flag: bool,
    all_flag: bool,
    limit: Option<usize>,
    force: bool,
    refresh_categories: bool,
) -> Result<()> {
    let paths = paths::AppPaths::init()?;
    let cfg = store::load_config(&paths.config_file);

    // Check offline mode
    if cfg.offline {
        bail!("online features are disabled. Run `ytq config offline false` to enable.");
    }

    // Resolve API key
    let api_key = cfg.effective_api_key().ok_or_else(|| {
        anyhow::anyhow!(
            "no YouTube Data API key configured.\n\
             Set it via: ytq config youtube_api_key <key>\n\
             Or set the YOUTUBE_DATA_API_KEY environment variable."
        )
    })?;

    // Fetch and save video categories if missing or explicitly requested
    if refresh_categories || !paths.categories_file.exists() {
        match youtube_api::fetch_categories(&api_key) {
            Ok(categories) => {
                store::save_categories(&paths.categories_file, &categories)?;
                eprintln!("Updated {} video categories.", categories.len());
            }
            Err(e) => {
                eprintln!("{} Failed to fetch categories: {e:#}", "Warning:".yellow());
            }
        }
    }

    // Determine whether this is a targeted or scope-based fetch.
    // Targeted fetches (explicit IDs) always force-refresh.
    let (mut ids_to_fetch, is_targeted) = if let Some(input) = target {
        // Parse comma-separated IDs/URLs
        let ids: Vec<String> = input
            .split(',')
            .map(|s| youtube::extract_video_id(s.trim()))
            .collect::<Result<Vec<_>>>()?;
        (ids, true)
    } else {
        let ids = collect_ids_for_scope(&paths, queue_flag, history_flag, all_flag)?;
        (ids, false)
    };

    // Deduplicate collected IDs
    ids_to_fetch.sort();
    ids_to_fetch.dedup();

    // Load existing metadata
    let mut metadata = store::load_metadata(&paths.metadata_file);

    // For scope-based fetches, filter out IDs that already have metadata.
    // Also skip unavailable (tombstone) entries unless --force is passed.
    // Targeted fetches (explicit IDs) always bypass the diff.
    if !is_targeted {
        ids_to_fetch.retain(|id| {
            match metadata.get(id) {
                None => true,                              // not in metadata, fetch it
                Some(m) if m.unavailable && force => true, // tombstone + --force, retry
                Some(m) if m.unavailable => false,         // tombstone, skip
                Some(_) if force => true,                  // has metadata + --force, re-fetch
                Some(_) => false,                          // has metadata, skip
            }
        });
    }

    // Apply limit
    if let Some(max) = limit {
        ids_to_fetch.truncate(max);
    }

    if ids_to_fetch.is_empty() {
        println!("{}", "All metadata is up to date.".green());
        return Ok(());
    }

    println!("Fetching metadata for {} video(s)...", ids_to_fetch.len());

    let fetched = youtube_api::fetch_video_metadata(&ids_to_fetch, &api_key)?;
    let count = fetched.len();

    // Identify which IDs were not returned by the API
    let fetched_ids: std::collections::HashSet<&str> =
        fetched.iter().map(|m| m.id.as_str()).collect();
    let missing_ids: Vec<&str> = ids_to_fetch
        .iter()
        .filter(|id| !fetched_ids.contains(id.as_str()))
        .map(|id| id.as_str())
        .collect();

    // Merge fetched entries into existing metadata (upsert)
    for meta in fetched {
        metadata.insert(meta.id.clone(), meta);
    }

    // Store tombstone entries for videos the API returned nothing for
    let now = Utc::now();
    for id in &missing_ids {
        metadata.insert(
            id.to_string(),
            VideoMeta {
                id: id.to_string(),
                title: String::new(),
                channel: String::new(),
                channel_id: String::new(),
                duration: String::new(),
                duration_seconds: 0,
                published_at: now,
                category_id: String::new(),
                tags: vec![],
                fetched_at: now,
                unavailable: true,
            },
        );
    }

    store::save_metadata(&paths.metadata_file, &metadata)?;

    println!("{} Fetched metadata for {count} video(s).", "Done.".green());

    if !missing_ids.is_empty() {
        println!(
            "{} {} video(s) returned no metadata (may be private, age-restricted, or deleted):",
            "Note:".yellow(),
            missing_ids.len()
        );
        for id in &missing_ids {
            println!("  - {id}");
        }
    }

    Ok(())
}

/// Collects video IDs based on the scope flags.
/// Default (no flags) behaves as --queue.
fn collect_ids_for_scope(
    paths: &paths::AppPaths,
    queue_flag: bool,
    history_flag: bool,
    all_flag: bool,
) -> Result<Vec<String>> {
    let mut ids = Vec::new();

    // Default to queue-only when no flags are given
    let use_queue = all_flag || queue_flag || (!history_flag);
    let use_history = all_flag || history_flag;

    if use_queue {
        store::with_queue_read(paths, |queue| {
            for v in queue {
                ids.push(v.id.clone());
            }
        })?;
    }

    if use_history {
        let events = store::stream_history(&paths.history_dir);
        for event in &events {
            ids.push(event.video_id.clone());
        }
    }

    Ok(ids)
}

pub fn random() -> Result<()> {
    let paths = paths::AppPaths::init()?;

    let video = store::with_queue(&paths, |queue| {
        if queue.is_empty() {
            return Ok(None);
        }

        let idx = rand::rng().random_range(0..queue.len());
        Ok(Some(queue.remove(idx)))
    })?;

    let Some(video) = video else {
        println!("{}", "Queue is empty.".yellow());
        return Ok(());
    };

    // Log event and open video
    let duration = Utc::now().signed_duration_since(video.added_at);
    let event = Event {
        timestamp: Utc::now(),
        action: Action::Watched,
        video_id: video.id.clone(),
        time_in_queue_sec: Some(duration.num_seconds()),
    };
    store::log_event(&paths.history_dir, &event)?;

    println!("{} {}", "Opening:".blue(), video.url);
    open::that(&video.url)?;

    Ok(())
}

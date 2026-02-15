use std::collections::HashMap;

use crate::models::{Action, Event, VideoMeta};
use crate::youtube_api;

use chrono::{DateTime, Datelike, Local, NaiveDate, TimeDelta, Timelike, Utc, Weekday};
use colored::Colorize;

// ---------------------------------------------------------------------------
// Local time conversion
// ---------------------------------------------------------------------------

/// Converts a UTC timestamp to a local DateTime for user-facing grouping
/// (time-of-day, weekday, date).
fn to_local(ts: &DateTime<Utc>) -> DateTime<Local> {
    DateTime::<Local>::from(*ts)
}

// ---------------------------------------------------------------------------
// DateRange — period filtering
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct DateRange {
    pub start: Option<DateTime<Utc>>,
    pub end: Option<DateTime<Utc>>,
}

impl DateRange {
    /// No filtering — all-time.
    pub fn all_time() -> Self {
        Self {
            start: None,
            end: None,
        }
    }

    /// Last N days from now.
    pub fn last_days(n: i64) -> Self {
        let start = Utc::now() - TimeDelta::days(n);
        Self {
            start: Some(start),
            end: None,
        }
    }

    /// A specific month: YYYY-MM.
    pub fn specific_month(year: i32, month: u32) -> Option<Self> {
        let start = NaiveDate::from_ymd_opt(year, month, 1)?
            .and_hms_opt(0, 0, 0)?
            .and_utc();
        let end = if month == 12 {
            NaiveDate::from_ymd_opt(year + 1, 1, 1)?
        } else {
            NaiveDate::from_ymd_opt(year, month + 1, 1)?
        }
        .and_hms_opt(0, 0, 0)?
        .and_utc();
        Some(Self {
            start: Some(start),
            end: Some(end),
        })
    }

    /// A specific year: YYYY.
    pub fn specific_year(year: i32) -> Option<Self> {
        let start = NaiveDate::from_ymd_opt(year, 1, 1)?
            .and_hms_opt(0, 0, 0)?
            .and_utc();
        let end = NaiveDate::from_ymd_opt(year + 1, 1, 1)?
            .and_hms_opt(0, 0, 0)?
            .and_utc();
        Some(Self {
            start: Some(start),
            end: Some(end),
        })
    }

    /// Custom range with optional start/end.
    pub fn custom(from: Option<NaiveDate>, to: Option<NaiveDate>) -> Self {
        Self {
            start: from.and_then(|d| d.and_hms_opt(0, 0, 0).map(|dt| dt.and_utc())),
            end: to.and_then(|d| d.and_hms_opt(0, 0, 0).map(|dt| dt.and_utc())),
        }
    }

    /// Returns true if the timestamp falls within this range.
    pub fn contains(&self, ts: &DateTime<Utc>) -> bool {
        if let Some(start) = &self.start
            && ts < start
        {
            return false;
        }
        if let Some(end) = &self.end
            && ts >= end
        {
            return false;
        }
        true
    }

    /// Returns a human-readable label for the period.
    pub fn label(&self) -> String {
        match (&self.start, &self.end) {
            (None, None) => "All Time".to_string(),
            (Some(s), None) => format!("Since {}", s.format("%Y-%m-%d")),
            (None, Some(e)) => format!("Before {}", e.format("%Y-%m-%d")),
            (Some(s), Some(e)) => {
                format!("{} to {}", s.format("%Y-%m-%d"), e.format("%Y-%m-%d"))
            }
        }
    }
}

/// Filters events to those within the given date range.
pub fn filter_events<'a>(events: &'a [Event], range: &DateRange) -> Vec<&'a Event> {
    events
        .iter()
        .filter(|e| range.contains(&e.timestamp))
        .collect()
}

// ---------------------------------------------------------------------------
// Basic stats computation
// ---------------------------------------------------------------------------

pub struct BasicStats {
    pub added: usize,
    pub watched: usize,
    pub skipped: usize,
    pub queue_depth: usize,
    pub completion_rate: f64,
    pub avg_time_in_queue_secs: Option<f64>,
    pub most_active_weekday: Option<(Weekday, usize)>,
    // Watch history metadata (deduplicated by video ID)
    pub total_watch_time_secs: Option<u64>,
    pub top_watched_channels: Vec<(String, usize)>,
    // Queue profile metadata
    pub queue_total_duration_secs: Option<u64>,
    pub top_queue_channels: Vec<(String, usize)>,
}

pub fn compute_basic(
    events: &[&Event],
    queue_ids: &[String],
    metadata: &HashMap<String, VideoMeta>,
) -> BasicStats {
    let added = events
        .iter()
        .filter(|e| matches!(e.action, Action::Queued))
        .count();
    let watched = events
        .iter()
        .filter(|e| matches!(e.action, Action::Watched))
        .count();
    let skipped = events
        .iter()
        .filter(|e| matches!(e.action, Action::Skipped))
        .count();

    let removed = watched + skipped;
    let completion_rate = if removed > 0 {
        watched as f64 / removed as f64
    } else {
        0.0
    };

    // Average time in queue (from watched events that have time_in_queue_sec)
    let queue_times: Vec<i64> = events
        .iter()
        .filter(|e| matches!(e.action, Action::Watched))
        .filter_map(|e| e.time_in_queue_sec)
        .collect();
    let avg_time_in_queue_secs = if queue_times.is_empty() {
        None
    } else {
        let sum: i64 = queue_times.iter().sum();
        Some(sum as f64 / queue_times.len() as f64)
    };

    // Most active weekday for adding videos
    let most_active_weekday = most_active_weekday_for(events, &Action::Queued);

    // Watch history: deduplicate watched IDs for metadata stats
    let unique_watched_ids = unique_ids_for_action(events, &Action::Watched);
    let watched_refs: Vec<&str> = unique_watched_ids.iter().map(|s| s.as_str()).collect();

    let has_watch_metadata = watched_refs
        .iter()
        .any(|id| metadata.get(*id).is_some_and(|m| !m.unavailable));

    let total_watch_time_secs = if has_watch_metadata {
        Some(
            watched_refs
                .iter()
                .filter_map(|id| metadata.get(*id))
                .filter(|m| !m.unavailable)
                .map(|m| m.duration_seconds)
                .sum(),
        )
    } else {
        None
    };

    let top_watched_channels = if has_watch_metadata {
        top_channels_from(&watched_refs, metadata, 3)
    } else {
        vec![]
    };

    // Queue profile: use queue video IDs joined against metadata
    let queue_refs: Vec<&str> = queue_ids.iter().map(|s| s.as_str()).collect();
    let has_queue_metadata = queue_refs
        .iter()
        .any(|id| metadata.get(*id).is_some_and(|m| !m.unavailable));

    let queue_total_duration_secs = if has_queue_metadata {
        Some(
            queue_refs
                .iter()
                .filter_map(|id| metadata.get(*id))
                .filter(|m| !m.unavailable)
                .map(|m| m.duration_seconds)
                .sum(),
        )
    } else {
        None
    };

    let top_queue_channels = if has_queue_metadata {
        top_channels_from(&queue_refs, metadata, 3)
    } else {
        vec![]
    };

    BasicStats {
        added,
        watched,
        skipped,
        queue_depth: queue_ids.len(),
        completion_rate,
        avg_time_in_queue_secs,
        most_active_weekday,
        total_watch_time_secs,
        top_watched_channels,
        queue_total_duration_secs,
        top_queue_channels,
    }
}

// ---------------------------------------------------------------------------
// Wrapped stats computation
// ---------------------------------------------------------------------------

pub struct MonthBucket {
    pub label: String, // "2025-06"
    pub count: usize,
}

pub struct TimeOfDayBucket {
    pub label: &'static str,
    pub count: usize,
}

pub struct CategoryPhase {
    pub period_label: String,
    pub category: String,
}

pub struct WrappedStats {
    // Includes all basic stats
    pub basic: BasicStats,

    // Monthly trends
    pub added_by_month: Vec<MonthBucket>,
    pub watched_by_month: Vec<MonthBucket>,

    // Time of day distribution (for watched events)
    pub time_of_day: Vec<TimeOfDayBucket>,

    // Busiest single day
    pub busiest_day: Option<(NaiveDate, usize)>,

    // Longest watch streak (consecutive days with at least one watch)
    pub longest_streak: usize,

    // Queue profile (from queue IDs + metadata)
    pub queue_top_channels: Vec<(String, usize)>,
    pub queue_categories: Vec<(String, usize)>,
    pub queue_top_tags: Vec<(String, usize)>,
    pub queue_avg_duration_secs: Option<u64>,

    // Watch history (from deduplicated watched IDs + metadata)
    pub watched_top_channels: Vec<(String, usize)>,
    pub watched_categories: Vec<(String, usize)>,
    pub watched_top_tags: Vec<(String, usize)>,
    pub watched_avg_duration_secs: Option<u64>,
    pub longest_video: Option<VideoDurationInfo>,
    pub shortest_video: Option<VideoDurationInfo>,

    // Skip rate
    pub skip_rate: f64,

    // Queue time extremes
    pub fastest_watch_secs: Option<i64>,
    pub slowest_watch_secs: Option<i64>,

    // Queue throughput (watches per week)
    pub watches_per_week: Option<f64>,

    // --- Fun Wrapped Insights ---

    // Viewer personality type: (label, description)
    pub viewer_personality: Option<(&'static str, &'static str)>,

    // Channel loyalty: (channel_name, percentage of watches)
    pub channel_loyalty: Option<(String, f64)>,

    // "Watching Age": average published year of watched videos
    pub watching_age: Option<i32>,

    // Discovery day: the day with the most unique channels watched
    pub discovery_day: Option<(NaiveDate, usize)>,

    // Category evolution: dominant category per time period
    pub category_evolution: Vec<CategoryPhase>,

    // Comfort video: most re-watched video (id, title, watch_count)
    pub comfort_video: Option<(String, String, usize)>,

    // Queue patience: (fun label, median time-in-queue secs)
    pub queue_patience: Option<(&'static str, i64)>,

    // Total throughput: unique videos that passed through the queue
    pub total_throughput: usize,

    // Oldest video watched by published_at: (id, title, published_at)
    pub oldest_video: Option<(String, String, DateTime<Utc>)>,

    // Weekend vs weekday: (fun label, weekend ratio 0.0-1.0)
    pub weekend_vs_weekday: Option<(&'static str, f64)>,
}

pub fn compute_wrapped(
    events: &[&Event],
    queue_ids: &[String],
    metadata: &HashMap<String, VideoMeta>,
    categories: &HashMap<String, String>,
    range: &DateRange,
) -> WrappedStats {
    let basic = compute_basic(events, queue_ids, metadata);

    // Deduplicated watched IDs for metadata stats
    let unique_watched_ids = unique_ids_for_action(events, &Action::Watched);
    let watched_refs: Vec<&str> = unique_watched_ids.iter().map(|s| s.as_str()).collect();
    let has_watch_meta = watched_refs
        .iter()
        .any(|id| metadata.get(*id).is_some_and(|m| !m.unavailable));

    // Queue IDs for queue profile
    let queue_refs: Vec<&str> = queue_ids.iter().map(|s| s.as_str()).collect();
    let has_queue_meta = queue_refs
        .iter()
        .any(|id| metadata.get(*id).is_some_and(|m| !m.unavailable));

    // Monthly trends
    let added_by_month = monthly_buckets(events, &Action::Queued);
    let watched_by_month = monthly_buckets(events, &Action::Watched);

    // Time of day distribution (for watched events)
    let time_of_day = time_of_day_distribution(events);

    // Busiest single day (by watched count)
    let busiest_day = busiest_day_for(events, &Action::Watched);

    // Longest watch streak
    let longest_streak = longest_streak(events);

    // Queue profile
    let queue_top_channels = if has_queue_meta {
        top_channels_from(&queue_refs, metadata, 10)
    } else {
        vec![]
    };
    let queue_categories = if has_queue_meta {
        category_breakdown_from(&queue_refs, metadata, categories)
    } else {
        vec![]
    };
    let queue_top_tags = if has_queue_meta {
        top_tags_from(&queue_refs, metadata, 10)
    } else {
        vec![]
    };
    let queue_avg_duration_secs = if has_queue_meta {
        let (avg, _, _) = duration_stats(&queue_refs, metadata);
        avg
    } else {
        None
    };

    // Watch history profile
    let watched_top_channels = if has_watch_meta {
        top_channels_from(&watched_refs, metadata, 10)
    } else {
        vec![]
    };
    let watched_categories = if has_watch_meta {
        category_breakdown_from(&watched_refs, metadata, categories)
    } else {
        vec![]
    };
    let watched_top_tags = if has_watch_meta {
        top_tags_from(&watched_refs, metadata, 10)
    } else {
        vec![]
    };
    let (watched_avg_duration_secs, longest_video, shortest_video) = if has_watch_meta {
        duration_stats(&watched_refs, metadata)
    } else {
        (None, None, None)
    };

    // Skip rate
    let removed = basic.watched + basic.skipped;
    let skip_rate = if removed > 0 {
        basic.skipped as f64 / removed as f64
    } else {
        0.0
    };

    // Queue time extremes
    let queue_times: Vec<i64> = events
        .iter()
        .filter(|e| matches!(e.action, Action::Watched))
        .filter_map(|e| e.time_in_queue_sec)
        .collect();
    let fastest_watch_secs = queue_times.iter().min().copied();
    let slowest_watch_secs = queue_times.iter().max().copied();

    // Watches per week
    let watches_per_week = compute_watches_per_week(events, range);

    // --- Fun Wrapped Insights ---

    let viewer_personality = compute_viewer_personality(
        events,
        &time_of_day,
        longest_streak,
        watches_per_week,
        skip_rate,
        queue_ids.len(),
        basic.watched,
        &watched_top_channels,
        &watched_categories,
    );

    let channel_loyalty = compute_channel_loyalty(&watched_top_channels, unique_watched_ids.len());

    let watching_age = if has_watch_meta {
        compute_watching_age(&watched_refs, metadata)
    } else {
        None
    };

    let discovery_day = if has_watch_meta {
        compute_discovery_day(events, metadata)
    } else {
        None
    };

    let category_evolution = if has_watch_meta {
        compute_category_evolution(events, metadata, categories, range)
    } else {
        vec![]
    };

    let comfort_video = compute_comfort_video(events, metadata);

    let queue_patience = compute_queue_patience(events);

    let total_throughput = compute_total_throughput(events);

    let oldest_video = if has_watch_meta {
        compute_oldest_video(&watched_refs, metadata)
    } else {
        None
    };

    let weekend_vs_weekday = compute_weekend_weekday(events);

    WrappedStats {
        basic,
        added_by_month,
        watched_by_month,
        time_of_day,
        busiest_day,
        longest_streak,
        queue_top_channels,
        queue_categories,
        queue_top_tags,
        queue_avg_duration_secs,
        watched_top_channels,
        watched_categories,
        watched_top_tags,
        watched_avg_duration_secs,
        longest_video,
        shortest_video,
        skip_rate,
        fastest_watch_secs,
        slowest_watch_secs,
        watches_per_week,
        viewer_personality,
        channel_loyalty,
        watching_age,
        discovery_day,
        category_evolution,
        comfort_video,
        queue_patience,
        total_throughput,
        oldest_video,
        weekend_vs_weekday,
    }
}

// ---------------------------------------------------------------------------
// Helper functions — aggregation
// ---------------------------------------------------------------------------

/// Returns deduplicated video IDs for a given action type, preserving first occurrence order.
fn unique_ids_for_action(events: &[&Event], action: &Action) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut ids = Vec::new();
    for e in events {
        if std::mem::discriminant(&e.action) == std::mem::discriminant(action)
            && seen.insert(e.video_id.clone())
        {
            ids.push(e.video_id.clone());
        }
    }
    ids
}

fn most_active_weekday_for(events: &[&Event], action: &Action) -> Option<(Weekday, usize)> {
    let mut counts: HashMap<Weekday, usize> = HashMap::new();
    for e in events {
        if std::mem::discriminant(&e.action) == std::mem::discriminant(action) {
            *counts.entry(to_local(&e.timestamp).weekday()).or_default() += 1;
        }
    }
    counts.into_iter().max_by_key(|(_, c)| *c)
}

fn top_channels_from(
    ids: &[&str],
    metadata: &HashMap<String, VideoMeta>,
    limit: usize,
) -> Vec<(String, usize)> {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for id in ids {
        if let Some(m) = metadata.get(*id)
            && !m.unavailable
            && !m.channel.is_empty()
        {
            *counts.entry(&m.channel).or_default() += 1;
        }
    }
    let mut sorted: Vec<(String, usize)> = counts
        .into_iter()
        .map(|(ch, c)| (ch.to_string(), c))
        .collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    sorted.truncate(limit);
    sorted
}

fn monthly_buckets(events: &[&Event], action: &Action) -> Vec<MonthBucket> {
    let mut counts: std::collections::BTreeMap<(i32, u32), usize> =
        std::collections::BTreeMap::new();
    for e in events {
        if std::mem::discriminant(&e.action) == std::mem::discriminant(action) {
            let local = to_local(&e.timestamp);
            let key = (local.year(), local.month());
            *counts.entry(key).or_default() += 1;
        }
    }
    counts
        .into_iter()
        .map(|((year, month), count)| {
            let label = format!("{year}-{month:02}");
            MonthBucket { label, count }
        })
        .collect()
}

fn time_of_day_distribution(events: &[&Event]) -> Vec<TimeOfDayBucket> {
    let mut morning = 0usize;
    let mut afternoon = 0usize;
    let mut evening = 0usize;
    let mut night = 0usize;

    for e in events {
        if matches!(e.action, Action::Watched) {
            match to_local(&e.timestamp).hour() {
                6..12 => morning += 1,
                12..17 => afternoon += 1,
                17..22 => evening += 1,
                _ => night += 1, // 22-5
            }
        }
    }

    vec![
        TimeOfDayBucket {
            label: "Morning (6am-12pm)",
            count: morning,
        },
        TimeOfDayBucket {
            label: "Afternoon (12-5pm)",
            count: afternoon,
        },
        TimeOfDayBucket {
            label: "Evening (5-10pm)",
            count: evening,
        },
        TimeOfDayBucket {
            label: "Night (10pm-6am)",
            count: night,
        },
    ]
}

fn busiest_day_for(events: &[&Event], action: &Action) -> Option<(NaiveDate, usize)> {
    let mut counts: HashMap<NaiveDate, usize> = HashMap::new();
    for e in events {
        if std::mem::discriminant(&e.action) == std::mem::discriminant(action) {
            let date = to_local(&e.timestamp).date_naive();
            *counts.entry(date).or_default() += 1;
        }
    }
    counts.into_iter().max_by_key(|(_, c)| *c)
}

fn longest_streak(events: &[&Event]) -> usize {
    let mut watch_dates: Vec<NaiveDate> = events
        .iter()
        .filter(|e| matches!(e.action, Action::Watched))
        .map(|e| to_local(&e.timestamp).date_naive())
        .collect();
    watch_dates.sort();
    watch_dates.dedup();

    if watch_dates.is_empty() {
        return 0;
    }

    let mut max_streak = 1usize;
    let mut current_streak = 1usize;

    for window in watch_dates.windows(2) {
        let diff = window[1].signed_duration_since(window[0]).num_days();
        if diff == 1 {
            current_streak += 1;
            max_streak = max_streak.max(current_streak);
        } else {
            current_streak = 1;
        }
    }

    max_streak
}

fn category_breakdown_from(
    ids: &[&str],
    metadata: &HashMap<String, VideoMeta>,
    categories: &HashMap<String, String>,
) -> Vec<(String, usize)> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for id in ids {
        if let Some(m) = metadata.get(*id)
            && !m.unavailable
            && !m.category_id.is_empty()
        {
            let name = categories
                .get(&m.category_id)
                .cloned()
                .unwrap_or_else(|| format!("Category {}", m.category_id));
            *counts.entry(name).or_default() += 1;
        }
    }
    let mut sorted: Vec<(String, usize)> = counts.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    sorted
}

fn top_tags_from(
    ids: &[&str],
    metadata: &HashMap<String, VideoMeta>,
    limit: usize,
) -> Vec<(String, usize)> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for id in ids {
        if let Some(m) = metadata.get(*id)
            && !m.unavailable
        {
            for tag in &m.tags {
                let normalized = tag.to_lowercase();
                *counts.entry(normalized).or_default() += 1;
            }
        }
    }
    let mut sorted: Vec<(String, usize)> = counts.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    sorted.truncate(limit);
    sorted
}

/// (id, title, duration_seconds)
type VideoDurationInfo = (String, String, u64);

fn duration_stats(
    ids: &[&str],
    metadata: &HashMap<String, VideoMeta>,
) -> (
    Option<u64>,
    Option<VideoDurationInfo>,
    Option<VideoDurationInfo>,
) {
    let durations: Vec<(&VideoMeta, u64)> = ids
        .iter()
        .filter_map(|id| metadata.get(*id))
        .filter(|m| !m.unavailable && m.duration_seconds > 0)
        .map(|m| (m, m.duration_seconds))
        .collect();

    if durations.is_empty() {
        return (None, None, None);
    }

    let total: u64 = durations.iter().map(|(_, d)| d).sum();
    let avg = total / durations.len() as u64;

    let longest = durations
        .iter()
        .max_by_key(|(_, d)| *d)
        .map(|(m, d)| (m.id.clone(), m.title.clone(), *d));

    let shortest = durations
        .iter()
        .min_by_key(|(_, d)| *d)
        .map(|(m, d)| (m.id.clone(), m.title.clone(), *d));

    (Some(avg), longest, shortest)
}

// ---------------------------------------------------------------------------
// Fun Wrapped insight helpers
// ---------------------------------------------------------------------------

/// Assigns a fun viewer personality based on behavior patterns.
#[allow(clippy::too_many_arguments)]
fn compute_viewer_personality(
    events: &[&Event],
    time_of_day: &[TimeOfDayBucket],
    longest_streak: usize,
    watches_per_week: Option<f64>,
    skip_rate: f64,
    queue_depth: usize,
    watched_count: usize,
    watched_top_channels: &[(String, usize)],
    watched_categories: &[(String, usize)],
) -> Option<(&'static str, &'static str)> {
    if watched_count == 0 && queue_depth == 0 {
        return None;
    }

    // Compute signals
    let night_heavy = time_of_day
        .iter()
        .find(|b| b.label.starts_with("Night"))
        .is_some_and(|night| {
            let total: usize = time_of_day.iter().map(|b| b.count).sum();
            total > 0 && (night.count as f64 / total as f64) > 0.4
        });

    let morning_heavy = time_of_day
        .iter()
        .find(|b| b.label.starts_with("Morning"))
        .is_some_and(|morning| {
            let total: usize = time_of_day.iter().map(|b| b.count).sum();
            total > 0 && (morning.count as f64 / total as f64) > 0.4
        });

    let high_streak = longest_streak >= 5;
    let high_throughput = watches_per_week.is_some_and(|w| w >= 5.0);
    let low_skip = skip_rate < 0.1;
    let stockpiler = queue_depth > 0 && watched_count > 0 && queue_depth > watched_count * 2;

    let channel_count = watched_top_channels.len();
    let top_channel_dominance = if watched_count > 0 {
        watched_top_channels
            .first()
            .map(|(_, c)| *c as f64 / watched_count as f64)
            .unwrap_or(0.0)
    } else {
        0.0
    };

    let diverse_categories = watched_categories.len() >= 4;

    let unique_channels_watched: usize = watched_top_channels.iter().map(|(_, c)| *c).sum();
    let _explores_many = channel_count >= 8;

    let queue_times: Vec<i64> = events
        .iter()
        .filter(|e| matches!(e.action, Action::Watched))
        .filter_map(|e| e.time_in_queue_sec)
        .collect();
    let fast_consumer = if !queue_times.is_empty() {
        let avg = queue_times.iter().sum::<i64>() as f64 / queue_times.len() as f64;
        avg < 3600.0 // less than 1 hour avg
    } else {
        false
    };

    // Priority-ordered personality assignment
    if night_heavy {
        Some(("The Night Owl", "Most of your watching happens after dark."))
    } else if morning_heavy {
        Some((
            "The Early Bird",
            "You start your day with videos before noon.",
        ))
    } else if high_streak && high_throughput {
        Some((
            "The Binger",
            "Long streaks and high volume — you can't stop watching.",
        ))
    } else if fast_consumer {
        Some((
            "The Speedrunner",
            "Queue it, watch it, done — you don't let videos sit.",
        ))
    } else if stockpiler {
        Some((
            "The Stockpiler",
            "Your queue grows faster than you can watch.",
        ))
    } else if top_channel_dominance > 0.5 && unique_channels_watched >= 3 {
        Some(("The Loyalist", "One channel owns your watch history."))
    } else if diverse_categories && channel_count >= 6 {
        Some((
            "The Explorer",
            "Diverse tastes across many channels and categories.",
        ))
    } else if low_skip && watched_count > 0 {
        Some(("The Curator", "You pick carefully and almost never skip."))
    } else if watched_count == 0 && queue_depth > 0 {
        Some(("The Collector", "All queue, no play — your time will come."))
    } else {
        Some(("The Balanced Viewer", "A healthy mix of watching habits."))
    }
}

/// Computes channel loyalty: what % of watched videos came from the top channel.
fn compute_channel_loyalty(
    watched_top_channels: &[(String, usize)],
    total_unique_watched: usize,
) -> Option<(String, f64)> {
    if total_unique_watched == 0 {
        return None;
    }
    let (name, count) = watched_top_channels.first()?;
    if *count < 2 {
        return None; // Need at least 2 to be meaningful
    }
    let ratio = *count as f64 / total_unique_watched as f64;
    Some((name.clone(), ratio))
}

/// Computes the "watching age" — average published year of watched videos.
fn compute_watching_age(
    watched_ids: &[&str],
    metadata: &HashMap<String, VideoMeta>,
) -> Option<i32> {
    let years: Vec<i32> = watched_ids
        .iter()
        .filter_map(|id| metadata.get(*id))
        .filter(|m| !m.unavailable)
        .map(|m| m.published_at.year())
        .collect();

    if years.is_empty() {
        return None;
    }

    let sum: i64 = years.iter().map(|y| *y as i64).sum();
    Some((sum / years.len() as i64) as i32)
}

/// Finds the day with the most unique channels watched (the "discovery day").
fn compute_discovery_day(
    events: &[&Event],
    metadata: &HashMap<String, VideoMeta>,
) -> Option<(NaiveDate, usize)> {
    let mut day_channels: HashMap<NaiveDate, std::collections::HashSet<String>> = HashMap::new();

    for e in events {
        if matches!(e.action, Action::Watched) {
            let date = to_local(&e.timestamp).date_naive();
            if let Some(m) = metadata.get(&e.video_id)
                && !m.unavailable
                && !m.channel.is_empty()
            {
                day_channels
                    .entry(date)
                    .or_default()
                    .insert(m.channel.clone());
            }
        }
    }

    day_channels
        .into_iter()
        .filter(|(_, channels)| channels.len() >= 2) // Only interesting if 2+ channels
        .max_by_key(|(_, channels)| channels.len())
        .map(|(date, channels)| (date, channels.len()))
}

/// Computes the dominant category per time period (quarters or halves).
fn compute_category_evolution(
    events: &[&Event],
    metadata: &HashMap<String, VideoMeta>,
    categories: &HashMap<String, String>,
    range: &DateRange,
) -> Vec<CategoryPhase> {
    let watch_events: Vec<&&Event> = events
        .iter()
        .filter(|e| matches!(e.action, Action::Watched))
        .collect();

    if watch_events.is_empty() {
        return vec![];
    }

    // Determine the time span
    let first_ts = range
        .start
        .unwrap_or(watch_events.first().unwrap().timestamp);
    let last_ts = range.end.unwrap_or(Utc::now());
    let span_days = (last_ts - first_ts).num_days();

    // Choose period size: quarters if >= 120 days, halves if >= 60, else skip
    let (period_count, period_labels): (usize, Vec<&str>) = if span_days >= 270 {
        (4, vec!["Q1", "Q2", "Q3", "Q4"])
    } else if span_days >= 120 {
        (2, vec!["First Half", "Second Half"])
    } else {
        return vec![];
    };

    let period_duration = (last_ts - first_ts) / period_count as i32;

    let mut phases = Vec::new();

    for (i, period_label) in period_labels.iter().enumerate().take(period_count) {
        let period_start = first_ts + period_duration * i as i32;
        let period_end = if i == period_count - 1 {
            last_ts
        } else {
            first_ts + period_duration * (i + 1) as i32
        };

        // Find dominant category in this period
        let mut cat_counts: HashMap<String, usize> = HashMap::new();
        for e in &watch_events {
            if e.timestamp >= period_start
                && e.timestamp < period_end
                && let Some(m) = metadata.get(&e.video_id)
                && !m.unavailable
                && !m.category_id.is_empty()
            {
                let name = categories
                    .get(&m.category_id)
                    .cloned()
                    .unwrap_or_else(|| format!("Category {}", m.category_id));
                *cat_counts.entry(name).or_default() += 1;
            }
        }

        if let Some((cat, _)) = cat_counts.into_iter().max_by_key(|(_, c)| *c) {
            phases.push(CategoryPhase {
                period_label: period_label.to_string(),
                category: cat,
            });
        }
    }

    // Only return if there's actually variation (at least 2 phases with different categories)
    if phases.len() < 2 {
        return vec![];
    }
    let all_same = phases.windows(2).all(|w| w[0].category == w[1].category);
    if all_same {
        return vec![];
    }

    phases
}

/// Finds the most re-watched video (video_id appearing in multiple Watched events).
fn compute_comfort_video(
    events: &[&Event],
    metadata: &HashMap<String, VideoMeta>,
) -> Option<(String, String, usize)> {
    let mut watch_counts: HashMap<&str, usize> = HashMap::new();
    for e in events {
        if matches!(e.action, Action::Watched) {
            *watch_counts.entry(&e.video_id).or_default() += 1;
        }
    }

    let (id, count) = watch_counts
        .into_iter()
        .filter(|(_, c)| *c >= 2) // Must be watched at least twice
        .max_by_key(|(_, c)| *c)?;

    let title = metadata
        .get(id)
        .filter(|m| !m.unavailable)
        .map(|m| m.title.clone())
        .unwrap_or_default();

    Some((id.to_string(), title, count))
}

/// Computes the queue patience label based on median time-in-queue.
fn compute_queue_patience(events: &[&Event]) -> Option<(&'static str, i64)> {
    let mut queue_times: Vec<i64> = events
        .iter()
        .filter(|e| matches!(e.action, Action::Watched))
        .filter_map(|e| e.time_in_queue_sec)
        .collect();

    if queue_times.is_empty() {
        return None;
    }

    queue_times.sort();
    let median = queue_times[queue_times.len() / 2];

    let label = if median < 3600 {
        "Impulsive"
    } else if median < 86400 {
        "Thoughtful"
    } else if median < 604800 {
        "Fermenter"
    } else {
        "Aged Like Fine Wine"
    };

    Some((label, median))
}

/// Computes total queue throughput (unique video IDs involved in any action).
fn compute_total_throughput(events: &[&Event]) -> usize {
    let mut seen = std::collections::HashSet::new();
    for e in events {
        seen.insert(&e.video_id);
    }
    seen.len()
}

/// Finds the oldest video watched by published_at date.
fn compute_oldest_video(
    watched_ids: &[&str],
    metadata: &HashMap<String, VideoMeta>,
) -> Option<(String, String, DateTime<Utc>)> {
    watched_ids
        .iter()
        .filter_map(|id| metadata.get(*id))
        .filter(|m| !m.unavailable)
        .min_by_key(|m| m.published_at)
        .map(|m| (m.id.clone(), m.title.clone(), m.published_at))
}

/// Computes weekend vs weekday ratio for watched events.
fn compute_weekend_weekday(events: &[&Event]) -> Option<(&'static str, f64)> {
    let mut weekend = 0usize;
    let mut weekday = 0usize;

    for e in events {
        if matches!(e.action, Action::Watched) {
            match to_local(&e.timestamp).weekday() {
                Weekday::Sat | Weekday::Sun => weekend += 1,
                _ => weekday += 1,
            }
        }
    }

    let total = weekend + weekday;
    if total == 0 {
        return None;
    }

    let ratio = weekend as f64 / total as f64;
    let label = if ratio > 0.6 {
        "Weekend Warrior"
    } else if ratio < 0.3 {
        "Weekday Grinder"
    } else {
        "Balanced Viewer"
    };

    Some((label, ratio))
}

fn compute_watches_per_week(events: &[&Event], range: &DateRange) -> Option<f64> {
    let watch_events: Vec<&&Event> = events
        .iter()
        .filter(|e| matches!(e.action, Action::Watched))
        .collect();

    if watch_events.is_empty() {
        return None;
    }

    // Determine the time span
    let first = range
        .start
        .unwrap_or(watch_events.first().unwrap().timestamp);
    let last = range.end.unwrap_or(Utc::now());
    let span_days = (last - first).num_days().max(1) as f64;
    let weeks = span_days / 7.0;

    Some(watch_events.len() as f64 / weeks)
}

// ---------------------------------------------------------------------------
// Printing — basic stats
// ---------------------------------------------------------------------------

pub fn print_basic(stats: &BasicStats, range: &DateRange, has_metadata_available: bool) {
    println!("{}", "YTQ Stats".bold());
    println!("------------------------------");

    if range.start.is_some() || range.end.is_some() {
        println!("Period: {}", range.label());
        println!();
    }

    println!("Videos Added:    {}", stats.added);
    println!("Videos Watched:  {}", stats.watched);
    println!("Videos Skipped:  {}", stats.skipped);
    println!("Completion Rate: {}", format_percent(stats.completion_rate));
    println!("Queue Depth:     {}", stats.queue_depth);

    println!();

    if let Some(avg) = stats.avg_time_in_queue_secs {
        println!("Avg Time in Queue: {}", format_duration_human(avg as i64));
    }

    if let Some(secs) = stats.total_watch_time_secs {
        println!("Total Watch Time:  {}", format_duration_long(secs));
    }

    if let Some((day, count)) = &stats.most_active_weekday {
        println!("Most Active Day:   {day} ({count} videos added)");
    }

    // Queue profile
    if !stats.top_queue_channels.is_empty() {
        println!();
        println!("{}", "Top Channels (Queue)".bold());
        for (i, (channel, count)) in stats.top_queue_channels.iter().enumerate() {
            let videos_label = if *count == 1 { "video" } else { "videos" };
            println!("  {}. {channel}  ({count} {videos_label})", i + 1);
        }
    }

    if let Some(secs) = stats.queue_total_duration_secs {
        println!("Total Queue Duration: {}", format_duration_long(secs));
    }

    // Watched channels (if any watches)
    if !stats.top_watched_channels.is_empty() {
        println!();
        println!("{}", "Top Channels (Watched)".bold());
        for (i, (channel, count)) in stats.top_watched_channels.iter().enumerate() {
            let videos_label = if *count == 1 { "video" } else { "videos" };
            println!("  {}. {channel}  ({count} {videos_label})", i + 1);
        }
    }

    if !has_metadata_available {
        println!();
        println!(
            "{}",
            "Tip: Run `ytq fetch --history` for richer stats (channels, durations, categories)."
                .dimmed()
        );
    }
}

// ---------------------------------------------------------------------------
// Printing — wrapped stats
// ---------------------------------------------------------------------------

pub fn print_wrapped(stats: &WrappedStats, range: &DateRange, has_metadata_available: bool) {
    println!("{}", "YTQ Wrapped".bold());
    println!("------------------------------");

    if range.start.is_some() || range.end.is_some() {
        println!("Period: {}", range.label());
    }
    println!();

    // --- Core counts ---
    println!("Videos Added:    {}", stats.basic.added);
    println!("Videos Watched:  {}", stats.basic.watched);
    println!("Videos Skipped:  {}", stats.basic.skipped);
    println!(
        "Completion Rate: {}",
        format_percent(stats.basic.completion_rate)
    );
    println!("Skip Rate:       {}", format_percent(stats.skip_rate));
    println!("Queue Depth:     {}", stats.basic.queue_depth);

    println!();

    // --- Queue behavior ---
    if let Some(avg) = stats.basic.avg_time_in_queue_secs {
        println!(
            "Avg Time in Queue:     {}",
            format_duration_human(avg as i64)
        );
    }
    if let Some(secs) = stats.fastest_watch_secs {
        println!("Fastest Time to Watch: {}", format_duration_human(secs));
    }
    if let Some(secs) = stats.slowest_watch_secs {
        println!("Slowest Time to Watch: {}", format_duration_human(secs));
    }
    if let Some(wpw) = stats.watches_per_week {
        println!("Watches per Week:      {:.1}", wpw);
    }

    // --- Watch time ---
    if let Some(secs) = stats.basic.total_watch_time_secs {
        println!("Total Watch Time:      {}", format_duration_long(secs));
    }
    if let Some(avg) = stats.watched_avg_duration_secs {
        println!(
            "Avg Video Duration:    {}",
            youtube_api::format_duration(avg)
        );
    }

    // --- Streaks and busy days ---
    println!();
    if stats.longest_streak > 0 {
        let days_label = if stats.longest_streak == 1 {
            "day"
        } else {
            "days"
        };
        println!(
            "Longest Watch Streak:  {} {days_label}",
            stats.longest_streak
        );
    }
    if let Some((day, count)) = &stats.busiest_day {
        println!(
            "Busiest Day:           {} ({count} videos)",
            day.format("%Y-%m-%d")
        );
    }
    if let Some((day, count)) = &stats.basic.most_active_weekday {
        println!("Most Active Weekday:   {day} ({count} videos added)");
    }

    // --- Fun Wrapped Insights (Your Year in Review) ---
    let has_insights = stats.viewer_personality.is_some()
        || stats.channel_loyalty.is_some()
        || stats.watching_age.is_some()
        || stats.queue_patience.is_some()
        || stats.weekend_vs_weekday.is_some()
        || stats.discovery_day.is_some()
        || stats.comfort_video.is_some()
        || stats.oldest_video.is_some()
        || stats.total_throughput > 0
        || !stats.category_evolution.is_empty();

    if has_insights {
        println!();
        println!("{}", "--- Your Year in Review ---".bold());
        println!();

        if let Some((label, description)) = stats.viewer_personality {
            println!("Viewer Personality:    {}", label.cyan().bold());
            println!(
                "                       {}",
                format!("\"{description}\"").dimmed()
            );
        }

        if let Some((ref channel, ratio)) = stats.channel_loyalty {
            println!(
                "Channel Loyalty:       {:.0}% of your watches were from {}",
                ratio * 100.0,
                channel.bold()
            );
        }

        if let Some(year) = stats.watching_age {
            println!("Watching Age:          You watched like it was {}", year);
        }

        if let Some((label, median)) = stats.queue_patience {
            println!(
                "Queue Patience:        {} (median: {} in queue)",
                label,
                format_duration_human(median)
            );
        }

        if let Some((label, ratio)) = stats.weekend_vs_weekday {
            println!(
                "Watch Style:           {} ({:.0}% on weekends)",
                label,
                ratio * 100.0
            );
        }

        if let Some((day, count)) = &stats.discovery_day {
            let channels_label = if *count == 1 { "channel" } else { "channels" };
            println!(
                "Discovery Day:         {} — you explored {} different {channels_label}",
                day.format("%Y-%m-%d"),
                count
            );
        }

        if let Some((ref _id, ref title, count)) = stats.comfort_video {
            let display = if title.is_empty() { _id } else { title };
            println!(
                "Comfort Video:         {} (watched {} times)",
                truncate(display, 40),
                count
            );
        }

        if let Some((ref _id, ref title, ref published_at)) = stats.oldest_video {
            let display = if title.is_empty() { _id } else { title };
            println!(
                "Oldest Video Watched:  {} (published {})",
                truncate(display, 35),
                published_at.format("%Y-%m-%d")
            );
        }

        if stats.total_throughput > 0 {
            let videos_label = if stats.total_throughput == 1 {
                "video"
            } else {
                "videos"
            };
            println!(
                "Queue Throughput:      {} {videos_label} passed through your queue",
                stats.total_throughput
            );
        }

        if !stats.category_evolution.is_empty() {
            println!();
            println!("{}", "Category Evolution".bold());
            for phase in &stats.category_evolution {
                println!("  {}: {}", phase.period_label, phase.category);
            }
        }
    }

    // --- Monthly trends ---
    if !stats.watched_by_month.is_empty() {
        println!();
        println!("{}", "Watched by Month".bold());
        print_bar_chart_monthly(&stats.watched_by_month);
    }

    if !stats.added_by_month.is_empty() {
        println!();
        println!("{}", "Added by Month".bold());
        print_bar_chart_monthly(&stats.added_by_month);
    }

    // --- Time of day ---
    let total_tod: usize = stats.time_of_day.iter().map(|b| b.count).sum();
    if total_tod > 0 {
        println!();
        println!("{}", "Time of Day (Watched)".bold());
        let max_count = stats.time_of_day.iter().map(|b| b.count).max().unwrap_or(1);
        let label_width = stats
            .time_of_day
            .iter()
            .map(|b| b.label.len())
            .max()
            .unwrap_or(10);
        for bucket in &stats.time_of_day {
            let bar = make_bar(bucket.count, max_count, 20);
            println!(
                "  {:<width$}  {} {}",
                bucket.label,
                bar,
                bucket.count,
                width = label_width
            );
        }
    }

    // --- Queue Profile ---
    let has_queue_profile = !stats.queue_top_channels.is_empty()
        || !stats.queue_categories.is_empty()
        || !stats.queue_top_tags.is_empty();

    if has_queue_profile {
        println!();
        println!("{}", "--- Queue Profile ---".bold());

        if let Some(secs) = stats.basic.queue_total_duration_secs {
            println!("Total Queue Duration:  {}", format_duration_long(secs));
        }
        if let Some(avg) = stats.queue_avg_duration_secs {
            println!(
                "Avg Video Duration:    {}",
                youtube_api::format_duration(avg)
            );
        }

        if !stats.queue_top_channels.is_empty() {
            println!();
            println!("{}", "Top Channels".bold());
            print_leaderboard(&stats.queue_top_channels);
        }

        if !stats.queue_categories.is_empty() {
            println!();
            println!("{}", "Categories".bold());
            print_bar_chart_named(&stats.queue_categories);
        }

        if !stats.queue_top_tags.is_empty() {
            println!();
            println!("{}", "Top Tags".bold());
            for (i, (tag, count)) in stats.queue_top_tags.iter().enumerate() {
                println!("  {:>2}. {tag}  ({count})", i + 1);
            }
        }
    }

    // --- Watch History Profile ---
    let has_watch_profile = !stats.watched_top_channels.is_empty()
        || !stats.watched_categories.is_empty()
        || !stats.watched_top_tags.is_empty();

    if has_watch_profile {
        println!();
        println!("{}", "--- Watch History ---".bold());

        if !stats.watched_top_channels.is_empty() {
            println!();
            println!("{}", "Top Channels".bold());
            print_leaderboard(&stats.watched_top_channels);
        }

        if !stats.watched_categories.is_empty() {
            println!();
            println!("{}", "Categories".bold());
            print_bar_chart_named(&stats.watched_categories);
        }

        if !stats.watched_top_tags.is_empty() {
            println!();
            println!("{}", "Top Tags".bold());
            for (i, (tag, count)) in stats.watched_top_tags.iter().enumerate() {
                println!("  {:>2}. {tag}  ({count})", i + 1);
            }
        }

        // Longest / shortest video
        if stats.longest_video.is_some() || stats.shortest_video.is_some() {
            println!();
            if let Some((id, title, secs)) = &stats.longest_video {
                let display = if title.is_empty() { id } else { title };
                println!(
                    "Longest Video:  {} ({})",
                    truncate(display, 40),
                    youtube_api::format_duration(*secs)
                );
            }
            if let Some((id, title, secs)) = &stats.shortest_video {
                let display = if title.is_empty() { id } else { title };
                println!(
                    "Shortest Video: {} ({})",
                    truncate(display, 40),
                    youtube_api::format_duration(*secs)
                );
            }
        }
    }

    if !has_metadata_available {
        println!();
        println!(
            "{}",
            "Tip: Run `ytq fetch --history` for richer stats (channels, durations, categories)."
                .dimmed()
        );
    }
}

// ---------------------------------------------------------------------------
// Formatting helpers
// ---------------------------------------------------------------------------

fn format_percent(ratio: f64) -> String {
    format!("{:.0}%", ratio * 100.0)
}

/// Formats seconds into a human-readable duration like "2d 4h", "3h 12m", "45m", "30s".
pub fn format_duration_human(total_secs: i64) -> String {
    let abs = total_secs.unsigned_abs();
    let days = abs / 86400;
    let hours = (abs % 86400) / 3600;
    let mins = (abs % 3600) / 60;
    let secs = abs % 60;

    if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {mins}m")
    } else if mins > 0 {
        format!("{mins}m")
    } else {
        format!("{secs}s")
    }
}

/// Formats seconds into a long-form duration like "18h 32m".
fn format_duration_long(total_secs: u64) -> String {
    let days = total_secs / 86400;
    let hours = (total_secs % 86400) / 3600;
    let mins = (total_secs % 3600) / 60;

    if days > 0 {
        format!("{days}d {hours}h {mins}m")
    } else if hours > 0 {
        format!("{hours}h {mins}m")
    } else {
        format!("{mins}m")
    }
}

fn make_bar(value: usize, max: usize, width: usize) -> String {
    if max == 0 {
        return " ".repeat(width);
    }
    let filled = (value as f64 / max as f64 * width as f64).round() as usize;
    let filled = filled.min(width);
    let empty = width - filled;
    format!("{}{}", "\u{2588}".repeat(filled), " ".repeat(empty))
}

fn print_bar_chart_monthly(buckets: &[MonthBucket]) {
    if buckets.is_empty() {
        return;
    }
    let max_count = buckets.iter().map(|b| b.count).max().unwrap_or(1);
    let label_width = buckets.iter().map(|b| b.label.len()).max().unwrap_or(7);
    for bucket in buckets {
        let bar = make_bar(bucket.count, max_count, 20);
        println!(
            "  {:<width$}  {} {}",
            bucket.label,
            bar,
            bucket.count,
            width = label_width
        );
    }
}

fn print_leaderboard(items: &[(String, usize)]) {
    if items.is_empty() {
        return;
    }
    let max_count = items.first().map(|(_, c)| *c).unwrap_or(1);
    let name_width = items
        .iter()
        .map(|(n, _)| n.len())
        .max()
        .unwrap_or(10)
        .min(30);
    for (i, (name, count)) in items.iter().enumerate() {
        let bar = make_bar(*count, max_count, 20);
        let display_name = truncate(name, name_width);
        println!(
            "  {:>2}. {:<width$}  {} {}",
            i + 1,
            display_name,
            bar,
            count,
            width = name_width
        );
    }
}

fn print_bar_chart_named(items: &[(String, usize)]) {
    if items.is_empty() {
        return;
    }
    let max_count = items.first().map(|(_, c)| *c).unwrap_or(1);
    let name_width = items
        .iter()
        .map(|(n, _)| n.len())
        .max()
        .unwrap_or(10)
        .min(25);
    for (name, count) in items {
        let bar = make_bar(*count, max_count, 20);
        let display_name = truncate(name, name_width);
        println!(
            "  {:<width$}  {} {}",
            display_name,
            bar,
            count,
            width = name_width
        );
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::sync::Once;

    use super::*;

    use chrono::TimeZone;

    /// Set TZ=UTC so that `to_local()` produces UTC timestamps in tests,
    /// making assertions deterministic regardless of the host timezone.
    static INIT_TZ: Once = Once::new();
    fn init_test_tz() {
        INIT_TZ.call_once(|| {
            unsafe { std::env::set_var("TZ", "UTC") };
        });
    }

    fn make_event(
        action: Action,
        video_id: &str,
        ts: DateTime<Utc>,
        queue_secs: Option<i64>,
    ) -> Event {
        init_test_tz();
        Event {
            timestamp: ts,
            action,
            video_id: video_id.to_string(),
            time_in_queue_sec: queue_secs,
        }
    }

    fn make_meta(
        id: &str,
        channel: &str,
        category_id: &str,
        duration_secs: u64,
        tags: Vec<&str>,
    ) -> VideoMeta {
        VideoMeta {
            id: id.to_string(),
            title: format!("Title for {id}"),
            channel: channel.to_string(),
            channel_id: format!("UC_{channel}"),
            duration: String::new(),
            duration_seconds: duration_secs,
            published_at: Utc::now(),
            category_id: category_id.to_string(),
            tags: tags.into_iter().map(String::from).collect(),
            fetched_at: Utc::now(),
            unavailable: false,
        }
    }

    fn make_meta_with_date(
        id: &str,
        channel: &str,
        category_id: &str,
        duration_secs: u64,
        tags: Vec<&str>,
        published_at: DateTime<Utc>,
    ) -> VideoMeta {
        VideoMeta {
            id: id.to_string(),
            title: format!("Title for {id}"),
            channel: channel.to_string(),
            channel_id: format!("UC_{channel}"),
            duration: String::new(),
            duration_seconds: duration_secs,
            published_at,
            category_id: category_id.to_string(),
            tags: tags.into_iter().map(String::from).collect(),
            fetched_at: Utc::now(),
            unavailable: false,
        }
    }

    // -- DateRange tests --

    #[test]
    fn date_range_all_time_contains_everything() {
        let range = DateRange::all_time();
        let ts = Utc::now();
        assert!(range.contains(&ts));
    }

    #[test]
    fn date_range_last_days_excludes_old() {
        let range = DateRange::last_days(7);
        let old = Utc::now() - TimeDelta::days(10);
        let recent = Utc::now() - TimeDelta::days(3);
        assert!(!range.contains(&old));
        assert!(range.contains(&recent));
    }

    #[test]
    fn date_range_specific_month() {
        let range = DateRange::specific_month(2025, 6).unwrap();
        let inside = Utc.with_ymd_and_hms(2025, 6, 15, 12, 0, 0).unwrap();
        let before = Utc.with_ymd_and_hms(2025, 5, 31, 23, 59, 59).unwrap();
        let after = Utc.with_ymd_and_hms(2025, 7, 1, 0, 0, 0).unwrap();
        assert!(range.contains(&inside));
        assert!(!range.contains(&before));
        assert!(!range.contains(&after));
    }

    #[test]
    fn date_range_specific_month_december() {
        let range = DateRange::specific_month(2025, 12).unwrap();
        let inside = Utc.with_ymd_and_hms(2025, 12, 25, 0, 0, 0).unwrap();
        let after = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        assert!(range.contains(&inside));
        assert!(!range.contains(&after));
    }

    #[test]
    fn date_range_specific_year() {
        let range = DateRange::specific_year(2025).unwrap();
        let inside = Utc.with_ymd_and_hms(2025, 6, 15, 0, 0, 0).unwrap();
        let before = Utc.with_ymd_and_hms(2024, 12, 31, 23, 59, 59).unwrap();
        let after = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        assert!(range.contains(&inside));
        assert!(!range.contains(&before));
        assert!(!range.contains(&after));
    }

    #[test]
    fn date_range_custom_from_only() {
        let from = NaiveDate::from_ymd_opt(2025, 1, 1).unwrap();
        let range = DateRange::custom(Some(from), None);
        let inside = Utc.with_ymd_and_hms(2025, 6, 1, 0, 0, 0).unwrap();
        let before = Utc.with_ymd_and_hms(2024, 12, 31, 0, 0, 0).unwrap();
        assert!(range.contains(&inside));
        assert!(!range.contains(&before));
    }

    #[test]
    fn date_range_label_all_time() {
        assert_eq!(DateRange::all_time().label(), "All Time");
    }

    #[test]
    fn date_range_label_specific_year() {
        let range = DateRange::specific_year(2025).unwrap();
        assert_eq!(range.label(), "2025-01-01 to 2026-01-01");
    }

    // -- filter_events tests --

    #[test]
    fn filter_events_all_time_returns_all() {
        let events = vec![
            make_event(Action::Queued, "a", Utc::now(), None),
            make_event(Action::Watched, "a", Utc::now(), Some(100)),
        ];
        let range = DateRange::all_time();
        let filtered = filter_events(&events, &range);
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn filter_events_by_range() {
        let old = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let recent = Utc.with_ymd_and_hms(2025, 6, 15, 0, 0, 0).unwrap();
        let events = vec![
            make_event(Action::Queued, "a", old, None),
            make_event(Action::Watched, "b", recent, Some(100)),
        ];
        let range = DateRange::specific_year(2025).unwrap();
        let filtered = filter_events(&events, &range);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].video_id, "b");
    }

    // -- compute_basic tests --

    #[test]
    fn basic_stats_counts() {
        let events = vec![
            make_event(Action::Queued, "a", Utc::now(), None),
            make_event(Action::Queued, "b", Utc::now(), None),
            make_event(Action::Watched, "a", Utc::now(), Some(3600)),
            make_event(Action::Skipped, "b", Utc::now(), None),
        ];
        let refs: Vec<&Event> = events.iter().collect();
        let queue_ids: Vec<String> = vec!["x".to_string(), "y".to_string()];
        let stats = compute_basic(&refs, &queue_ids, &HashMap::new());

        assert_eq!(stats.added, 2);
        assert_eq!(stats.watched, 1);
        assert_eq!(stats.skipped, 1);
        assert_eq!(stats.queue_depth, 2);
        assert!((stats.completion_rate - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn basic_stats_avg_queue_time() {
        let events = vec![
            make_event(Action::Watched, "a", Utc::now(), Some(100)),
            make_event(Action::Watched, "b", Utc::now(), Some(200)),
        ];
        let refs: Vec<&Event> = events.iter().collect();
        let stats = compute_basic(&refs, &[], &HashMap::new());

        assert!((stats.avg_time_in_queue_secs.unwrap() - 150.0).abs() < f64::EPSILON);
    }

    #[test]
    fn basic_stats_no_events() {
        let refs: Vec<&Event> = vec![];
        let stats = compute_basic(&refs, &[], &HashMap::new());

        assert_eq!(stats.added, 0);
        assert_eq!(stats.watched, 0);
        assert_eq!(stats.skipped, 0);
        assert!(stats.avg_time_in_queue_secs.is_none());
        assert!(stats.total_watch_time_secs.is_none());
        assert!(stats.top_watched_channels.is_empty());
    }

    #[test]
    fn basic_stats_with_metadata() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "a".to_string(),
            make_meta("a", "Channel A", "10", 300, vec![]),
        );
        metadata.insert(
            "b".to_string(),
            make_meta("b", "Channel A", "10", 200, vec![]),
        );
        metadata.insert(
            "c".to_string(),
            make_meta("c", "Channel B", "28", 100, vec![]),
        );

        let events = vec![
            make_event(Action::Watched, "a", Utc::now(), Some(100)),
            make_event(Action::Watched, "b", Utc::now(), Some(200)),
            make_event(Action::Watched, "c", Utc::now(), Some(50)),
        ];
        let refs: Vec<&Event> = events.iter().collect();
        let stats = compute_basic(&refs, &[], &metadata);

        assert_eq!(stats.total_watch_time_secs, Some(600));
        assert_eq!(stats.top_watched_channels.len(), 2);
        assert_eq!(stats.top_watched_channels[0].0, "Channel A");
        assert_eq!(stats.top_watched_channels[0].1, 2);
    }

    #[test]
    fn basic_stats_queue_profile() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "q1".to_string(),
            make_meta("q1", "Chan X", "10", 600, vec![]),
        );
        metadata.insert(
            "q2".to_string(),
            make_meta("q2", "Chan X", "10", 400, vec![]),
        );
        metadata.insert(
            "q3".to_string(),
            make_meta("q3", "Chan Y", "28", 200, vec![]),
        );

        let queue_ids = vec!["q1".to_string(), "q2".to_string(), "q3".to_string()];
        let refs: Vec<&Event> = vec![];
        let stats = compute_basic(&refs, &queue_ids, &metadata);

        assert_eq!(stats.queue_depth, 3);
        assert_eq!(stats.queue_total_duration_secs, Some(1200));
        assert_eq!(stats.top_queue_channels.len(), 2);
        assert_eq!(stats.top_queue_channels[0].0, "Chan X");
        assert_eq!(stats.top_queue_channels[0].1, 2);
    }

    #[test]
    fn watched_deduplicates_for_metadata() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "a".to_string(),
            make_meta("a", "Channel A", "10", 300, vec![]),
        );

        // Same video watched twice — should only count once for metadata stats
        let events = vec![
            make_event(Action::Watched, "a", Utc::now(), Some(100)),
            make_event(Action::Watched, "a", Utc::now(), Some(200)),
        ];
        let refs: Vec<&Event> = events.iter().collect();
        let stats = compute_basic(&refs, &[], &metadata);

        // Event count is 2, but watch time is deduped (300, not 600)
        assert_eq!(stats.watched, 2);
        assert_eq!(stats.total_watch_time_secs, Some(300));
        assert_eq!(stats.top_watched_channels.len(), 1);
        assert_eq!(stats.top_watched_channels[0].1, 1); // 1 unique video, not 2 events
    }

    // -- unique_ids_for_action tests --

    #[test]
    fn unique_ids_deduplicates() {
        let events = vec![
            make_event(Action::Watched, "a", Utc::now(), Some(0)),
            make_event(Action::Watched, "b", Utc::now(), Some(0)),
            make_event(Action::Watched, "a", Utc::now(), Some(0)),
        ];
        let refs: Vec<&Event> = events.iter().collect();
        let ids = unique_ids_for_action(&refs, &Action::Watched);
        assert_eq!(ids, vec!["a".to_string(), "b".to_string()]);
    }

    // -- longest_streak tests --

    #[test]
    fn streak_consecutive_days() {
        let events = vec![
            make_event(
                Action::Watched,
                "a",
                Utc.with_ymd_and_hms(2025, 1, 1, 10, 0, 0).unwrap(),
                Some(0),
            ),
            make_event(
                Action::Watched,
                "b",
                Utc.with_ymd_and_hms(2025, 1, 2, 10, 0, 0).unwrap(),
                Some(0),
            ),
            make_event(
                Action::Watched,
                "c",
                Utc.with_ymd_and_hms(2025, 1, 3, 10, 0, 0).unwrap(),
                Some(0),
            ),
            // gap
            make_event(
                Action::Watched,
                "d",
                Utc.with_ymd_and_hms(2025, 1, 5, 10, 0, 0).unwrap(),
                Some(0),
            ),
        ];
        let refs: Vec<&Event> = events.iter().collect();
        assert_eq!(longest_streak(&refs), 3);
    }

    #[test]
    fn streak_no_watches() {
        let events = vec![make_event(Action::Queued, "a", Utc::now(), None)];
        let refs: Vec<&Event> = events.iter().collect();
        assert_eq!(longest_streak(&refs), 0);
    }

    #[test]
    fn streak_single_day() {
        let events = vec![make_event(Action::Watched, "a", Utc::now(), Some(0))];
        let refs: Vec<&Event> = events.iter().collect();
        assert_eq!(longest_streak(&refs), 1);
    }

    #[test]
    fn streak_multiple_watches_same_day() {
        let day = Utc.with_ymd_and_hms(2025, 1, 1, 10, 0, 0).unwrap();
        let events = vec![
            make_event(Action::Watched, "a", day, Some(0)),
            make_event(Action::Watched, "b", day, Some(0)),
            make_event(Action::Watched, "c", day, Some(0)),
        ];
        let refs: Vec<&Event> = events.iter().collect();
        assert_eq!(longest_streak(&refs), 1);
    }

    // -- time_of_day tests --

    #[test]
    fn time_of_day_buckets() {
        let events = vec![
            make_event(
                Action::Watched,
                "a",
                Utc.with_ymd_and_hms(2025, 1, 1, 8, 0, 0).unwrap(),
                Some(0),
            ), // morning
            make_event(
                Action::Watched,
                "b",
                Utc.with_ymd_and_hms(2025, 1, 1, 14, 0, 0).unwrap(),
                Some(0),
            ), // afternoon
            make_event(
                Action::Watched,
                "c",
                Utc.with_ymd_and_hms(2025, 1, 1, 19, 0, 0).unwrap(),
                Some(0),
            ), // evening
            make_event(
                Action::Watched,
                "d",
                Utc.with_ymd_and_hms(2025, 1, 1, 23, 0, 0).unwrap(),
                Some(0),
            ), // night
            make_event(
                Action::Watched,
                "e",
                Utc.with_ymd_and_hms(2025, 1, 1, 3, 0, 0).unwrap(),
                Some(0),
            ), // night
        ];
        let refs: Vec<&Event> = events.iter().collect();
        let tod = time_of_day_distribution(&refs);

        assert_eq!(tod[0].count, 1); // morning
        assert_eq!(tod[1].count, 1); // afternoon
        assert_eq!(tod[2].count, 1); // evening
        assert_eq!(tod[3].count, 2); // night
    }

    // -- category_breakdown tests --

    #[test]
    fn category_breakdown() {
        let mut metadata = HashMap::new();
        metadata.insert("a".to_string(), make_meta("a", "Ch", "10", 100, vec![]));
        metadata.insert("b".to_string(), make_meta("b", "Ch", "10", 100, vec![]));
        metadata.insert("c".to_string(), make_meta("c", "Ch", "28", 100, vec![]));

        let mut categories = HashMap::new();
        categories.insert("10".to_string(), "Music".to_string());
        categories.insert("28".to_string(), "Science & Technology".to_string());

        let ids = vec!["a", "b", "c"];
        let result = category_breakdown_from(&ids, &metadata, &categories);

        assert_eq!(result[0], ("Music".to_string(), 2));
        assert_eq!(result[1], ("Science & Technology".to_string(), 1));
    }

    // -- top_tags tests --

    #[test]
    fn top_tags_aggregation() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "a".to_string(),
            make_meta("a", "Ch", "10", 100, vec!["rust", "programming"]),
        );
        metadata.insert(
            "b".to_string(),
            make_meta("b", "Ch", "10", 100, vec!["Rust", "tutorial"]),
        );
        metadata.insert(
            "c".to_string(),
            make_meta("c", "Ch", "28", 100, vec!["python"]),
        );

        let ids = vec!["a", "b", "c"];
        let result = top_tags_from(&ids, &metadata, 5);

        // "rust" and "Rust" should be normalized to "rust" with count 2
        assert_eq!(result[0].0, "rust");
        assert_eq!(result[0].1, 2);
    }

    // -- duration_stats tests --

    #[test]
    fn duration_stats_computes_correctly() {
        let mut metadata = HashMap::new();
        metadata.insert("a".to_string(), make_meta("a", "Ch", "10", 600, vec![]));
        metadata.insert("b".to_string(), make_meta("b", "Ch", "10", 300, vec![]));
        metadata.insert("c".to_string(), make_meta("c", "Ch", "10", 120, vec![]));

        let ids = vec!["a", "b", "c"];
        let (avg, longest, shortest) = duration_stats(&ids, &metadata);

        assert_eq!(avg, Some(340)); // (600+300+120)/3
        assert_eq!(longest.as_ref().unwrap().0, "a");
        assert_eq!(longest.as_ref().unwrap().2, 600);
        assert_eq!(shortest.as_ref().unwrap().0, "c");
        assert_eq!(shortest.as_ref().unwrap().2, 120);
    }

    // -- format_duration_human tests --

    #[test]
    fn format_duration_human_days() {
        assert_eq!(format_duration_human(90000), "1d 1h");
    }

    #[test]
    fn format_duration_human_hours() {
        assert_eq!(format_duration_human(3720), "1h 2m");
    }

    #[test]
    fn format_duration_human_minutes() {
        assert_eq!(format_duration_human(300), "5m");
    }

    #[test]
    fn format_duration_human_seconds() {
        assert_eq!(format_duration_human(45), "45s");
    }

    // -- make_bar tests --

    #[test]
    fn bar_full_width() {
        let bar = make_bar(10, 10, 10);
        assert_eq!(
            bar,
            "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}"
        );
    }

    #[test]
    fn bar_half_width() {
        let bar = make_bar(5, 10, 10);
        assert_eq!(bar, "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}     ");
    }

    #[test]
    fn bar_zero() {
        let bar = make_bar(0, 10, 10);
        assert_eq!(bar, "          ");
    }

    #[test]
    fn bar_zero_max() {
        let bar = make_bar(0, 0, 10);
        assert_eq!(bar, "          ");
    }

    // -- monthly_buckets tests --

    #[test]
    fn monthly_buckets_groups_correctly() {
        let events = vec![
            make_event(
                Action::Queued,
                "a",
                Utc.with_ymd_and_hms(2025, 1, 5, 0, 0, 0).unwrap(),
                None,
            ),
            make_event(
                Action::Queued,
                "b",
                Utc.with_ymd_and_hms(2025, 1, 15, 0, 0, 0).unwrap(),
                None,
            ),
            make_event(
                Action::Queued,
                "c",
                Utc.with_ymd_and_hms(2025, 2, 1, 0, 0, 0).unwrap(),
                None,
            ),
        ];
        let refs: Vec<&Event> = events.iter().collect();
        let buckets = monthly_buckets(&refs, &Action::Queued);

        assert_eq!(buckets.len(), 2);
        assert_eq!(buckets[0].label, "2025-01");
        assert_eq!(buckets[0].count, 2);
        assert_eq!(buckets[1].label, "2025-02");
        assert_eq!(buckets[1].count, 1);
    }

    // -- channel_loyalty tests --

    #[test]
    fn channel_loyalty_returns_top_channel() {
        let channels = vec![("Fireship".to_string(), 5), ("Other".to_string(), 3)];
        let result = compute_channel_loyalty(&channels, 10);
        assert!(result.is_some());
        let (name, ratio) = result.unwrap();
        assert_eq!(name, "Fireship");
        assert!((ratio - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn channel_loyalty_none_when_empty() {
        let result = compute_channel_loyalty(&[], 0);
        assert!(result.is_none());
    }

    #[test]
    fn channel_loyalty_none_when_single_watch() {
        let channels = vec![("Fireship".to_string(), 1)];
        let result = compute_channel_loyalty(&channels, 1);
        assert!(result.is_none()); // Need at least 2
    }

    // -- watching_age tests --

    #[test]
    fn watching_age_computes_average_year() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "a".to_string(),
            make_meta_with_date(
                "a",
                "Ch",
                "10",
                100,
                vec![],
                Utc.with_ymd_and_hms(2020, 6, 1, 0, 0, 0).unwrap(),
            ),
        );
        metadata.insert(
            "b".to_string(),
            make_meta_with_date(
                "b",
                "Ch",
                "10",
                100,
                vec![],
                Utc.with_ymd_and_hms(2022, 6, 1, 0, 0, 0).unwrap(),
            ),
        );

        let ids = vec!["a", "b"];
        let result = compute_watching_age(&ids, &metadata);
        assert_eq!(result, Some(2021));
    }

    #[test]
    fn watching_age_none_when_no_metadata() {
        let result = compute_watching_age(&[], &HashMap::new());
        assert!(result.is_none());
    }

    // -- discovery_day tests --

    #[test]
    fn discovery_day_finds_most_diverse_day() {
        let day1 = Utc.with_ymd_and_hms(2025, 3, 14, 10, 0, 0).unwrap();
        let day1b = Utc.with_ymd_and_hms(2025, 3, 14, 14, 0, 0).unwrap();
        let day1c = Utc.with_ymd_and_hms(2025, 3, 14, 18, 0, 0).unwrap();
        let day2 = Utc.with_ymd_and_hms(2025, 3, 15, 10, 0, 0).unwrap();

        let events = vec![
            make_event(Action::Watched, "a", day1, Some(0)),
            make_event(Action::Watched, "b", day1b, Some(0)),
            make_event(Action::Watched, "c", day1c, Some(0)),
            make_event(Action::Watched, "d", day2, Some(0)),
        ];

        let mut metadata = HashMap::new();
        metadata.insert("a".to_string(), make_meta("a", "Chan A", "10", 100, vec![]));
        metadata.insert("b".to_string(), make_meta("b", "Chan B", "10", 100, vec![]));
        metadata.insert("c".to_string(), make_meta("c", "Chan C", "10", 100, vec![]));
        metadata.insert("d".to_string(), make_meta("d", "Chan A", "10", 100, vec![]));

        let refs: Vec<&Event> = events.iter().collect();
        let result = compute_discovery_day(&refs, &metadata);
        assert!(result.is_some());
        let (date, count) = result.unwrap();
        assert_eq!(date, NaiveDate::from_ymd_opt(2025, 3, 14).unwrap());
        assert_eq!(count, 3);
    }

    #[test]
    fn discovery_day_none_when_single_channel_per_day() {
        let day1 = Utc.with_ymd_and_hms(2025, 3, 14, 10, 0, 0).unwrap();

        let events = vec![make_event(Action::Watched, "a", day1, Some(0))];

        let mut metadata = HashMap::new();
        metadata.insert("a".to_string(), make_meta("a", "Chan A", "10", 100, vec![]));

        let refs: Vec<&Event> = events.iter().collect();
        let result = compute_discovery_day(&refs, &metadata);
        assert!(result.is_none()); // Only 1 channel, not interesting
    }

    // -- comfort_video tests --

    #[test]
    fn comfort_video_finds_most_rewatched() {
        let events = vec![
            make_event(Action::Watched, "a", Utc::now(), Some(0)),
            make_event(Action::Watched, "b", Utc::now(), Some(0)),
            make_event(Action::Watched, "a", Utc::now(), Some(0)),
            make_event(Action::Watched, "a", Utc::now(), Some(0)),
        ];

        let mut metadata = HashMap::new();
        metadata.insert("a".to_string(), make_meta("a", "Ch", "10", 100, vec![]));

        let refs: Vec<&Event> = events.iter().collect();
        let result = compute_comfort_video(&refs, &metadata);
        assert!(result.is_some());
        let (id, _title, count) = result.unwrap();
        assert_eq!(id, "a");
        assert_eq!(count, 3);
    }

    #[test]
    fn comfort_video_none_when_no_rewatches() {
        let events = vec![
            make_event(Action::Watched, "a", Utc::now(), Some(0)),
            make_event(Action::Watched, "b", Utc::now(), Some(0)),
        ];

        let refs: Vec<&Event> = events.iter().collect();
        let result = compute_comfort_video(&refs, &HashMap::new());
        assert!(result.is_none());
    }

    // -- queue_patience tests --

    #[test]
    fn queue_patience_impulsive() {
        let events = vec![
            make_event(Action::Watched, "a", Utc::now(), Some(300)), // 5 min
            make_event(Action::Watched, "b", Utc::now(), Some(600)), // 10 min
        ];
        let refs: Vec<&Event> = events.iter().collect();
        let result = compute_queue_patience(&refs);
        assert!(result.is_some());
        let (label, _) = result.unwrap();
        assert_eq!(label, "Impulsive");
    }

    #[test]
    fn queue_patience_thoughtful() {
        let events = vec![
            make_event(Action::Watched, "a", Utc::now(), Some(7200)), // 2h
            make_event(Action::Watched, "b", Utc::now(), Some(14400)), // 4h
        ];
        let refs: Vec<&Event> = events.iter().collect();
        let result = compute_queue_patience(&refs);
        let (label, _) = result.unwrap();
        assert_eq!(label, "Thoughtful");
    }

    #[test]
    fn queue_patience_fermenter() {
        let events = vec![
            make_event(Action::Watched, "a", Utc::now(), Some(172800)), // 2 days
            make_event(Action::Watched, "b", Utc::now(), Some(259200)), // 3 days
        ];
        let refs: Vec<&Event> = events.iter().collect();
        let result = compute_queue_patience(&refs);
        let (label, _) = result.unwrap();
        assert_eq!(label, "Fermenter");
    }

    #[test]
    fn queue_patience_aged() {
        let events = vec![
            make_event(Action::Watched, "a", Utc::now(), Some(700000)), // > 1 week
            make_event(Action::Watched, "b", Utc::now(), Some(800000)),
        ];
        let refs: Vec<&Event> = events.iter().collect();
        let result = compute_queue_patience(&refs);
        let (label, _) = result.unwrap();
        assert_eq!(label, "Aged Like Fine Wine");
    }

    #[test]
    fn queue_patience_none_when_no_watches() {
        let events = vec![make_event(Action::Queued, "a", Utc::now(), None)];
        let refs: Vec<&Event> = events.iter().collect();
        let result = compute_queue_patience(&refs);
        assert!(result.is_none());
    }

    // -- total_throughput tests --

    #[test]
    fn total_throughput_counts_unique_ids() {
        let events = vec![
            make_event(Action::Queued, "a", Utc::now(), None),
            make_event(Action::Queued, "b", Utc::now(), None),
            make_event(Action::Watched, "a", Utc::now(), Some(0)),
            make_event(Action::Queued, "c", Utc::now(), None),
            make_event(Action::Skipped, "c", Utc::now(), None),
        ];
        let refs: Vec<&Event> = events.iter().collect();
        assert_eq!(compute_total_throughput(&refs), 3); // a, b, c
    }

    #[test]
    fn total_throughput_zero_when_empty() {
        let refs: Vec<&Event> = vec![];
        assert_eq!(compute_total_throughput(&refs), 0);
    }

    // -- oldest_video tests --

    #[test]
    fn oldest_video_finds_earliest_published() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "a".to_string(),
            make_meta_with_date(
                "a",
                "Ch",
                "10",
                100,
                vec![],
                Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap(),
            ),
        );
        metadata.insert(
            "b".to_string(),
            make_meta_with_date(
                "b",
                "Ch",
                "10",
                100,
                vec![],
                Utc.with_ymd_and_hms(2015, 6, 15, 0, 0, 0).unwrap(),
            ),
        );
        metadata.insert(
            "c".to_string(),
            make_meta_with_date(
                "c",
                "Ch",
                "10",
                100,
                vec![],
                Utc.with_ymd_and_hms(2023, 3, 1, 0, 0, 0).unwrap(),
            ),
        );

        let ids = vec!["a", "b", "c"];
        let result = compute_oldest_video(&ids, &metadata);
        assert!(result.is_some());
        let (id, _, published_at) = result.unwrap();
        assert_eq!(id, "b");
        assert_eq!(published_at.year(), 2015);
    }

    #[test]
    fn oldest_video_none_when_empty() {
        let result = compute_oldest_video(&[], &HashMap::new());
        assert!(result.is_none());
    }

    // -- weekend_weekday tests --

    #[test]
    fn weekend_warrior() {
        // All watches on weekends (Sat = 2025-01-04, Sun = 2025-01-05)
        let sat = Utc.with_ymd_and_hms(2025, 1, 4, 10, 0, 0).unwrap();
        let sun = Utc.with_ymd_and_hms(2025, 1, 5, 10, 0, 0).unwrap();
        let mon = Utc.with_ymd_and_hms(2025, 1, 6, 10, 0, 0).unwrap();

        let events = vec![
            make_event(Action::Watched, "a", sat, Some(0)),
            make_event(Action::Watched, "b", sun, Some(0)),
            make_event(Action::Watched, "c", sat, Some(0)),
            make_event(Action::Watched, "d", mon, Some(0)),
        ];
        let refs: Vec<&Event> = events.iter().collect();
        let result = compute_weekend_weekday(&refs);
        assert!(result.is_some());
        let (label, ratio) = result.unwrap();
        assert_eq!(label, "Weekend Warrior");
        assert!((ratio - 0.75).abs() < f64::EPSILON);
    }

    #[test]
    fn weekday_grinder() {
        // All watches on weekdays
        let mon = Utc.with_ymd_and_hms(2025, 1, 6, 10, 0, 0).unwrap();
        let tue = Utc.with_ymd_and_hms(2025, 1, 7, 10, 0, 0).unwrap();
        let wed = Utc.with_ymd_and_hms(2025, 1, 8, 10, 0, 0).unwrap();

        let events = vec![
            make_event(Action::Watched, "a", mon, Some(0)),
            make_event(Action::Watched, "b", tue, Some(0)),
            make_event(Action::Watched, "c", wed, Some(0)),
        ];
        let refs: Vec<&Event> = events.iter().collect();
        let result = compute_weekend_weekday(&refs);
        let (label, ratio) = result.unwrap();
        assert_eq!(label, "Weekday Grinder");
        assert!((ratio - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn weekend_weekday_none_when_no_watches() {
        let events = vec![make_event(Action::Queued, "a", Utc::now(), None)];
        let refs: Vec<&Event> = events.iter().collect();
        let result = compute_weekend_weekday(&refs);
        assert!(result.is_none());
    }

    // -- category_evolution tests --

    #[test]
    fn category_evolution_shows_shift() {
        let mut metadata = HashMap::new();
        metadata.insert("a".to_string(), make_meta("a", "Ch", "10", 100, vec![]));
        metadata.insert("b".to_string(), make_meta("b", "Ch", "10", 100, vec![]));
        metadata.insert("c".to_string(), make_meta("c", "Ch", "20", 100, vec![]));
        metadata.insert("d".to_string(), make_meta("d", "Ch", "20", 100, vec![]));

        let mut categories = HashMap::new();
        categories.insert("10".to_string(), "Music".to_string());
        categories.insert("20".to_string(), "Gaming".to_string());

        // Watches span a full year, first half Music, second half Gaming
        let events = vec![
            make_event(
                Action::Watched,
                "a",
                Utc.with_ymd_and_hms(2025, 2, 1, 10, 0, 0).unwrap(),
                Some(0),
            ),
            make_event(
                Action::Watched,
                "b",
                Utc.with_ymd_and_hms(2025, 3, 1, 10, 0, 0).unwrap(),
                Some(0),
            ),
            make_event(
                Action::Watched,
                "c",
                Utc.with_ymd_and_hms(2025, 9, 1, 10, 0, 0).unwrap(),
                Some(0),
            ),
            make_event(
                Action::Watched,
                "d",
                Utc.with_ymd_and_hms(2025, 10, 1, 10, 0, 0).unwrap(),
                Some(0),
            ),
        ];

        let refs: Vec<&Event> = events.iter().collect();
        let range = DateRange::specific_year(2025).unwrap();
        let result = compute_category_evolution(&refs, &metadata, &categories, &range);

        assert!(!result.is_empty());
        // Should have categories split across periods
        assert!(result.len() >= 2);
        // First period should be Music, later period should be Gaming
        assert_eq!(result.first().unwrap().category, "Music");
        assert_eq!(result.last().unwrap().category, "Gaming");
    }

    #[test]
    fn category_evolution_empty_when_span_too_short() {
        let events = vec![
            make_event(
                Action::Watched,
                "a",
                Utc.with_ymd_and_hms(2025, 1, 1, 10, 0, 0).unwrap(),
                Some(0),
            ),
            make_event(
                Action::Watched,
                "b",
                Utc.with_ymd_and_hms(2025, 1, 5, 10, 0, 0).unwrap(),
                Some(0),
            ),
        ];

        let refs: Vec<&Event> = events.iter().collect();
        // Range of only 30 days — too short for evolution
        let range = DateRange::specific_month(2025, 1).unwrap();
        let result = compute_category_evolution(&refs, &HashMap::new(), &HashMap::new(), &range);
        assert!(result.is_empty());
    }

    #[test]
    fn category_evolution_empty_when_all_same_category() {
        let mut metadata = HashMap::new();
        metadata.insert("a".to_string(), make_meta("a", "Ch", "10", 100, vec![]));
        metadata.insert("b".to_string(), make_meta("b", "Ch", "10", 100, vec![]));
        metadata.insert("c".to_string(), make_meta("c", "Ch", "10", 100, vec![]));
        metadata.insert("d".to_string(), make_meta("d", "Ch", "10", 100, vec![]));

        let mut categories = HashMap::new();
        categories.insert("10".to_string(), "Music".to_string());

        let events = vec![
            make_event(
                Action::Watched,
                "a",
                Utc.with_ymd_and_hms(2025, 2, 1, 10, 0, 0).unwrap(),
                Some(0),
            ),
            make_event(
                Action::Watched,
                "b",
                Utc.with_ymd_and_hms(2025, 5, 1, 10, 0, 0).unwrap(),
                Some(0),
            ),
            make_event(
                Action::Watched,
                "c",
                Utc.with_ymd_and_hms(2025, 8, 1, 10, 0, 0).unwrap(),
                Some(0),
            ),
            make_event(
                Action::Watched,
                "d",
                Utc.with_ymd_and_hms(2025, 11, 1, 10, 0, 0).unwrap(),
                Some(0),
            ),
        ];

        let refs: Vec<&Event> = events.iter().collect();
        let range = DateRange::specific_year(2025).unwrap();
        let result = compute_category_evolution(&refs, &metadata, &categories, &range);
        assert!(result.is_empty()); // All same category, no evolution to show
    }

    // -- viewer_personality tests --

    #[test]
    fn personality_night_owl() {
        let time_of_day = vec![
            TimeOfDayBucket {
                label: "Morning (6am-12pm)",
                count: 1,
            },
            TimeOfDayBucket {
                label: "Afternoon (12-5pm)",
                count: 1,
            },
            TimeOfDayBucket {
                label: "Evening (5-10pm)",
                count: 1,
            },
            TimeOfDayBucket {
                label: "Night (10pm-6am)",
                count: 7,
            },
        ];

        let result =
            compute_viewer_personality(&[], &time_of_day, 1, Some(2.0), 0.1, 5, 10, &[], &[]);
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, "The Night Owl");
    }

    #[test]
    fn personality_binger() {
        let time_of_day = vec![
            TimeOfDayBucket {
                label: "Morning (6am-12pm)",
                count: 3,
            },
            TimeOfDayBucket {
                label: "Afternoon (12-5pm)",
                count: 3,
            },
            TimeOfDayBucket {
                label: "Evening (5-10pm)",
                count: 3,
            },
            TimeOfDayBucket {
                label: "Night (10pm-6am)",
                count: 1,
            },
        ];

        let result =
            compute_viewer_personality(&[], &time_of_day, 7, Some(8.0), 0.1, 5, 10, &[], &[]);
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, "The Binger");
    }

    #[test]
    fn personality_stockpiler() {
        let time_of_day = vec![
            TimeOfDayBucket {
                label: "Morning (6am-12pm)",
                count: 2,
            },
            TimeOfDayBucket {
                label: "Afternoon (12-5pm)",
                count: 2,
            },
            TimeOfDayBucket {
                label: "Evening (5-10pm)",
                count: 2,
            },
            TimeOfDayBucket {
                label: "Night (10pm-6am)",
                count: 2,
            },
        ];

        let result =
            compute_viewer_personality(&[], &time_of_day, 1, Some(1.0), 0.2, 50, 5, &[], &[]);
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, "The Stockpiler");
    }

    #[test]
    fn personality_collector_no_watches() {
        let time_of_day = vec![
            TimeOfDayBucket {
                label: "Morning (6am-12pm)",
                count: 0,
            },
            TimeOfDayBucket {
                label: "Afternoon (12-5pm)",
                count: 0,
            },
            TimeOfDayBucket {
                label: "Evening (5-10pm)",
                count: 0,
            },
            TimeOfDayBucket {
                label: "Night (10pm-6am)",
                count: 0,
            },
        ];

        let result = compute_viewer_personality(&[], &time_of_day, 0, None, 0.0, 10, 0, &[], &[]);
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, "The Collector");
    }

    #[test]
    fn personality_none_when_empty() {
        let time_of_day = vec![
            TimeOfDayBucket {
                label: "Morning (6am-12pm)",
                count: 0,
            },
            TimeOfDayBucket {
                label: "Afternoon (12-5pm)",
                count: 0,
            },
            TimeOfDayBucket {
                label: "Evening (5-10pm)",
                count: 0,
            },
            TimeOfDayBucket {
                label: "Night (10pm-6am)",
                count: 0,
            },
        ];

        let result = compute_viewer_personality(&[], &time_of_day, 0, None, 0.0, 0, 0, &[], &[]);
        assert!(result.is_none());
    }
}

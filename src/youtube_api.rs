use std::collections::HashMap;
use std::sync::LazyLock;

use crate::models::VideoMeta;

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use regex::Regex;
use serde_json::Value;

const YOUTUBE_API_BASE: &str = "https://www.googleapis.com/youtube/v3/videos";
const YOUTUBE_CATEGORIES_API: &str = "https://www.googleapis.com/youtube/v3/videoCategories";

/// Maximum number of video IDs per API request (YouTube API limit).
const BATCH_SIZE: usize = 50;

/// Regex for parsing ISO 8601 durations (e.g., PT1H2M3S, PT3M33S, PT45S).
static DURATION_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^PT(?:(\d+)H)?(?:(\d+)M)?(?:(\d+)S)?$").unwrap());

/// Parses an ISO 8601 duration string (e.g., "PT3M33S") into total seconds.
pub fn parse_iso8601_duration(duration: &str) -> Option<u64> {
    let caps = DURATION_RE.captures(duration)?;

    let hours: u64 = caps.get(1).map_or(0, |m| m.as_str().parse().unwrap_or(0));
    let minutes: u64 = caps.get(2).map_or(0, |m| m.as_str().parse().unwrap_or(0));
    let seconds: u64 = caps.get(3).map_or(0, |m| m.as_str().parse().unwrap_or(0));

    Some(hours * 3600 + minutes * 60 + seconds)
}

/// Formats a duration in seconds as "H:MM:SS" or "M:SS".
pub fn format_duration(seconds: u64) -> String {
    let h = seconds / 3600;
    let m = (seconds % 3600) / 60;
    let s = seconds % 60;

    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// Fetches metadata for a batch of video IDs from the YouTube Data API v3.
/// IDs are automatically chunked into batches of 50 (API limit).
/// Returns metadata for all videos that were successfully resolved.
/// Videos that are deleted/private/unavailable are silently skipped.
pub fn fetch_video_metadata(ids: &[String], api_key: &str) -> Result<Vec<VideoMeta>> {
    let mut all_metadata = Vec::new();
    let total = ids.len();

    for (chunk_idx, chunk) in ids.chunks(BATCH_SIZE).enumerate() {
        let start = chunk_idx * BATCH_SIZE + 1;
        let end = (start + chunk.len() - 1).min(total);
        eprintln!("Fetching {start}-{end} of {total}...");

        let id_param = chunk.join(",");
        let url =
            format!("{YOUTUBE_API_BASE}?part=snippet,contentDetails&id={id_param}&key={api_key}");

        // ureq 3.x returns Err for non-2xx status codes
        let mut response = match ureq::get(&url).call() {
            Ok(resp) => resp,
            Err(ureq::Error::StatusCode(403)) => {
                bail!(
                    "YouTube API returned 403 Forbidden. Check your API key \
                     and ensure the YouTube Data API v3 is enabled."
                );
            }
            Err(ureq::Error::StatusCode(code)) => {
                bail!("YouTube API returned HTTP {code}");
            }
            Err(e) => {
                return Err(anyhow::anyhow!(e).context("failed to reach YouTube Data API"));
            }
        };

        let body: Value = response
            .body_mut()
            .read_json()
            .context("failed to parse YouTube API response")?;

        let items = body["items"]
            .as_array()
            .context("unexpected API response: missing 'items' array")?;

        let now = Utc::now();

        for item in items {
            let id = item["id"].as_str().unwrap_or_default().to_string();
            let snippet = &item["snippet"];
            let content_details = &item["contentDetails"];

            let title = snippet["title"]
                .as_str()
                .unwrap_or("Unknown Title")
                .to_string();

            let channel = snippet["channelTitle"]
                .as_str()
                .unwrap_or("Unknown Channel")
                .to_string();

            let channel_id = snippet["channelId"]
                .as_str()
                .unwrap_or_default()
                .to_string();

            let published_at = snippet["publishedAt"]
                .as_str()
                .and_then(|s| s.parse::<DateTime<Utc>>().ok())
                .unwrap_or(now);

            let category_id = snippet["categoryId"]
                .as_str()
                .unwrap_or_default()
                .to_string();

            let tags = snippet["tags"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();

            let duration = content_details["duration"]
                .as_str()
                .unwrap_or("PT0S")
                .to_string();
            let duration_seconds = parse_iso8601_duration(&duration).unwrap_or(0);

            all_metadata.push(VideoMeta {
                id,
                title,
                channel,
                channel_id,
                duration,
                duration_seconds,
                published_at,
                category_id,
                tags,
                fetched_at: now,
                unavailable: false,
            });
        }
    }

    Ok(all_metadata)
}

/// Fetches YouTube video categories for the US region.
/// Returns a HashMap mapping category ID (e.g., "10") to name (e.g., "Music").
pub fn fetch_categories(api_key: &str) -> Result<HashMap<String, String>> {
    let url = format!("{YOUTUBE_CATEGORIES_API}?part=snippet&regionCode=US&key={api_key}");

    let mut response = match ureq::get(&url).call() {
        Ok(resp) => resp,
        Err(ureq::Error::StatusCode(code)) => {
            bail!("YouTube Categories API returned HTTP {code}");
        }
        Err(e) => {
            return Err(anyhow::anyhow!(e).context("failed to reach YouTube Categories API"));
        }
    };

    let body: Value = response
        .body_mut()
        .read_json()
        .context("failed to parse YouTube Categories API response")?;

    let items = body["items"]
        .as_array()
        .context("unexpected Categories API response: missing 'items' array")?;

    let mut categories = HashMap::new();
    for item in items {
        let id = item["id"].as_str().unwrap_or_default().to_string();
        let title = item["snippet"]["title"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        if !id.is_empty() && !title.is_empty() {
            categories.insert(id, title);
        }
    }

    Ok(categories)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_duration_hours_minutes_seconds() {
        assert_eq!(parse_iso8601_duration("PT1H2M3S"), Some(3723));
    }

    #[test]
    fn parse_duration_minutes_seconds() {
        assert_eq!(parse_iso8601_duration("PT3M33S"), Some(213));
    }

    #[test]
    fn parse_duration_seconds_only() {
        assert_eq!(parse_iso8601_duration("PT45S"), Some(45));
    }

    #[test]
    fn parse_duration_minutes_only() {
        assert_eq!(parse_iso8601_duration("PT10M"), Some(600));
    }

    #[test]
    fn parse_duration_hours_only() {
        assert_eq!(parse_iso8601_duration("PT2H"), Some(7200));
    }

    #[test]
    fn parse_duration_hours_seconds() {
        assert_eq!(parse_iso8601_duration("PT1H30S"), Some(3630));
    }

    #[test]
    fn parse_duration_zero() {
        assert_eq!(parse_iso8601_duration("PT0S"), Some(0));
    }

    #[test]
    fn parse_duration_invalid() {
        assert_eq!(parse_iso8601_duration("invalid"), None);
        assert_eq!(parse_iso8601_duration(""), None);
        assert_eq!(parse_iso8601_duration("P1D"), None);
    }

    #[test]
    fn format_duration_with_hours() {
        assert_eq!(format_duration(3723), "1:02:03");
    }

    #[test]
    fn format_duration_minutes_seconds() {
        assert_eq!(format_duration(213), "3:33");
    }

    #[test]
    fn format_duration_seconds_only() {
        assert_eq!(format_duration(45), "0:45");
    }

    #[test]
    fn format_duration_zero() {
        assert_eq!(format_duration(0), "0:00");
    }

    #[test]
    fn format_duration_exact_hour() {
        assert_eq!(format_duration(3600), "1:00:00");
    }
}

use std::collections::HashMap;
use std::sync::LazyLock;
use std::time::Duration;

use crate::models::VideoMeta;
use crate::youtube;

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use regex::Regex;
use serde_json::Value;

const YOUTUBE_API_BASE: &str = "https://www.googleapis.com/youtube/v3/videos";
const YOUTUBE_CATEGORIES_API: &str = "https://www.googleapis.com/youtube/v3/videoCategories";

/// Maximum number of video IDs per API request (YouTube API limit).
const BATCH_SIZE: usize = 50;
const HTTP_TIMEOUT: Duration = Duration::from_secs(30);

static HTTP_AGENT: LazyLock<ureq::Agent> = LazyLock::new(|| {
    ureq::Agent::config_builder()
        .https_only(true)
        .timeout_global(Some(HTTP_TIMEOUT))
        .user_agent(concat!("ytq/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
});

/// Regex for parsing ISO 8601 durations (e.g., PT1H2M3S, PT3M33S, PT45S).
static DURATION_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^PT(?:(\d+)H)?(?:(\d+)M)?(?:(\d+)S)?$").unwrap());

/// Parses an ISO 8601 duration string (e.g., "PT3M33S") into total seconds.
pub fn parse_iso8601_duration(duration: &str) -> Option<u64> {
    let caps = DURATION_RE.captures(duration)?;
    if caps.get(1).is_none() && caps.get(2).is_none() && caps.get(3).is_none() {
        return None;
    }

    let parse_component = |index| {
        caps.get(index)
            .map(|value| value.as_str().parse::<u64>().ok())
            .unwrap_or(Some(0))
    };
    let hours = parse_component(1)?;
    let minutes = parse_component(2)?;
    let seconds = parse_component(3)?;

    hours
        .checked_mul(3600)?
        .checked_add(minutes.checked_mul(60)?)?
        .checked_add(seconds)
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
/// Videos that are deleted/private/unavailable are silently skipped.
///
/// `on_chunk` is invoked with the metadata resolved from each request as soon
/// as that request completes, so callers can persist incrementally. A large
/// backlog takes hundreds of requests, and a failure partway through should not
/// throw away everything already downloaded.
pub fn fetch_video_metadata<F>(ids: &[String], api_key: &str, mut on_chunk: F) -> Result<()>
where
    F: FnMut(Vec<VideoMeta>) -> Result<()>,
{
    let total = ids.len();

    for (chunk_idx, chunk) in ids.chunks(BATCH_SIZE).enumerate() {
        let start = chunk_idx * BATCH_SIZE + 1;
        let end = (start + chunk.len() - 1).min(total);
        eprintln!("Fetching {start}-{end} of {total}...");

        let id_param = chunk.join(",");

        // Query values are encoded by ureq, rather than interpolated into a
        // URL. This keeps unusual key characters from changing the request.
        let request = HTTP_AGENT
            .get(YOUTUBE_API_BASE)
            .query("part", "snippet,contentDetails")
            .query("id", &id_param)
            .query("key", api_key);

        // ureq 3.x returns Err for non-2xx status codes.
        let mut response = match request.call() {
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
        let mut chunk_metadata = Vec::with_capacity(items.len());

        for item in items {
            let id = item["id"].as_str().unwrap_or_default().to_string();

            // Defense in depth: skip any response item whose id doesn't match
            // the canonical 11-character video ID format. The YouTube API
            // should never return malformed IDs, but persisting only validated
            // values keeps the database invariants identical to direct adds.
            if !youtube::is_valid_id_format(&id) {
                eprintln!("Warning: Skipping API response item with invalid id '{id}'");
                continue;
            }

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

            chunk_metadata.push(VideoMeta {
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

        on_chunk(chunk_metadata)?;
    }

    Ok(())
}

/// Fetches YouTube video categories for the US region.
/// Returns a HashMap mapping category ID (e.g., "10") to name (e.g., "Music").
pub fn fetch_categories(api_key: &str) -> Result<HashMap<String, String>> {
    let request = HTTP_AGENT
        .get(YOUTUBE_CATEGORIES_API)
        .query("part", "snippet")
        .query("regionCode", "US")
        .query("key", api_key);

    let mut response = match request.call() {
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
        assert_eq!(parse_iso8601_duration("PT"), None);
        assert_eq!(parse_iso8601_duration("P1D"), None);
        assert_eq!(parse_iso8601_duration("PT18446744073709551615H"), None);
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

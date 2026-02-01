use std::sync::LazyLock;

use anyhow::{Result, anyhow, bail};
use regex::Regex;
use url::Url;

/// Video IDs are exactly 11 characters: A-Z, a-z, 0-9, hyphen, underscore.
static VIDEO_ID_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-zA-Z0-9_-]{11}$").unwrap());

pub fn extract_video_id(input: &str) -> Result<String> {
    let input = input.trim();

    // Handle video IDs directly
    if is_valid_id_format(input) {
        return Ok(input.to_string());
    }

    // Handle partial links
    let url_string = if input.contains("://") {
        input.to_string()
    } else {
        format!("https://{input}")
    };

    // Parse URL
    let parsed = Url::parse(&url_string).map_err(|_| anyhow!("Invalid URL format"))?;

    let id = if let Some(host) = parsed.host_str() {
        if host == "youtu.be" {
            // Case: youtu.be/ID
            let path = parsed.path().trim_start_matches('/');
            let id: String = path.chars().take(11).collect();
            if !is_valid_id_format(&id) {
                bail!("Invalid video ID in youtu.be URL");
            }
            id
        } else if host.ends_with("youtube.com") {
            let path = parsed.path();
            let query = parsed.query();

            // Check for unsupported URL types first (provides specific error messages)
            check_unsupported_url(path, query)?;

            // Try path-based extraction (shorts, live, embed, v)
            if let Some(id) = extract_id_from_path(path) {
                id
            } else {
                // Fall back to ?v= parameter (standard watch URLs)
                parsed
                    .query_pairs()
                    .find(|(k, _)| k == "v")
                    .map(|(_, v)| v.to_string())
                    .ok_or_else(|| {
                        anyhow!(
                            "Could not find video ID. Supported formats:\n  \
                             - youtube.com/watch?v=ID\n  \
                             - youtube.com/shorts/ID\n  \
                             - youtube.com/live/ID\n  \
                             - youtu.be/ID"
                        )
                    })?
            }
        } else {
            bail!("Not a YouTube domain");
        }
    } else {
        bail!("Invalid URL");
    };

    // Final validation
    if is_valid_id_format(&id) {
        Ok(id)
    } else {
        bail!(
            "Extracted ID '{id}' is invalid (must be exactly 11 characters: A-Z, a-z, 0-9, -, _)"
        );
    }
}

/// Path prefixes that contain a video ID directly after them.
/// Order doesn't matter since we check all prefixes.
/// Note: `/watch/` is a less common format (e.g., youtube.com/watch/ID) distinct from
/// the standard `/watch?v=ID` query parameter format, which is handled separately.
const VIDEO_PATH_PREFIXES: &[&str] = &["/shorts/", "/live/", "/embed/", "/v/", "/e/", "/watch/"];

/// Extract video ID from path-based YouTube URLs.
///
/// Matches patterns like `/shorts/ID`, `/live/ID`, `/embed/ID`, `/v/ID`, `/e/ID`.
/// The ID is always exactly 11 characters following the prefix.
fn extract_id_from_path(path: &str) -> Option<String> {
    VIDEO_PATH_PREFIXES.iter().find_map(|prefix| {
        path.strip_prefix(prefix).and_then(|rest| {
            let id: String = rest.chars().take(11).collect();
            is_valid_id_format(&id).then_some(id)
        })
    })
}

/// Check if URL is an unsupported YouTube page type (channel, playlist, search)
fn check_unsupported_url(path: &str, query: Option<&str>) -> Result<()> {
    // Channel URLs
    if path.starts_with("/@")
        || path.starts_with("/channel/")
        || path.starts_with("/c/")
        || path.starts_with("/user/")
    {
        bail!(
            "Channel URLs are not supported. Please provide a direct video link (e.g., youtube.com/watch?v=ID)"
        );
    }

    // Playlist URLs
    if path.starts_with("/playlist")
        || query.is_some_and(|q| q.contains("list=") && !q.contains("v="))
    {
        bail!(
            "Playlist URLs are not supported. Please provide a direct video link (e.g., youtube.com/watch?v=ID)"
        );
    }

    // Search results
    if path.starts_with("/results") {
        bail!("Search result URLs are not supported. Please provide a direct video link.");
    }

    Ok(())
}

/// Build a canonical YouTube URL from a video ID.
///
/// Always returns the standard `watch?v=ID` format for consistency.
///
/// # Why Canonicalize to `watch?v=ID`?
///
/// All YouTube video URL formats (shorts, live, embed, youtu.be) refer to the same
/// underlying video ID and can be accessed via the standard watch URL:
///
/// - **Shorts**: `youtube.com/shorts/ID` → `youtube.com/watch?v=ID` works identically.
///   Browser extensions exist specifically to redirect shorts to the standard player.
/// - **Live**: `youtube.com/live/ID` → `youtube.com/watch?v=ID` works for both active
///   and archived streams. The `/live/` format is just a convenience URL.
/// - **Embed/v/e**: These are embedding formats that map 1:1 to watch URLs.
/// - **youtu.be**: YouTube's URL shortener, redirects to `watch?v=ID`.
///
/// Using a canonical format ensures:
/// 1. Consistent deduplication (same video isn't added twice with different URL formats)
/// 2. Predictable behavior when opening videos
/// 3. Compatibility with the standard YouTube player (vs. shorts vertical player)
pub fn build_canonical_url(video_id: &str) -> String {
    format!("https://www.youtube.com/watch?v={video_id}")
}

fn is_valid_id_format(id: &str) -> bool {
    VIDEO_ID_RE.is_match(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_video_id_direct() {
        assert_eq!(extract_video_id("dQw4w9WgXcQ").unwrap(), "dQw4w9WgXcQ");
    }

    #[test]
    fn valid_id_with_hyphen_underscore() {
        // IDs can contain hyphens and underscores
        assert_eq!(extract_video_id("abc-_123ABC").unwrap(), "abc-_123ABC");
    }

    #[test]
    fn full_youtube_url() {
        let result = extract_video_id("https://www.youtube.com/watch?v=dQw4w9WgXcQ");
        assert_eq!(result.unwrap(), "dQw4w9WgXcQ");
    }

    #[test]
    fn youtube_url_with_extra_params() {
        let result = extract_video_id("https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=42s");
        assert_eq!(result.unwrap(), "dQw4w9WgXcQ");
    }

    #[test]
    fn short_url() {
        let result = extract_video_id("https://youtu.be/dQw4w9WgXcQ");
        assert_eq!(result.unwrap(), "dQw4w9WgXcQ");
    }

    #[test]
    fn url_without_protocol() {
        let result = extract_video_id("youtube.com/watch?v=dQw4w9WgXcQ");
        assert_eq!(result.unwrap(), "dQw4w9WgXcQ");
    }

    #[test]
    fn url_without_www() {
        let result = extract_video_id("https://youtube.com/watch?v=dQw4w9WgXcQ");
        assert_eq!(result.unwrap(), "dQw4w9WgXcQ");
    }

    #[test]
    fn input_with_whitespace() {
        assert_eq!(extract_video_id("  dQw4w9WgXcQ  ").unwrap(), "dQw4w9WgXcQ");
    }

    #[test]
    fn invalid_domain() {
        let result = extract_video_id("https://vimeo.com/12345");
        assert!(result.is_err());
    }

    #[test]
    fn missing_v_param() {
        let result = extract_video_id("https://youtube.com/watch");
        assert!(result.is_err());
    }

    #[test]
    fn invalid_id_too_short() {
        let result = extract_video_id("tooshort");
        assert!(result.is_err());
    }

    #[test]
    fn invalid_id_too_long() {
        let result = extract_video_id("waytoolongforavideoid");
        assert!(result.is_err());
    }

    #[test]
    fn invalid_characters_in_id() {
        // ID with invalid characters (spaces, special chars)
        let result = extract_video_id("abc def!@#$");
        assert!(result.is_err());
    }

    // === New tests for shorts URLs ===
    #[test]
    fn shorts_url() {
        let result = extract_video_id("https://www.youtube.com/shorts/dQw4w9WgXcQ");
        assert_eq!(result.unwrap(), "dQw4w9WgXcQ");
    }

    #[test]
    fn shorts_url_with_query_params() {
        let result = extract_video_id("https://www.youtube.com/shorts/dQw4w9WgXcQ?feature=share");
        assert_eq!(result.unwrap(), "dQw4w9WgXcQ");
    }

    // === New tests for live URLs ===
    #[test]
    fn live_url() {
        let result = extract_video_id("https://www.youtube.com/live/dQw4w9WgXcQ");
        assert_eq!(result.unwrap(), "dQw4w9WgXcQ");
    }

    #[test]
    fn live_url_with_timestamp() {
        let result = extract_video_id("https://www.youtube.com/live/dQw4w9WgXcQ?t=123");
        assert_eq!(result.unwrap(), "dQw4w9WgXcQ");
    }

    // === New tests for embed URLs ===
    #[test]
    fn embed_url() {
        let result = extract_video_id("https://www.youtube.com/embed/dQw4w9WgXcQ");
        assert_eq!(result.unwrap(), "dQw4w9WgXcQ");
    }

    #[test]
    fn embed_url_with_params() {
        let result = extract_video_id("https://www.youtube.com/embed/dQw4w9WgXcQ?autoplay=1");
        assert_eq!(result.unwrap(), "dQw4w9WgXcQ");
    }

    // === New tests for v/ and e/ URLs (legacy embed formats) ===
    #[test]
    fn v_path_url() {
        let result = extract_video_id("https://www.youtube.com/v/dQw4w9WgXcQ");
        assert_eq!(result.unwrap(), "dQw4w9WgXcQ");
    }

    #[test]
    fn e_path_url() {
        let result = extract_video_id("https://www.youtube.com/e/dQw4w9WgXcQ");
        assert_eq!(result.unwrap(), "dQw4w9WgXcQ");
    }

    // === Channel URL errors ===
    #[test]
    fn channel_handle_url_error() {
        let result = extract_video_id("https://www.youtube.com/@SomeChannel");
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Channel URLs are not supported")
        );
    }

    #[test]
    fn channel_id_url_error() {
        let result = extract_video_id("https://www.youtube.com/channel/UCxxxxxxxx");
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Channel URLs are not supported")
        );
    }

    #[test]
    fn channel_c_url_error() {
        let result = extract_video_id("https://www.youtube.com/c/SomeChannel");
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Channel URLs are not supported")
        );
    }

    #[test]
    fn channel_user_url_error() {
        let result = extract_video_id("https://www.youtube.com/user/SomeUser");
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Channel URLs are not supported")
        );
    }

    // === Playlist URL errors ===
    #[test]
    fn playlist_url_error() {
        let result = extract_video_id("https://www.youtube.com/playlist?list=PLxxxxxxxxxx");
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Playlist URLs are not supported")
        );
    }

    // === Search URL errors ===
    #[test]
    fn search_results_url_error() {
        let result = extract_video_id("https://www.youtube.com/results?search_query=test");
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Search result URLs are not supported")
        );
    }

    // === Canonical URL builder ===
    #[test]
    fn build_canonical_url_test() {
        assert_eq!(
            build_canonical_url("dQw4w9WgXcQ"),
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
        );
    }

    // === Edge cases ===
    #[test]
    fn watch_url_with_playlist_param() {
        // Watch URLs with list= param should still work (video in playlist context)
        let result = extract_video_id("https://www.youtube.com/watch?v=dQw4w9WgXcQ&list=PLxxx");
        assert_eq!(result.unwrap(), "dQw4w9WgXcQ");
    }

    // === Tests from GitHub gist (rodrigoborgesdeoliveira/987683cfbfcc8d800192da1e73adc486) ===

    // /watch/ID format (without ?v= query param)
    #[test]
    fn watch_path_url() {
        let result = extract_video_id("https://www.youtube.com/watch/-wtIMTCHWuI");
        assert_eq!(result.unwrap(), "-wtIMTCHWuI");
    }

    #[test]
    fn watch_path_url_with_query() {
        let result = extract_video_id("https://www.youtube.com/watch/-wtIMTCHWuI?app=desktop");
        assert_eq!(result.unwrap(), "-wtIMTCHWuI");
    }

    // Mobile URLs (m.youtube.com)
    #[test]
    fn mobile_watch_url() {
        let result = extract_video_id("https://m.youtube.com/watch?v=lalOy8Mbfdc");
        assert_eq!(result.unwrap(), "lalOy8Mbfdc");
    }

    #[test]
    fn mobile_shorts_url() {
        let result = extract_video_id("https://m.youtube.com/shorts/j9rZxAF3C0I");
        assert_eq!(result.unwrap(), "j9rZxAF3C0I");
    }

    // URLs with si= tracking parameter (added by YouTube Share button in 2023)
    #[test]
    fn youtu_be_with_si_param() {
        let result = extract_video_id("https://youtu.be/M9bq_alk-sw?si=B_RZg_I-lLaa7UU-");
        assert_eq!(result.unwrap(), "M9bq_alk-sw");
    }

    // URLs with v= not as first param
    #[test]
    fn watch_url_v_not_first_param() {
        let result =
            extract_video_id("https://www.youtube.com/watch?feature=player_embedded&v=dQw4w9WgXcQ");
        assert_eq!(result.unwrap(), "dQw4w9WgXcQ");
    }

    // === YouTube Music URLs ===
    #[test]
    fn music_youtube_watch_url() {
        let result = extract_video_id("https://music.youtube.com/watch?v=dQw4w9WgXcQ");
        assert_eq!(result.unwrap(), "dQw4w9WgXcQ");
    }

    #[test]
    fn music_youtube_with_params() {
        let result =
            extract_video_id("https://music.youtube.com/watch?v=dQw4w9WgXcQ&list=RDAMVM123");
        assert_eq!(result.unwrap(), "dQw4w9WgXcQ");
    }

    // === HTTP URLs (vs HTTPS) ===
    #[test]
    fn http_watch_url() {
        let result = extract_video_id("http://www.youtube.com/watch?v=dQw4w9WgXcQ");
        assert_eq!(result.unwrap(), "dQw4w9WgXcQ");
    }

    #[test]
    fn http_youtu_be_url() {
        let result = extract_video_id("http://youtu.be/-wtIMTCHWuI");
        assert_eq!(result.unwrap(), "-wtIMTCHWuI");
    }

    // === Fragment timestamps ===
    #[test]
    fn watch_url_with_fragment_timestamp() {
        let result = extract_video_id("https://www.youtube.com/watch?v=0zM3nApSvMg#t=0m10s");
        assert_eq!(result.unwrap(), "0zM3nApSvMg");
    }

    // === Error path coverage ===

    #[test]
    fn invalid_youtu_be_url() {
        // ID too short in youtu.be URL
        let result = extract_video_id("https://youtu.be/tooshort");
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Invalid video ID in youtu.be URL")
        );
    }

    #[test]
    fn youtu_be_empty_path() {
        let result = extract_video_id("https://youtu.be/");
        assert!(result.is_err());
    }

    #[test]
    fn malformed_url() {
        // Not a valid URL and not a valid ID
        let result = extract_video_id("not://a:valid:url");
        assert!(result.is_err());
    }

    #[test]
    fn playlist_only_url_error() {
        // watch URL with list= but no v= should be rejected as playlist
        let result = extract_video_id("https://www.youtube.com/watch?list=PLxxxxxxxxxx");
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Playlist URLs are not supported")
        );
    }
}

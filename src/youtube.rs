use std::sync::LazyLock;

use anyhow::{anyhow, Result};
use regex::Regex;
use url::Url;

/// Video IDs are exactly 11 characters: A-Z, a-z, 0-9, hyphen, underscore.
static VIDEO_ID_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-zA-Z0-9_-]{11}$").unwrap());

pub fn extract_video_id(input: &str) -> Result<String> {
    let input = input.trim();

    // handle video IDs directly
    if is_valid_id_format(input) {
        return Ok(input.to_string());
    }

    // Handle partial links
    let url_string = if input.contains("://") {
        input.to_string()
    } else {
        format!("https://{input}")
    };

    // Parse URL and look for the ID
    let parsed = Url::parse(&url_string).map_err(|_| anyhow!("Invalid URL format"))?;

    let id = if let Some(host) = parsed.host_str() {
        if host.contains("youtu.be") {
            // Case: youtu.be/ID
            parsed.path().trim_start_matches('/').to_string()
        } else if host.contains("youtube.com") {
            // Case: youtube.com/watch?v=ID
            parsed
                .query_pairs()
                .find(|(k, _)| k == "v")
                .map(|(_, v)| v.to_string())
                .ok_or_else(|| anyhow!("Could not find 'v' parameter in YouTube URL"))?
        } else {
            return Err(anyhow!("Not a YouTube domain"));
        }
    } else {
        return Err(anyhow!("Invalid URL"));
    };

    // catch empty IDs or invalid formats
    if is_valid_id_format(&id) {
        Ok(id)
    } else {
        Err(anyhow!("Extracted ID '{id}' is invalid"))
    }
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
}

use anyhow::{anyhow, Result};
use regex::Regex;
use url::Url;

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
        format!("https://{}", input)
    };

    // Parse URL and look for the ID
    let parsed = Url::parse(&url_string).map_err(|_| anyhow!("Invalid URL format"))?;

    let id = if let Some(host) = parsed.host_str() {
        if host.contains("youtu.be") {
            // Case: youtu.be/ID
            parsed.path().trim_start_matches('/').to_string()
        } else if host.contains("youtube.com") {
            // Case: youtube.com/watch?v=ID
            parsed.query_pairs()
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
        Err(anyhow!("Extracted ID '{}' is invalid", id))
    }
}

/// YouTube IDs are exactly 11 characters,
/// containing A-Z, a-z, 0-9, - (hyphen), and _ (underscore).
fn is_valid_id_format(id: &str) -> bool {
    let re = Regex::new(r"^[a-zA-Z0-9_-]{11}$").unwrap();
    re.is_match(id)
}

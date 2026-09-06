use std::collections::HashMap;

use anyhow::{Result, bail};
use clap::Args;

/// Metadata constraints shared by search and opening commands.
#[derive(Args, Debug, Default)]
pub struct SelectionArgs {
    /// YouTube category ID or unambiguous cached name (e.g. tech)
    #[arg(long, value_name = "CATEGORY")]
    pub category: Option<String>,

    /// Case-insensitive substring of the cached channel name
    #[arg(long, value_name = "CHANNEL")]
    pub channel: Option<String>,

    /// Maximum video duration, inclusive (e.g. 10m, 30m, 1h, 90s)
    #[arg(long, value_name = "DURATION", value_parser = parse_duration)]
    pub max_duration: Option<i64>,
}

#[derive(Debug, Default)]
pub struct Selection {
    pub query: Option<String>,
    pub category_id: Option<String>,
    pub channel: Option<String>,
    pub max_duration: Option<i64>,
}

impl SelectionArgs {
    pub fn has_constraints(&self) -> bool {
        self.category.is_some() || self.channel.is_some() || self.max_duration.is_some()
    }

    pub fn resolve(
        &self,
        query: Option<&str>,
        categories: &HashMap<String, String>,
    ) -> Result<Selection> {
        Ok(Selection {
            query: query.map(|q| nonempty(q, "search query")).transpose()?,
            category_id: self
                .category
                .as_deref()
                .map(|value| resolve_category(value, categories))
                .transpose()?,
            channel: self
                .channel
                .as_deref()
                .map(|value| nonempty(value, "channel"))
                .transpose()?,
            max_duration: self.max_duration,
        })
    }
}

fn nonempty(value: &str, label: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() {
        bail!("{label} must not be empty");
    }
    Ok(value.to_lowercase())
}

fn resolve_category(value: &str, categories: &HashMap<String, String>) -> Result<String> {
    let value = nonempty(value, "category")?;
    // IDs work without a categories cache. Unknown IDs simply match no videos.
    if value.bytes().all(|b| b.is_ascii_digit()) {
        return Ok(value);
    }
    let mut matches: Vec<_> = categories
        .iter()
        .filter(|(_, name)| name.to_lowercase().contains(&value))
        .collect();
    matches.sort_by(|a, b| a.0.cmp(b.0));
    let exact: Vec<_> = matches
        .iter()
        .filter(|(_, name)| name.to_lowercase() == value)
        .collect();
    if exact.len() == 1 {
        return Ok(exact[0].0.clone());
    }
    match matches.as_slice() {
        [(id, _)] => Ok((*id).clone()),
        [] => bail!(
            "unknown category '{value}'; use a YouTube category ID or run `ytq fetch` to cache category names"
        ),
        _ => bail!(
            "ambiguous category '{value}': {}",
            matches
                .iter()
                .map(|(id, name)| format!("{name} ({id})"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

fn parse_duration(value: &str) -> Result<i64, String> {
    let value = value.trim();
    let (number, multiplier) = match value.as_bytes().last() {
        Some(b's') => (&value[..value.len() - 1], 1),
        Some(b'm') => (&value[..value.len() - 1], 60),
        Some(b'h') => (&value[..value.len() - 1], 3600),
        _ => return Err("expected a positive duration with unit s, m, or h (e.g. 30m)".into()),
    };
    if number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) {
        return Err("duration must be a positive whole number followed by s, m, or h".into());
    }
    number
        .parse::<i64>()
        .ok()
        .and_then(|n| n.checked_mul(multiplier))
        .filter(|n| *n > 0)
        .ok_or_else(|| "duration must be positive and fit in a 64-bit seconds count".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_accepts_explicit_units_and_rejects_invalid_values() {
        for (text, seconds) in [("90s", 90), ("10m", 600), ("30m", 1800), ("1h", 3600)] {
            assert_eq!(parse_duration(text).unwrap(), seconds);
        }
        for text in [
            "",
            "30",
            "0m",
            "-1h",
            "+1h",
            "1.5h",
            "1h30m",
            "∞",
            "999999999999999999h",
        ] {
            assert!(parse_duration(text).is_err(), "{text}");
        }
    }

    #[test]
    fn categories_accept_ids_exact_names_and_unambiguous_substrings() {
        let categories = HashMap::from([
            ("28".into(), "Science & Technology".into()),
            ("10".into(), "Music".into()),
            ("20".into(), "Music Discussion".into()),
        ]);
        assert_eq!(resolve_category(" TECH ", &categories).unwrap(), "28");
        assert_eq!(resolve_category("Music", &categories).unwrap(), "10");
        assert_eq!(resolve_category("28", &HashMap::new()).unwrap(), "28");
        assert!(
            resolve_category("mus", &categories)
                .unwrap_err()
                .to_string()
                .contains("ambiguous")
        );
        assert!(resolve_category("unknown", &categories).is_err());
        assert!(resolve_category(" ", &categories).is_err());
    }
}

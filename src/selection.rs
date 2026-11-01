use std::collections::HashMap;

use anyhow::{Result, bail};
use clap::Args;

/// Metadata constraints shared by every command that selects queued videos.
#[derive(Args, Debug, Default)]
pub struct SelectionArgs {
    /// YouTube category ID or unambiguous cached name (e.g. tech)
    #[arg(long, value_name = "CATEGORY", value_parser = parse_text)]
    pub category: Option<String>,

    /// Case-insensitive substring of the cached channel name
    #[arg(long, value_name = "CHANNEL", value_parser = parse_text)]
    pub channel: Option<String>,

    /// Maximum video duration, inclusive (e.g. 10m, 30m, 1h, 90s)
    #[arg(long, value_name = "DURATION", value_parser = parse_duration)]
    pub max_duration: Option<i64>,

    /// Minimum video duration, inclusive (e.g. 2m, 10m, 1h, 90s)
    #[arg(long, value_name = "DURATION", value_parser = parse_duration)]
    pub min_duration: Option<i64>,
}

/// Row-count flags shared by every command that displays queued videos.
#[derive(Args, Debug, Default)]
pub struct CountArgs {
    /// Show at most N videos (defaults: list all, peek 1, search 20)
    #[arg(long, value_name = "N", value_parser = parse_count, conflicts_with = "all")]
    pub limit: Option<usize>,

    /// Show every matching video, with no count cap (list already does)
    #[arg(long)]
    pub all: bool,
}

impl CountArgs {
    /// The row cap to apply, given the command's own default.
    pub fn cap(&self, default: Option<usize>) -> Option<usize> {
        if self.all {
            None
        } else {
            self.limit.or(default)
        }
    }
}

#[derive(Debug, Default)]
pub struct Selection {
    pub query: Option<String>,
    pub category_id: Option<String>,
    pub channel: Option<String>,
    pub max_duration: Option<i64>,
    pub min_duration: Option<i64>,
}

impl SelectionArgs {
    pub fn has_constraints(&self) -> bool {
        self.category.is_some()
            || self.channel.is_some()
            || self.max_duration.is_some()
            || self.min_duration.is_some()
    }

    pub fn resolve(
        &self,
        query: Option<&str>,
        load_categories: impl FnOnce() -> Result<HashMap<String, String>>,
    ) -> Result<Selection> {
        if let (Some(min), Some(max)) = (self.min_duration, self.max_duration)
            && min > max
        {
            bail!("minimum duration must not exceed maximum duration");
        }
        Ok(Selection {
            query: query.map(str::to_owned),
            category_id: self
                .category
                .as_deref()
                .map(|value| resolve_category(value, load_categories))
                .transpose()?,
            channel: self.channel.clone(),
            max_duration: self.max_duration,
            min_duration: self.min_duration,
        })
    }
}

/// Normalizes free-text matching input, which is always compared lowercase.
pub fn parse_text(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err("must not be empty".into());
    }
    Ok(value.to_lowercase())
}

/// The one parser for every row count. The ceiling is SQLite's integer range.
pub fn parse_count(value: &str) -> Result<usize, String> {
    value
        .parse::<i64>()
        .ok()
        .filter(|n| *n > 0)
        .and_then(|n| usize::try_from(n).ok())
        .ok_or_else(|| format!("expected a whole number from 1 to {}", i64::MAX))
}

fn resolve_category(
    value: &str,
    load_categories: impl FnOnce() -> Result<HashMap<String, String>>,
) -> Result<String> {
    // IDs work without a categories cache. Unknown IDs simply match no videos.
    if value.bytes().all(|b| b.is_ascii_digit()) {
        return Ok(value.to_owned());
    }
    let categories = load_categories()?;
    let mut matches: Vec<_> = categories
        .iter()
        .filter(|(_, name)| name.to_lowercase().contains(value))
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
    fn counts_accept_one_through_the_sqlite_ceiling() {
        assert_eq!(parse_count("1").unwrap(), 1);
        assert_eq!(
            parse_count("9223372036854775807").unwrap(),
            i64::MAX as usize
        );
        for text in [
            "",
            "0",
            "-1",
            "abc",
            "1.5",
            "9223372036854775808",
            "18446744073709551615",
            "18446744073709551616",
        ] {
            assert!(parse_count(text).is_err(), "{text}");
        }
    }

    #[test]
    fn text_is_trimmed_lowercased_and_never_empty() {
        assert_eq!(parse_text(" TECH ").unwrap(), "tech");
        assert!(parse_text("").is_err());
        assert!(parse_text("   ").is_err());
    }

    #[test]
    fn count_flags_override_the_default_and_all_removes_it() {
        let count = |limit, all| CountArgs { limit, all };
        assert_eq!(count(None, false).cap(Some(20)), Some(20));
        assert_eq!(count(None, false).cap(None), None);
        assert_eq!(count(Some(3), false).cap(Some(20)), Some(3));
        assert_eq!(count(None, true).cap(Some(20)), None);
    }

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
        let load = || Ok(categories.clone());
        assert_eq!(resolve_category("tech", load).unwrap(), "28");
        assert_eq!(resolve_category("music", load).unwrap(), "10");
        assert_eq!(resolve_category("28", load).unwrap(), "28");
        assert!(
            resolve_category("mus", load)
                .unwrap_err()
                .to_string()
                .contains("ambiguous")
        );
        assert!(resolve_category("unknown", load).is_err());
    }

    #[test]
    fn duration_range_rejects_only_an_inverted_pair() {
        let range = |min, max| SelectionArgs {
            min_duration: Some(min),
            max_duration: Some(max),
            ..SelectionArgs::default()
        };
        let no_categories = || Ok(HashMap::new());
        assert!(range(600, 600).resolve(None, no_categories).is_ok());
        assert!(range(60, 600).resolve(None, no_categories).is_ok());
        assert_eq!(
            range(601, 600)
                .resolve(None, no_categories)
                .unwrap_err()
                .to_string(),
            "minimum duration must not exceed maximum duration"
        );
    }

    #[test]
    fn categories_load_only_to_resolve_a_name() {
        let unreadable = || bail!("categories cache is unreadable");
        let by_id = SelectionArgs {
            category: Some("28".into()),
            ..SelectionArgs::default()
        };
        assert_eq!(
            by_id.resolve(None, unreadable).unwrap().category_id,
            Some("28".into())
        );
        assert!(
            SelectionArgs::default()
                .resolve(None, unreadable)
                .unwrap()
                .category_id
                .is_none()
        );
        let by_name = SelectionArgs {
            category: Some("tech".into()),
            ..SelectionArgs::default()
        };
        assert!(
            by_name
                .resolve(None, unreadable)
                .unwrap_err()
                .to_string()
                .contains("unreadable")
        );
    }
}

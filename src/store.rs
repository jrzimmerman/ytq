use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::path::Path;

use crate::models::{Config, Event};

use anyhow::{Context, Result};
use chrono::Datelike;

pub fn load_config(path: &Path) -> Result<Config> {
    let data = match fs::read_to_string(path) {
        Ok(data) => data,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Config::default()),
        Err(error) => {
            return Err(error).with_context(|| format!("failed to read {}", path.display()));
        }
    };

    serde_json::from_str(&data).with_context(|| format!("failed to parse {}", path.display()))
}

pub fn save_config(path: &Path, config: &Config) -> Result<()> {
    let data = serde_json::to_string_pretty(config).context("failed to serialize configuration")?;
    // The config file can hold a YouTube Data API key, so keep it owner-only.
    write_atomic(path, &data, true)
}

pub fn log_event(history_dir: &Path, event: &Event) -> Result<()> {
    let year = event.timestamp.year();
    let month = event.timestamp.month();

    // Partition: ~/.local/share/ytq/history/2026-01.jsonl
    let log_file_path = history_dir.join(format!("{year}-{month:02}.jsonl"));

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_file_path)
        .with_context(|| format!("failed to open {}", log_file_path.display()))?;

    let mut log_entry =
        serde_json::to_string(event).context("failed to serialize history event")?;
    log_entry.push('\n');
    file.write_all(log_entry.as_bytes())
        .with_context(|| format!("failed to append to {}", log_file_path.display()))?;

    Ok(())
}

pub fn stream_history(history_dir: &Path) -> Result<Vec<Event>> {
    let mut events = Vec::new();

    let entries = match fs::read_dir(history_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(events),
        Err(error) => {
            return Err(error).with_context(|| format!("failed to read {}", history_dir.display()));
        }
    };

    for entry in entries {
        let entry = entry
            .with_context(|| format!("failed to read an entry in {}", history_dir.display()))?;
        let path = entry.path();

        if !path.is_file()
            || !path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("jsonl"))
        {
            continue;
        }

        let file = fs::File::open(&path)
            .with_context(|| format!("failed to open history file {}", path.display()))?;
        let reader = BufReader::new(file);
        for (index, line) in reader.lines().enumerate() {
            let line = line
                .with_context(|| format!("failed to read {} line {}", path.display(), index + 1))?;
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<Event>(&line) {
                Ok(event) => events.push(event),
                Err(error) => eprintln!(
                    "Warning: skipping invalid history event in {} line {}: {error}",
                    path.display(),
                    index + 1
                ),
            }
        }
    }

    // Sort logic is critical now that we read multiple files.
    events.sort_by_key(|event| event.timestamp);

    Ok(events)
}

/// Loads YouTube video categories from categories.json.
/// Returns a HashMap mapping category ID to category name.
pub fn load_categories(path: &Path) -> Result<HashMap<String, String>> {
    let data = match fs::read_to_string(path) {
        Ok(data) => data,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(HashMap::new()),
        Err(error) => {
            return Err(error).with_context(|| format!("failed to read {}", path.display()));
        }
    };

    serde_json::from_str(&data).with_context(|| format!("failed to parse {}", path.display()))
}

/// Saves YouTube video categories to categories.json.
pub fn save_categories(path: &Path, categories: &HashMap<String, String>) -> Result<()> {
    let data =
        serde_json::to_string_pretty(categories).context("failed to serialize categories")?;
    write_atomic(path, &data, false)
}

/// Writes `data` to `path` via a temporary file in the same directory, then
/// renames it into place. A crash mid-write leaves the previous file intact
/// instead of truncating it, which matters because `config.json` holds the
/// user's API key.
///
/// When `private` is set, the file is restricted to the owner on Unix.
fn write_atomic(path: &Path, data: &str, private: bool) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("{} has no parent directory", path.display()))?;

    let file_name = path
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("{} has no file name", path.display()))?;
    let mut temp_name = file_name.to_os_string();
    temp_name.push(format!(".tmp.{}", std::process::id()));
    let temp_path = parent.join(temp_name);

    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    if private {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    #[cfg(not(unix))]
    let _ = private;

    let write_result = (|| -> Result<()> {
        let mut file = options
            .open(&temp_path)
            .with_context(|| format!("failed to create {}", temp_path.display()))?;
        file.write_all(data.as_bytes())
            .with_context(|| format!("failed to write {}", temp_path.display()))?;
        // Flush to disk before the rename so a power loss cannot leave an
        // empty file renamed over good data.
        file.sync_all()
            .with_context(|| format!("failed to flush {}", temp_path.display()))?;
        Ok(())
    })();

    if let Err(error) = write_result {
        let _ = fs::remove_file(&temp_path);
        return Err(error);
    }

    if let Err(error) = fs::rename(&temp_path, path) {
        let _ = fs::remove_file(&temp_path);
        return Err(error).with_context(|| format!("failed to write {}", path.display()));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use chrono::{TimeZone, Utc};

    use super::*;
    use crate::models::Action;

    static NEXT_TEMP_ID: AtomicUsize = AtomicUsize::new(0);

    fn temp_dir(label: &str) -> std::path::PathBuf {
        let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("ytq-store-{label}-{}-{id}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn missing_config_uses_defaults() {
        let dir = temp_dir("missing-config");
        let config = load_config(&dir.join("config.json")).unwrap();
        assert_eq!(config.mode, crate::models::Mode::Queue);
        assert!(config.offline);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn malformed_config_is_reported() {
        let dir = temp_dir("malformed-config");
        let path = dir.join("config.json");
        fs::write(&path, "not json").unwrap();
        let error = load_config(&path).unwrap_err();
        assert!(error.to_string().contains("failed to parse"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn history_is_sorted_and_malformed_lines_are_skipped() {
        let dir = temp_dir("history");
        let path = dir.join("2025-01.jsonl");
        let later = Event {
            timestamp: Utc.with_ymd_and_hms(2025, 1, 2, 0, 0, 0).unwrap(),
            action: Action::Watched,
            video_id: "bbbbbbbbbb2".to_string(),
            time_in_queue_sec: Some(10),
        };
        let earlier = Event {
            timestamp: Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap(),
            action: Action::Queued,
            video_id: "aaaaaaaaaa1".to_string(),
            time_in_queue_sec: None,
        };
        fs::write(
            &path,
            format!(
                "{}\ninvalid\n{}\n",
                serde_json::to_string(&later).unwrap(),
                serde_json::to_string(&earlier).unwrap()
            ),
        )
        .unwrap();

        let events = stream_history(&dir).unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].video_id, "aaaaaaaaaa1");
        assert_eq!(events[1].video_id, "bbbbbbbbbb2");
        fs::remove_dir_all(dir).unwrap();
    }
}

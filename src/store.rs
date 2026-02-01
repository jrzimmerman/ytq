use crate::models::{Config, Event, Video};
use anyhow::Result;
use chrono::Datelike;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

pub fn load_queue(path: &Path) -> Vec<Video> {
    if let Ok(data) = fs::read_to_string(path) {
        serde_json::from_str(&data).unwrap_or_default()
    } else {
        Vec::new()
    }
}

pub fn save_queue(path: &Path, queue: &[Video]) -> Result<()> {
    let data = serde_json::to_string_pretty(queue)?;
    fs::write(path, data)?;
    Ok(())
}

pub fn load_config(path: &Path) -> Config {
    if let Ok(data) = fs::read_to_string(path) {
        serde_json::from_str(&data).unwrap_or_default()
    } else {
        Config::default()
    }
}

pub fn save_config(path: &Path, config: &Config) -> Result<()> {
    let data = serde_json::to_string_pretty(config)?;
    fs::write(path, data)?;
    Ok(())
}

pub fn log_event(history_dir: &Path, event: &Event) -> Result<()> {
    let year = event.timestamp.year();
    let month = event.timestamp.month();

    // Partition: ~/.local/share/ytq/history/2026-01.log
    let log_file_path = history_dir.join(format!("{year}-{month:02}.log"));

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_file_path)?;

    let log_entry = serde_json::to_string(&event)?;
    writeln!(file, "{log_entry}")?;

    Ok(())
}

pub fn stream_history(history_dir: &Path) -> Vec<Event> {
    let mut events = Vec::new();

    if let Ok(entries) = fs::read_dir(history_dir) {
        for entry in entries.flatten() {
            let path = entry.path();

            if path.is_file()
                && path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("log"))
                && let Ok(file) = fs::File::open(&path)
            {
                let reader = BufReader::new(file);
                for line in reader.lines().map_while(Result::ok) {
                    // Skip empty lines or bad JSON
                    if let Ok(event) = serde_json::from_str::<Event>(&line) {
                        events.push(event);
                    }
                }
            }
        }
    }

    // Sort logic is critical now that we read multiple files
    events.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

    events
}

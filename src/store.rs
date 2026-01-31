use crate::models::{Event, Video};
use anyhow::Result;
use chrono::Datelike;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

pub fn load_queue(path: &Path) -> Vec<Video> {
    if let Ok(data) = fs::read_to_string(path) {
        serde_json::from_str(&data).unwrap_or_default()
    } else {
        Vec::new()
    }
}

pub fn save_queue(path: &Path, queue: &Vec<Video>) -> Result<()> {
    let data = serde_json::to_string_pretty(queue)?;
    fs::write(path, data)?;
    Ok(())
}

pub fn log_event(history_dir: &Path, event: &Event) -> Result<()> {
    let year = event.timestamp.year();
    let month = event.timestamp.month();
    let log_file_path = history_dir.join(format!("{}-{:02}.log", year, month));

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_file_path)?;

    let log_entry = serde_json::to_string(&event)?;
    writeln!(file, "{}", log_entry)?;

    Ok(())
}

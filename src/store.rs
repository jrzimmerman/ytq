use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

use crate::models::{Config, Event, Video, VideoMeta};
use crate::paths::AppPaths;

use anyhow::Result;
use chrono::Datelike;
use fd_lock::RwLock;

/// Acquires an exclusive lock on the queue, loads it, runs the callback with
/// mutable access, and saves the result. The lock is held for the entire operation.
///
/// Use this for any operation that modifies the queue (add, remove, next).
pub fn with_queue<T, F>(paths: &AppPaths, f: F) -> Result<T>
where
    F: FnOnce(&mut Vec<Video>) -> Result<T>,
{
    // Open/create the lock file
    let lock_file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&paths.lock_file)?;

    // Acquire exclusive lock (blocks until available)
    let mut lock = RwLock::new(lock_file);
    let _guard = lock.write()?;

    // Load, modify, save while holding the lock
    let mut queue = load_queue(&paths.queue_file);
    let result = f(&mut queue)?;
    save_queue(&paths.queue_file, &queue)?;

    Ok(result)
    // Lock released when _guard drops
}

/// Acquires a shared lock on the queue and loads it for read-only access.
///
/// Use this for operations that only read the queue (list, peek).
pub fn with_queue_read<T, F>(paths: &AppPaths, f: F) -> Result<T>
where
    F: FnOnce(&[Video]) -> T,
{
    // Open/create the lock file
    let lock_file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&paths.lock_file)?;

    // Acquire shared lock (blocks if exclusive lock held, allows multiple readers)
    let lock = RwLock::new(lock_file);
    let _guard = lock.read()?;

    // Load and process while holding the lock
    let queue = load_queue(&paths.queue_file);
    let result = f(&queue);

    Ok(result)
    // Lock released when _guard drops
}

fn load_queue(path: &Path) -> Vec<Video> {
    if let Ok(data) = fs::read_to_string(path) {
        serde_json::from_str(&data).unwrap_or_default()
    } else {
        Vec::new()
    }
}

fn save_queue(path: &Path, queue: &[Video]) -> Result<()> {
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

    // Partition: ~/.local/share/ytq/history/2026-01.jsonl
    let log_file_path = history_dir.join(format!("{year}-{month:02}.jsonl"));

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
                && path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("jsonl"))
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

/// Loads video metadata from metadata.json.
/// Returns a HashMap keyed by video ID. Returns empty map if file is
/// missing or contains invalid JSON.
pub fn load_metadata(path: &Path) -> HashMap<String, VideoMeta> {
    if let Ok(data) = fs::read_to_string(path) {
        serde_json::from_str(&data).unwrap_or_default()
    } else {
        HashMap::new()
    }
}

/// Saves the full metadata map to metadata.json.
pub fn save_metadata(path: &Path, metadata: &HashMap<String, VideoMeta>) -> Result<()> {
    let data = serde_json::to_string_pretty(metadata)?;
    fs::write(path, data)?;
    Ok(())
}

/// Loads YouTube video categories from categories.json.
/// Returns a HashMap mapping category ID to category name.
pub fn load_categories(path: &Path) -> HashMap<String, String> {
    if let Ok(data) = fs::read_to_string(path) {
        serde_json::from_str(&data).unwrap_or_default()
    } else {
        HashMap::new()
    }
}

/// Saves YouTube video categories to categories.json.
pub fn save_categories(path: &Path, categories: &HashMap<String, String>) -> Result<()> {
    let data = serde_json::to_string_pretty(categories)?;
    fs::write(path, data)?;
    Ok(())
}

use crate::models::{Action, Event, Mode, Video};
use crate::{paths, store, youtube};
use anyhow::Result;
use chrono::{DateTime, Local, Utc};
use colored::*;

pub fn add(input: String) -> Result<()> {
    let paths = paths::AppPaths::init()?;
    let mut queue = store::load_queue(&paths.queue_file);

    // Normalize input
    let id = match youtube::extract_video_id(&input) {
        Ok(valid_id) => valid_id,
        Err(e) => {
            eprintln!("{} {}", "Error:".red(), e);
            return Ok(());
        }
    };

    // Standardize URL
    let url = format!("https://www.youtube.com/watch?v={}", id);

    // Deduplicate
    if queue.iter().any(|v| v.id == id) {
        println!("{} {}", "Video already in queue.".yellow(), input);
        return Ok(());
    }

    let video = Video {
        id: id.clone(),
        url: url.clone(),
        title: url.clone(),
        added_at: Utc::now(),
        meta: None,
    };

    queue.push(video);
    store::save_queue(&paths.queue_file, &queue)?;

    let event = Event {
        timestamp: Utc::now(),
        action: Action::QUEUED,
        video_id: id.clone(),
        video_title: url.clone(),
        meta: None,
        time_in_queue_sec: None,
    };
    store::log_event(&paths.history_dir, &event)?;

    println!("{} {}", "Added:".green(), id);
    Ok(())
}

pub fn next() -> Result<()> {
    let paths = paths::AppPaths::init()?;
    let cfg = store::load_config(&paths.config_file);
    let mut queue = store::load_queue(&paths.queue_file);

    if queue.is_empty() {
        println!("{}", "The queue is empty.".yellow());
        return Ok(());
    }

    let video = match cfg.mode {
        Mode::Queue => queue.remove(0),
        Mode::Stack => queue.pop().unwrap(),
    };

    store::save_queue(&paths.queue_file, &queue)?;

    let duration = Utc::now().signed_duration_since(video.added_at);
    let sec_in_queue = duration.num_seconds();

    let event = Event {
        timestamp: Utc::now(),
        action: Action::WATCHED,
        video_id: video.id.clone(),
        video_title: video.title.clone(),
        meta: video.meta,
        time_in_queue_sec: Some(sec_in_queue),
    };

    store::log_event(&paths.history_dir, &event)?;

    println!("{} {}", "Opening:".blue(), video.url);
    open::that(video.url)?;

    Ok(())
}

pub fn remove(target: String) -> Result<()> {
    let paths = paths::AppPaths::init()?;
    let mut queue = store::load_queue(&paths.queue_file);

    if queue.is_empty() {
        println!("Queue is empty.");
        return Ok(());
    }

    // Extract ID from input
    let target_id = match youtube::extract_video_id(&target) {
        Ok(id) => id,
        Err(e) => {
             eprintln!("{} {}", "Error:".red(), e);
             return Ok(());
        }
    };

    // Find position
    let index = queue.iter().position(|v| v.id == target_id);

    if let Some(idx) = index {
        let video = queue.remove(idx);
        store::save_queue(&paths.queue_file, &queue)?;

        let event = Event {
            timestamp: Utc::now(),
            action: Action::SKIPPED,
            video_id: video.id.clone(),
            video_title: video.title.clone(),
            meta: video.meta,
            time_in_queue_sec: None,
        };
        store::log_event(&paths.history_dir, &event)?;

        println!("{} {}", "Removed:".red(), video.id);
    } else {
        println!("{} Could not find video with ID '{}'", "Error:".red(), target_id);
    }
    Ok(())
}

pub fn list() -> Result<()> {
    let paths = paths::AppPaths::init()?;
    let queue = store::load_queue(&paths.queue_file);

    if queue.is_empty() {
        println!("Queue is empty.");
    } else {
        println!("{} videos in queue:", queue.len());
        for (i, v) in queue.iter().enumerate() {
            // Display in Local time for the user
            let local_time: DateTime<Local> = DateTime::from(v.added_at);
            println!("[{}] {} ({})", i + 1, v.id, local_time.format("%Y-%m-%d %H:%M"));
        }
    }
    Ok(())
}

pub fn peek(n: usize) -> Result<()> {
    let paths = paths::AppPaths::init()?;
    let cfg = store::load_config(&paths.config_file);
    let queue = store::load_queue(&paths.queue_file);

    if queue.is_empty() {
        println!("Queue is empty.");
        return Ok(());
    }

    println!("Next {} videos ({:?} mode):", n, cfg.mode);

    let iter: Box<dyn Iterator<Item = &Video>> = match cfg.mode {
        Mode::Queue => Box::new(queue.iter().take(n)),       // Top N
        Mode::Stack => Box::new(queue.iter().rev().take(n)), // Bottom N (Reversed)
    };

    for (i, v) in iter.enumerate() {
        let local_time: DateTime<Local> = DateTime::from(v.added_at);
        println!("[{}] {} ({})", i + 1, v.id, local_time.format("%H:%M"));
    }
    Ok(())
}

pub fn stats() -> Result<()> {
    let paths = paths::AppPaths::init()?;
    let events = store::stream_history(&paths.history_dir);

    let watched = events.iter().filter(|e| matches!(e.action, Action::WATCHED)).count();
    let added = events.iter().filter(|e| matches!(e.action, Action::QUEUED)).count();
    let skipped = events.iter().filter(|e| matches!(e.action, Action::SKIPPED)).count();

    println!("{}", "YTQ Stats".bold());
    println!("----------------");
    println!("Videos Added:   {}", added);
    println!("Videos Watched: {}", watched);
    println!("Videos Skipped: {}", skipped);
    Ok(())
}

pub fn config(key: String, value: String) -> Result<()> {
    let paths = paths::AppPaths::init()?;
    let mut cfg = store::load_config(&paths.config_file);

    match key.as_str() {
        "mode" => match value.to_lowercase().as_str() {
            "stack" => cfg.mode = Mode::Stack,
            "queue" => cfg.mode = Mode::Queue,
            _ => println!("Invalid mode. Use 'stack' or 'queue'."),
        },
        "offline" => match value.to_lowercase().as_str() {
            "true" => cfg.offline = true,
            "false" => cfg.offline = false,
            _ => println!("Invalid boolean. Use 'true' or 'false'."),
        },
        _ => println!("Unknown config key. Available: mode, offline"),
    }

    store::save_config(&paths.config_file, &cfg)?;
    println!("Config updated.");
    Ok(())
}

pub fn info() -> Result<()> {
    let paths = paths::AppPaths::init()?;

    println!("Data Paths");
    println!("-------------");
    println!("Config:  {}", paths.config_file.display());
    println!("Queue:   {}", paths.queue_file.display());
    println!("History: {}", paths.history_dir.display());

    let queue_exists = paths.queue_file.exists();
    println!("Queue File Exists? {}", queue_exists);

    Ok(())
}

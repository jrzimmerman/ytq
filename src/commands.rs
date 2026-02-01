use crate::models::{Action, Event, Mode, Video};
use crate::{paths, store, youtube};
use anyhow::{bail, Result};
use chrono::{DateTime, Local, Utc};
use colored::Colorize;
use either::Either;

pub fn add(input: &str) -> Result<()> {
    let paths = paths::AppPaths::init()?;
    let mut queue = store::load_queue(&paths.queue_file);

    // Normalize input
    let id = youtube::extract_video_id(input)?;

    // Standardize URL
    let url = format!("https://www.youtube.com/watch?v={id}");

    // Deduplicate
    if queue.iter().any(|v| v.id == id) {
        println!("{} {input}", "Video already in queue:".yellow());
        return Ok(());
    }

    let video = Video {
        id: id.clone(),
        url: url.clone(),
        added_at: Utc::now(),
    };

    queue.push(video);
    store::save_queue(&paths.queue_file, &queue)?;

    let event = Event {
        timestamp: Utc::now(),
        action: Action::Queued,
        video_id: id.clone(),
        time_in_queue_sec: None,
    };
    store::log_event(&paths.history_dir, &event)?;

    println!("{} {id}", "Added:".green());
    Ok(())
}

pub fn next() -> Result<()> {
    let paths = paths::AppPaths::init()?;
    let cfg = store::load_config(&paths.config_file);
    let mut queue = store::load_queue(&paths.queue_file);

    if queue.is_empty() {
        println!("{}", "Queue is empty.".yellow());
        return Ok(());
    }

    let video = match cfg.mode {
        Mode::Queue => queue.remove(0),
        Mode::Stack => queue.pop().expect("queue verified non-empty"),
    };

    store::save_queue(&paths.queue_file, &queue)?;

    let duration = Utc::now().signed_duration_since(video.added_at);
    let sec_in_queue = duration.num_seconds();

    let event = Event {
        timestamp: Utc::now(),
        action: Action::Watched,
        video_id: video.id.clone(),
        time_in_queue_sec: Some(sec_in_queue),
    };

    store::log_event(&paths.history_dir, &event)?;

    println!("{} {}", "Opening:".blue(), video.url);
    open::that(video.url)?;

    Ok(())
}

pub fn remove(target: &str) -> Result<()> {
    let paths = paths::AppPaths::init()?;
    let mut queue = store::load_queue(&paths.queue_file);

    if queue.is_empty() {
        println!("{}", "Queue is empty.".yellow());
        return Ok(());
    }

    // Extract ID from input
    let target_id = youtube::extract_video_id(target)?;

    // Find position
    let Some(idx) = queue.iter().position(|v| v.id == target_id) else {
        bail!("video with ID '{target_id}' not found in queue");
    };

    let video = queue.remove(idx);
    store::save_queue(&paths.queue_file, &queue)?;

    let event = Event {
        timestamp: Utc::now(),
        action: Action::Skipped,
        video_id: video.id.clone(),
        time_in_queue_sec: None,
    };
    store::log_event(&paths.history_dir, &event)?;

    println!("{} {}", "Removed:".red(), video.id);
    Ok(())
}

pub fn list() -> Result<()> {
    let paths = paths::AppPaths::init()?;
    let queue = store::load_queue(&paths.queue_file);

    if queue.is_empty() {
        println!("{}", "Queue is empty.".yellow());
    } else {
        println!("{} videos in queue:", queue.len());
        for (i, v) in queue.iter().enumerate() {
            // Display in Local time for the user
            let local_time: DateTime<Local> = DateTime::from(v.added_at);
            println!(
                "[{}] {} ({})",
                i + 1,
                v.id,
                local_time.format("%Y-%m-%d %H:%M")
            );
        }
    }
    Ok(())
}

pub fn peek(n: usize) -> Result<()> {
    let paths = paths::AppPaths::init()?;
    let cfg = store::load_config(&paths.config_file);
    let queue = store::load_queue(&paths.queue_file);

    if queue.is_empty() {
        println!("{}", "Queue is empty.".yellow());
        return Ok(());
    }

    println!("Next {n} video(s) ({:?} mode):", cfg.mode);

    let iter = match cfg.mode {
        Mode::Queue => Either::Left(queue.iter().take(n)),
        Mode::Stack => Either::Right(queue.iter().rev().take(n)),
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

    let watched = events
        .iter()
        .filter(|e| matches!(e.action, Action::Watched))
        .count();
    let added = events
        .iter()
        .filter(|e| matches!(e.action, Action::Queued))
        .count();
    let skipped = events
        .iter()
        .filter(|e| matches!(e.action, Action::Skipped))
        .count();

    println!("{}", "YTQ Stats".bold());
    println!("----------------");
    println!("Videos Added:   {added}");
    println!("Videos Watched: {watched}");
    println!("Videos Skipped: {skipped}");
    Ok(())
}

pub fn config(key: &str, value: &str) -> Result<()> {
    let paths = paths::AppPaths::init()?;
    let mut cfg = store::load_config(&paths.config_file);

    match key {
        "mode" => match value.to_lowercase().as_str() {
            "stack" => cfg.mode = Mode::Stack,
            "queue" => cfg.mode = Mode::Queue,
            _ => bail!("invalid mode '{value}': use 'stack' or 'queue'"),
        },
        _ => bail!("unknown config key '{key}': available keys are 'mode'"),
    }

    store::save_config(&paths.config_file, &cfg)?;
    println!("{}", "Config updated.".green());
    Ok(())
}

pub fn info() -> Result<()> {
    let paths = paths::AppPaths::init()?;

    println!("{}", "Data Paths".bold());
    println!("-------------");
    println!("Config:  {}", paths.config_file.display());
    println!("Queue:   {}", paths.queue_file.display());
    println!("History: {}", paths.history_dir.display());

    let queue_exists = paths.queue_file.exists();
    println!("Queue File Exists? {queue_exists}");

    Ok(())
}

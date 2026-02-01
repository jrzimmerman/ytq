use crate::models::{Action, Event, Mode, Video};
use crate::{paths, store, youtube};
use anyhow::{Result, bail};
use chrono::{DateTime, Local, Utc};
use colored::Colorize;
use either::Either;
use rand::Rng;

pub fn add(input: &str) -> Result<()> {
    let paths = paths::AppPaths::init()?;

    // Normalize input before acquiring lock
    let id = youtube::extract_video_id(input)?;
    let url = youtube::build_canonical_url(&id);

    let added = store::with_queue(&paths, |queue| {
        // Deduplicate
        if queue.iter().any(|v| v.id == id) {
            return Ok(false);
        }

        let video = Video {
            id: id.clone(),
            url: url.clone(),
            added_at: Utc::now(),
        };

        queue.push(video);
        Ok(true)
    })?;

    if added {
        let event = Event {
            timestamp: Utc::now(),
            action: Action::Queued,
            video_id: id.clone(),
            time_in_queue_sec: None,
        };
        store::log_event(&paths.history_dir, &event)?;

        println!("{} {id}", "Added:".green());
    } else {
        println!("{} {input}", "Video already in queue:".yellow());
    }

    Ok(())
}

pub fn next(target: Option<&str>) -> Result<()> {
    let paths = paths::AppPaths::init()?;
    let cfg = store::load_config(&paths.config_file);

    // If a specific target is provided, parse it before acquiring the lock
    let target_id = target.map(youtube::extract_video_id).transpose()?;

    // Remove the video from queue while holding the lock
    let video = store::with_queue(&paths, |queue| {
        if queue.is_empty() {
            return Ok(None);
        }

        let video = match &target_id {
            // Specific video requested - find by ID
            Some(id) => {
                let idx = queue
                    .iter()
                    .position(|v| v.id == *id)
                    .ok_or_else(|| anyhow::anyhow!("video with ID '{id}' not found in queue"))?;
                queue.remove(idx)
            }
            // No target - use mode-based selection
            None => match cfg.mode {
                Mode::Queue => queue.remove(0),
                Mode::Stack => queue.pop().expect("queue verified non-empty"),
            },
        };

        Ok(Some(video))
    })?;

    let Some(video) = video else {
        println!("{}", "Queue is empty.".yellow());
        return Ok(());
    };

    // Log event and open video (outside the lock)
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
    open::that(&video.url)?;

    Ok(())
}

pub fn remove(target: &str) -> Result<()> {
    let paths = paths::AppPaths::init()?;

    // Extract ID from input before acquiring lock
    let target_id = youtube::extract_video_id(target)?;

    let video = store::with_queue(&paths, |queue| {
        if queue.is_empty() {
            return Ok(None);
        }

        // Find by ID
        let idx = queue
            .iter()
            .position(|v| v.id == target_id)
            .ok_or_else(|| anyhow::anyhow!("video with ID '{target_id}' not found in queue"))?;

        Ok(Some(queue.remove(idx)))
    })?;

    let Some(video) = video else {
        println!("{}", "Queue is empty.".yellow());
        return Ok(());
    };

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

    store::with_queue_read(&paths, |queue| {
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
    })
}

pub fn peek(n: usize) -> Result<()> {
    let paths = paths::AppPaths::init()?;
    let cfg = store::load_config(&paths.config_file);

    store::with_queue_read(&paths, |queue| {
        if queue.is_empty() {
            println!("{}", "Queue is empty.".yellow());
            return;
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
    })
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

pub fn random() -> Result<()> {
    let paths = paths::AppPaths::init()?;

    let video = store::with_queue(&paths, |queue| {
        if queue.is_empty() {
            return Ok(None);
        }

        let idx = rand::rng().random_range(0..queue.len());
        Ok(Some(queue.remove(idx)))
    })?;

    let Some(video) = video else {
        println!("{}", "Queue is empty.".yellow());
        return Ok(());
    };

    // Log event and open video
    let duration = Utc::now().signed_duration_since(video.added_at);
    let event = Event {
        timestamp: Utc::now(),
        action: Action::Watched,
        video_id: video.id.clone(),
        time_in_queue_sec: Some(duration.num_seconds()),
    };
    store::log_event(&paths.history_dir, &event)?;

    println!("{} {}", "Opening:".blue(), video.url);
    open::that(&video.url)?;

    Ok(())
}

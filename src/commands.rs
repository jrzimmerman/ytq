use crate::models::{Action, Event, Video};
use crate::{paths, store};
use anyhow::Result;
use chrono::Utc;
use colored::*;

pub fn add(url: String) -> Result<()> {
    let paths = paths::AppPaths::init()?;
    let mut queue = store::load_queue(&paths.queue_file);

    let id = url.clone();

    if queue.iter().any(|v| v.id == id) {
        println!("{}", "Video is already in the queue.".yellow());
        return Ok(());
    }

    let video = Video {
        id: id.clone(),
        url: url.clone(),
        added_at: Utc::now(),
        metadata: None,
    };

    queue.push(video);
    store::save_queue(&paths.queue_file, &queue)?;

    let event = Event {
        timestamp: Utc::now(),
        action: Action::QUEUED,
        video_id: id,
    };
    store::log_event(&paths.history_dir, &event)?;

    println!("{}", "Video added to the queue.".green());
    Ok(())
}

pub fn next() -> Result<()> {
    let paths = paths::AppPaths::init()?;
    let mut queue = store::load_queue(&paths.queue_file);

    if queue.is_empty() {
        println!("{}", "The queue is empty.".yellow());
        return Ok(());
    }

    let next_video = queue.remove(0);

    store::save_queue(&paths.queue_file, &queue)?;

    let event = Event {
        timestamp: Utc::now(),
        action: Action::WATCHED,
        video_id: next_video.id.clone(),
    };

    store::log_event(&paths.history_dir, &event)?;

    println!("{} {}", "Opening:".blue(), next_video.url);
    open::that(next_video.url)?;

    Ok(())
}

pub fn info() -> Result<()> {
    // This runs the exact same logic as 'add', so it will show the truth
    let paths = paths::AppPaths::init()?;

    println!("Data Paths");
    println!("-------------");
    // println!("Config:  {}", paths.config_file.display());
    println!("Queue:   {}", paths.queue_file.display());
    println!("History: {}", paths.history_dir.display());

    // Check if they actually exist
    let queue_exists = paths.queue_file.exists();
    println!("Queue File Exists? {}", queue_exists);

    Ok(())
}

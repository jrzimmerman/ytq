use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

#[derive(Serialize, Deserialize, Debug)]
pub enum Action {
    QUEUED,
    WATCHED,
    SKIPPED,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Video {
    pub id: String,
    pub url: String,
    pub added_at: DateTime<Utc>,
    pub metadata: Option<VideoMetadata>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct VideoMetadata {
    pub title: String,
    pub duration_seconds: u32,
    pub channel: String,
    pub tags: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Event {
    pub timestamp: DateTime<Utc>,
    pub action: Action,
    pub video_id: String,
}

use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Queue, // First In, First Out (FIFO)
    Stack, // Last In, First Out (LIFO)
}

// Default to Queue mode (FIFO)
impl Default for Mode {
    fn default() -> Self { Mode::Queue }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Config {
    #[serde(default)]
    pub mode: Mode,

    pub youtube_api_key: Option<String>,

    // Default to TRUE if missing from JSON
    #[serde(default = "default_offline")]
    pub offline: bool,
}

fn default_offline() -> bool { true }

impl Default for Config {
    fn default() -> Self {
        Self {
            mode: Mode::Queue,
            youtube_api_key: None,
            offline: true,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Video {
    pub id: String,
    pub url: String,
    pub title: String,
    pub added_at: DateTime<Utc>,
    pub meta: Option<VideoMeta>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct VideoMeta {
    pub channel: String,
    pub duration_seconds: u32,
    pub tags: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum Action {
    QUEUED,
    WATCHED,
    SKIPPED,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Event {
    pub timestamp: DateTime<Utc>,
    pub action: Action,
    pub video_id: String,
    pub video_title: String,
    pub meta: Option<VideoMeta>,
    pub time_in_queue_sec: Option<i64>,
}

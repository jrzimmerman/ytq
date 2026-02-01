use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    Queue, // First In, First Out (FIFO)
    Stack, // Last In, First Out (LIFO)
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Config {
    #[serde(default)]
    pub mode: Mode,
}

impl Default for Config {
    fn default() -> Self {
        Self { mode: Mode::Queue }
    }
}

// TODO: Add optional metadata fetching (title, channel, duration) via YouTube API
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Video {
    pub id: String,
    pub url: String,
    pub added_at: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum Action {
    Queued,
    Watched,
    Skipped,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Event {
    pub timestamp: DateTime<Utc>,
    pub action: Action,
    pub video_id: String,
    pub time_in_queue_sec: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_default_is_queue() {
        assert_eq!(Mode::default(), Mode::Queue);
    }

    #[test]
    fn config_default_values() {
        let cfg = Config::default();
        assert_eq!(cfg.mode, Mode::Queue);
    }

    #[test]
    fn config_serde_roundtrip() {
        let cfg = Config { mode: Mode::Stack };

        let json = serde_json::to_string(&cfg).unwrap();
        let parsed: Config = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.mode, Mode::Stack);
    }

    #[test]
    fn config_deserialize_with_defaults() {
        // JSON missing 'mode' field should use default
        let json = r#"{}"#;
        let cfg: Config = serde_json::from_str(json).unwrap();

        assert_eq!(cfg.mode, Mode::Queue);
    }

    #[test]
    fn mode_serializes_lowercase() {
        let json = serde_json::to_string(&Mode::Queue).unwrap();
        assert_eq!(json, r#""queue""#);

        let json = serde_json::to_string(&Mode::Stack).unwrap();
        assert_eq!(json, r#""stack""#);
    }

    #[test]
    fn video_serde_roundtrip() {
        let video = Video {
            id: "dQw4w9WgXcQ".to_string(),
            url: "https://youtube.com/watch?v=dQw4w9WgXcQ".to_string(),
            added_at: Utc::now(),
        };

        let json = serde_json::to_string(&video).unwrap();
        let parsed: Video = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.id, video.id);
        assert_eq!(parsed.url, video.url);
    }
}

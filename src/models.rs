use std::env;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    Queue, // First In, First Out (FIFO)
    Stack, // Last In, First Out (LIFO)
}

fn default_true() -> bool {
    true
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Config {
    #[serde(default)]
    pub mode: Mode,
    #[serde(default = "default_true")]
    pub offline: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub youtube_api_key: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            mode: Mode::Queue,
            offline: true,
            youtube_api_key: None,
        }
    }
}

impl Config {
    /// Returns the effective API key, checking the environment variable first,
    /// then falling back to the config file value.
    pub fn effective_api_key(&self) -> Option<String> {
        Self::resolve_api_key(env::var("YOUTUBE_DATA_API_KEY").ok(), &self.youtube_api_key)
    }

    /// Resolves the API key from an environment value and a config value.
    /// Environment variable takes precedence; empty strings are ignored.
    fn resolve_api_key(env_val: Option<String>, config_val: &Option<String>) -> Option<String> {
        env_val
            .filter(|k| !k.is_empty())
            .or_else(|| config_val.clone())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Video {
    pub id: String,
    pub url: String,
    pub added_at: DateTime<Utc>,
}

/// Video metadata fetched from the YouTube Data API v3.
/// Stored in a separate metadata.json sidecar file, keyed by video ID.
/// Videos that the API returns no data for are stored as tombstones
/// with `unavailable: true` so they aren't retried on every fetch.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct VideoMeta {
    pub id: String,
    pub title: String,
    pub channel: String,
    pub channel_id: String,
    pub duration: String,
    pub duration_seconds: u64,
    pub published_at: DateTime<Utc>,
    pub category_id: String,
    pub tags: Vec<String>,
    pub fetched_at: DateTime<Utc>,
    #[serde(default)]
    pub unavailable: bool,
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
        assert!(cfg.offline);
        assert!(cfg.youtube_api_key.is_none());
    }

    #[test]
    fn config_serde_roundtrip() {
        let cfg = Config {
            mode: Mode::Stack,
            offline: false,
            youtube_api_key: Some("test-key-123".to_string()),
        };

        let json = serde_json::to_string(&cfg).unwrap();
        let parsed: Config = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.mode, Mode::Stack);
        assert!(!parsed.offline);
        assert_eq!(parsed.youtube_api_key.as_deref(), Some("test-key-123"));
    }

    #[test]
    fn config_deserialize_with_defaults() {
        // JSON missing all new fields should use defaults (backward compat)
        let json = r#"{}"#;
        let cfg: Config = serde_json::from_str(json).unwrap();

        assert_eq!(cfg.mode, Mode::Queue);
        assert!(cfg.offline); // defaults to true
        assert!(cfg.youtube_api_key.is_none());
    }

    #[test]
    fn config_deserialize_legacy_mode_only() {
        // Old config.json with only "mode" should still parse
        let json = r#"{"mode":"stack"}"#;
        let cfg: Config = serde_json::from_str(json).unwrap();

        assert_eq!(cfg.mode, Mode::Stack);
        assert!(cfg.offline);
        assert!(cfg.youtube_api_key.is_none());
    }

    #[test]
    fn config_api_key_not_serialized_when_none() {
        let cfg = Config::default();
        let json = serde_json::to_string_pretty(&cfg).unwrap();
        assert!(!json.contains("youtube_api_key"));
    }

    #[test]
    fn resolve_api_key_env_takes_precedence() {
        let config_val = Some("config-key".to_string());
        let env_val = Some("env-key".to_string());

        let result = Config::resolve_api_key(env_val, &config_val);
        assert_eq!(result.as_deref(), Some("env-key"));
    }

    #[test]
    fn resolve_api_key_empty_env_falls_back_to_config() {
        let config_val = Some("config-key".to_string());
        let env_val = Some(String::new());

        let result = Config::resolve_api_key(env_val, &config_val);
        assert_eq!(result.as_deref(), Some("config-key"));
    }

    #[test]
    fn resolve_api_key_no_env_falls_back_to_config() {
        let config_val = Some("config-key".to_string());

        let result = Config::resolve_api_key(None, &config_val);
        assert_eq!(result.as_deref(), Some("config-key"));
    }

    #[test]
    fn resolve_api_key_none_when_both_missing() {
        let result = Config::resolve_api_key(None, &None);
        assert!(result.is_none());
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

    #[test]
    fn video_meta_serde_roundtrip() {
        let meta = VideoMeta {
            id: "dQw4w9WgXcQ".to_string(),
            title: "Never Gonna Give You Up".to_string(),
            channel: "Rick Astley".to_string(),
            channel_id: "UCuAXFkgsw1L7xaCfnd5JJOw".to_string(),
            duration: "PT3M33S".to_string(),
            duration_seconds: 213,
            published_at: Utc::now(),
            category_id: "10".to_string(),
            tags: vec!["rick astley".to_string(), "music".to_string()],
            fetched_at: Utc::now(),
            unavailable: false,
        };

        let json = serde_json::to_string(&meta).unwrap();
        let parsed: VideoMeta = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.id, meta.id);
        assert_eq!(parsed.title, meta.title);
        assert_eq!(parsed.channel, meta.channel);
        assert_eq!(parsed.channel_id, meta.channel_id);
        assert_eq!(parsed.duration, "PT3M33S");
        assert_eq!(parsed.duration_seconds, 213);
        assert_eq!(parsed.category_id, "10");
        assert_eq!(parsed.tags.len(), 2);
        assert!(!parsed.unavailable);
    }

    #[test]
    fn video_meta_unavailable_defaults_to_false() {
        // Existing metadata entries without the 'unavailable' field
        // should default to false via #[serde(default)]
        let json = r#"{
            "id":"dQw4w9WgXcQ","title":"T","channel":"C","channel_id":"UC",
            "duration":"PT1S","duration_seconds":1,
            "published_at":"2026-01-01T00:00:00Z","category_id":"10",
            "tags":[],"fetched_at":"2026-01-01T00:00:00Z"
        }"#;
        let parsed: VideoMeta = serde_json::from_str(json).unwrap();
        assert!(!parsed.unavailable);
    }

    #[test]
    fn video_meta_unavailable_tombstone() {
        let meta = VideoMeta {
            id: "deleted12345".to_string(),
            title: String::new(),
            channel: String::new(),
            channel_id: String::new(),
            duration: String::new(),
            duration_seconds: 0,
            published_at: Utc::now(),
            category_id: String::new(),
            tags: vec![],
            fetched_at: Utc::now(),
            unavailable: true,
        };

        let json = serde_json::to_string(&meta).unwrap();
        let parsed: VideoMeta = serde_json::from_str(&json).unwrap();
        assert!(parsed.unavailable);
    }

    #[test]
    fn video_meta_old_format_fails_to_parse() {
        // Old metadata entries missing new fields should fail deserialization
        let old_json = r#"{"id":"abc","title":"T","channel":"C","duration_seconds":10,"fetched_at":"2026-01-01T00:00:00Z"}"#;
        let result = serde_json::from_str::<VideoMeta>(old_json);
        assert!(result.is_err());
    }
}

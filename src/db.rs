use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use crate::models::{Mode, Video, VideoMeta};
use crate::selection::Selection;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, functions::FunctionFlags, named_params, params};

pub enum SelectionOrder {
    Queue,
    Stack,
    Random,
}

impl SelectionOrder {
    fn sql(&self) -> &'static str {
        match self {
            Self::Queue => "q.position ASC",
            Self::Stack => "q.position DESC",
            Self::Random => "RANDOM()",
        }
    }
}

// Literal substring matching: %, _ and quotes are data, not SQL patterns.
const SELECTION_WHERE: &str = "
    (:target IS NULL OR q.id = :target)
    AND (:query IS NULL OR instr(ytq_lower(q.id), :query) > 0
         OR (m.unavailable = 0 AND (
             instr(ytq_lower(m.title), :query) > 0
             OR instr(ytq_lower(m.channel), :query) > 0)))
    AND (:category IS NULL OR (m.unavailable = 0 AND m.category_id = :category))
    AND (:channel IS NULL OR (m.unavailable = 0 AND instr(ytq_lower(m.channel), :channel) > 0))
    AND (:duration IS NULL OR (m.unavailable = 0 AND m.duration_seconds > 0
         AND m.duration_seconds <= :duration))
";

const SCHEMA_SQL: &str = r#"
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
PRAGMA foreign_keys = ON;
PRAGMA busy_timeout = 5000;

CREATE TABLE IF NOT EXISTS queue (
    id          TEXT PRIMARY KEY,
    url         TEXT NOT NULL,
    added_at    TEXT NOT NULL,
    position    INTEGER NOT NULL UNIQUE
);
CREATE INDEX IF NOT EXISTS idx_queue_position ON queue(position);

CREATE TABLE IF NOT EXISTS metadata (
    id                TEXT PRIMARY KEY,
    title             TEXT NOT NULL,
    channel           TEXT NOT NULL,
    channel_id        TEXT NOT NULL,
    duration          TEXT NOT NULL,
    duration_seconds  INTEGER NOT NULL CHECK (duration_seconds >= 0),
    published_at      TEXT NOT NULL,
    category_id       TEXT NOT NULL,
    tags              TEXT NOT NULL DEFAULT '[]',
    fetched_at        TEXT NOT NULL,
    unavailable       INTEGER NOT NULL DEFAULT 0 CHECK (unavailable IN (0, 1)),
    CHECK (json_valid(tags))
);
CREATE INDEX IF NOT EXISTS idx_metadata_channel ON metadata(channel);
CREATE INDEX IF NOT EXISTS idx_metadata_category ON metadata(category_id);
"#;

/// Wrapper around a SQLite connection holding the ytq queue and metadata tables.
pub struct Db {
    conn: Connection,
}

/// A queue row removed by a command, including enough information to restore
/// its original ordering if a subsequent history write fails.
pub struct RemovedVideo {
    pub video: Video,
    position: i64,
}

impl Db {
    /// Opens the database at `path` and initializes its schema if needed.
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)
            .with_context(|| format!("failed to open database: {}", path.display()))?;
        conn.busy_timeout(Duration::from_secs(5))
            .context("failed to configure database busy timeout")?;
        let db = Self { conn };
        db.init_schema()?;
        Ok(db)
    }

    /// Opens an in-memory database with schema initialized. Used in tests.
    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let db = Self { conn };
        db.init_schema()?;
        Ok(db)
    }

    fn init_schema(&self) -> Result<()> {
        self.conn.create_scalar_function(
            "ytq_lower",
            1,
            FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
            |ctx| Ok(ctx.get::<Option<String>>(0)?.map(|s| s.to_lowercase())),
        )?;
        self.conn
            .execute_batch(SCHEMA_SQL)
            .context("failed to initialize database schema")?;
        Ok(())
    }

    // ---------------------------------------------------------------
    // Queue operations
    // ---------------------------------------------------------------

    /// Returns true if the video was inserted (false if a row with the same id
    /// already existed).
    pub fn add_video(&self, video: &Video) -> Result<bool> {
        let mut stmt = self.conn.prepare_cached(
            "INSERT OR IGNORE INTO queue (id, url, added_at, position) \
             VALUES (?1, ?2, ?3, COALESCE((SELECT MAX(position) FROM queue), 0) + 1)",
        )?;
        let n = stmt.execute(params![video.id, video.url, video.added_at])?;
        Ok(n > 0)
    }

    /// Removes and returns the front of the queue (lowest position).
    #[cfg(test)]
    pub fn pop_front(&self) -> Result<Option<Video>> {
        Ok(self.take_front()?.map(|removed| removed.video))
    }

    /// Removes and returns the back of the queue (highest position).
    #[cfg(test)]
    pub fn pop_back(&self) -> Result<Option<Video>> {
        Ok(self.take_back()?.map(|removed| removed.video))
    }

    #[cfg(test)]
    pub fn take_front(&self) -> Result<Option<RemovedVideo>> {
        self.take_matching(&Selection::default(), SelectionOrder::Queue, None)
    }

    #[cfg(test)]
    pub fn take_back(&self) -> Result<Option<RemovedVideo>> {
        self.take_matching(&Selection::default(), SelectionOrder::Stack, None)
    }

    /// Removes and returns a random video from the queue.
    #[cfg(test)]
    pub fn pop_random(&self) -> Result<Option<Video>> {
        Ok(self.take_random()?.map(|removed| removed.video))
    }

    #[cfg(test)]
    pub fn take_random(&self) -> Result<Option<RemovedVideo>> {
        self.take_matching(&Selection::default(), SelectionOrder::Random, None)
    }

    /// Removes the video with the given id and returns it. Returns Ok(None) when
    /// the id is not present.
    pub fn remove_video(&self, id: &str) -> Result<Option<Video>> {
        Ok(self.take_video(id)?.map(|removed| removed.video))
    }

    pub fn take_video(&self, id: &str) -> Result<Option<RemovedVideo>> {
        let mut stmt = self.conn.prepare_cached(
            "DELETE FROM queue WHERE id = ?1 RETURNING id, url, added_at, position",
        )?;
        let video = stmt
            .query_row(params![id], |row| {
                Ok(RemovedVideo {
                    video: Video {
                        id: row.get(0)?,
                        url: row.get(1)?,
                        added_at: row.get(2)?,
                    },
                    position: row.get(3)?,
                })
            })
            .optional()?;
        Ok(video)
    }

    /// Restores a removed queue row at its original position. Existing rows at
    /// that position or later are shifted forward while preserving their order.
    pub fn restore_video(&self, removed: &RemovedVideo) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("UPDATE queue SET position = -position", [])?;
        tx.execute(
            "UPDATE queue SET position = CASE \
             WHEN -position >= ?1 THEN -position + 1 ELSE -position END",
            params![removed.position],
        )?;
        tx.execute(
            "INSERT INTO queue (id, url, added_at, position) VALUES (?1, ?2, ?3, ?4)",
            params![
                removed.video.id,
                removed.video.url,
                removed.video.added_at,
                removed.position
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Search is read-only and returns matching videos in configured queue order.
    pub fn search_videos(
        &self,
        selection: &Selection,
        order: SelectionOrder,
        limit: Option<usize>,
    ) -> Result<Vec<Video>> {
        let limit = limit
            .map(i64::try_from)
            .transpose()
            .context("search limit is too large")?
            .unwrap_or(-1);
        let sql = format!(
            "SELECT q.id, q.url, q.added_at FROM queue q
             LEFT JOIN metadata m ON m.id = q.id
             WHERE {SELECTION_WHERE} ORDER BY {} LIMIT :limit",
            order.sql()
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(
            named_params! {
                ":target": Option::<&str>::None,
                ":query": selection.query,
                ":category": selection.category_id,
                ":channel": selection.channel,
                ":duration": selection.max_duration,
                ":limit": limit,
            },
            |row| {
                Ok(Video {
                    id: row.get(0)?,
                    url: row.get(1)?,
                    added_at: row.get(2)?,
                })
            },
        )?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    /// Choose and remove a matching video in one statement, so concurrent commands
    /// cannot open the same queue row. Nonmatching rows keep their positions.
    pub fn take_matching(
        &self,
        selection: &Selection,
        order: SelectionOrder,
        target: Option<&str>,
    ) -> Result<Option<RemovedVideo>> {
        let sql = format!(
            "DELETE FROM queue WHERE id = (
                SELECT q.id FROM queue q LEFT JOIN metadata m ON m.id = q.id
                WHERE {SELECTION_WHERE} ORDER BY {} LIMIT 1
             ) RETURNING id, url, added_at, position",
            order.sql()
        );
        self.conn
            .query_row(
                &sql,
                named_params! {
                    ":target": target,
                    ":query": selection.query,
                    ":category": selection.category_id,
                    ":channel": selection.channel,
                    ":duration": selection.max_duration,
                },
                |row| {
                    Ok(RemovedVideo {
                        video: Video {
                            id: row.get(0)?,
                            url: row.get(1)?,
                            added_at: row.get(2)?,
                        },
                        position: row.get(3)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    /// Lists all videos in FIFO order (oldest first).
    pub fn list_videos(&self) -> Result<Vec<Video>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, url, added_at FROM queue ORDER BY position ASC")?;
        let rows = stmt.query_map([], |row| {
            Ok(Video {
                id: row.get(0)?,
                url: row.get(1)?,
                added_at: row.get(2)?,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Returns the first or last `n` videos depending on `mode`.
    pub fn peek_videos(&self, n: usize, mode: &Mode) -> Result<Vec<Video>> {
        let order = match mode {
            Mode::Queue => "ASC",
            Mode::Stack => "DESC",
        };
        let sql = format!("SELECT id, url, added_at FROM queue ORDER BY position {order} LIMIT ?1");
        let mut stmt = self.conn.prepare(&sql)?;
        let limit = i64::try_from(n).context("peek count is too large")?;
        let rows = stmt.query_map(params![limit], |row| {
            Ok(Video {
                id: row.get(0)?,
                url: row.get(1)?,
                added_at: row.get(2)?,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Returns just the IDs of all videos in the queue (FIFO order).
    pub fn queue_ids(&self) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id FROM queue ORDER BY position ASC")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Total queue length.
    pub fn queue_len(&self) -> Result<usize> {
        let n: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM queue", [], |r| r.get(0))?;
        usize::try_from(n).context("queue count is outside the supported range")
    }

    // ---------------------------------------------------------------
    // Metadata operations
    // ---------------------------------------------------------------

    /// Upserts a batch of metadata entries inside a single transaction.
    pub fn upsert_metadata_batch(&self, items: &[VideoMeta]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO metadata (id, title, channel, channel_id, duration, duration_seconds, \
                 published_at, category_id, tags, fetched_at, unavailable) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11) \
                 ON CONFLICT(id) DO UPDATE SET \
                    title = excluded.title, \
                    channel = excluded.channel, \
                    channel_id = excluded.channel_id, \
                    duration = excluded.duration, \
                    duration_seconds = excluded.duration_seconds, \
                    published_at = excluded.published_at, \
                    category_id = excluded.category_id, \
                    tags = excluded.tags, \
                    fetched_at = excluded.fetched_at, \
                    unavailable = excluded.unavailable",
            )?;
            for m in items {
                let tags_json = serde_json::to_string(&m.tags)?;
                let duration_seconds = duration_to_sql(m.duration_seconds)?;
                stmt.execute(params![
                    m.id,
                    m.title,
                    m.channel,
                    m.channel_id,
                    m.duration,
                    duration_seconds,
                    m.published_at,
                    m.category_id,
                    tags_json,
                    m.fetched_at,
                    m.unavailable as i64,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Loads all metadata entries into a HashMap keyed by video id.
    pub fn load_all_metadata(&self) -> Result<HashMap<String, VideoMeta>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, channel, channel_id, duration, duration_seconds, \
             published_at, category_id, tags, fetched_at, unavailable FROM metadata",
        )?;
        let rows = stmt.query_map([], row_to_meta)?;
        let mut out = HashMap::new();
        for r in rows {
            let m = r?;
            out.insert(m.id.clone(), m);
        }
        Ok(out)
    }

    /// Load only the metadata needed for a displayed page, not the full library.
    pub fn load_metadata_for(&self, videos: &[Video]) -> Result<HashMap<String, VideoMeta>> {
        let ids = serde_json::to_string(&videos.iter().map(|v| &v.id).collect::<Vec<_>>())?;
        let mut stmt = self.conn.prepare(
            "SELECT id, title, channel, channel_id, duration, duration_seconds,
             published_at, category_id, tags, fetched_at, unavailable FROM metadata
             WHERE id IN (SELECT value FROM json_each(?1))",
        )?;
        let rows = stmt.query_map([ids], row_to_meta)?;
        rows.map(|row| row.map(|meta| (meta.id.clone(), meta)))
            .collect::<rusqlite::Result<HashMap<_, _>>>()
            .map_err(Into::into)
    }

    /// Total metadata row count.
    pub fn metadata_len(&self) -> Result<usize> {
        let n: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM metadata", [], |r| r.get(0))?;
        usize::try_from(n).context("metadata count is outside the supported range")
    }
}

// ---------------------------------------------------------------
// Row mappers
// ---------------------------------------------------------------

fn duration_to_sql(duration_seconds: u64) -> Result<i64> {
    i64::try_from(duration_seconds).context("video duration is too large to store")
}

fn row_to_meta(row: &rusqlite::Row<'_>) -> rusqlite::Result<VideoMeta> {
    let tags_json: String = row.get(8)?;
    let tags: Vec<String> = serde_json::from_str(&tags_json).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(8, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let duration_seconds: i64 = row.get(5)?;
    let duration_seconds = u64::try_from(duration_seconds).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            5,
            rusqlite::types::Type::Integer,
            Box::new(error),
        )
    })?;
    let unavailable: i64 = row.get(10)?;
    Ok(VideoMeta {
        id: row.get(0)?,
        title: row.get(1)?,
        channel: row.get(2)?,
        channel_id: row.get(3)?,
        duration: row.get(4)?,
        duration_seconds,
        published_at: row.get::<_, DateTime<Utc>>(6)?,
        category_id: row.get(7)?,
        tags,
        fetched_at: row.get::<_, DateTime<Utc>>(9)?,
        unavailable: unavailable != 0,
    })
}

// ---------------------------------------------------------------
// Tests
// ---------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    static NEXT_TEMP_ID: AtomicUsize = AtomicUsize::new(0);

    fn temp_db_dir() -> std::path::PathBuf {
        let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("ytq-db-{}-{id}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn sample_video(id: &str) -> Video {
        Video {
            id: id.to_string(),
            url: format!("https://www.youtube.com/watch?v={id}"),
            added_at: Utc::now(),
        }
    }

    fn sample_meta(id: &str, channel: &str) -> VideoMeta {
        VideoMeta {
            id: id.to_string(),
            title: format!("title-{id}"),
            channel: channel.to_string(),
            channel_id: "UCxxx".to_string(),
            duration: "PT1M".to_string(),
            duration_seconds: 60,
            published_at: Utc::now(),
            category_id: "10".to_string(),
            tags: vec!["a".to_string(), "b".to_string()],
            fetched_at: Utc::now(),
            unavailable: false,
        }
    }

    #[test]
    fn constrained_selection_preserves_nonmatches_and_queue_order() {
        for order in [
            SelectionOrder::Queue,
            SelectionOrder::Stack,
            SelectionOrder::Random,
        ] {
            let db = Db::open_in_memory().unwrap();
            for id in [
                "missing0001",
                "long0000001",
                "match000001",
                "other000001",
                "match000002",
                "zero0000001",
                "gone0000001",
            ] {
                db.add_video(&sample_video(id)).unwrap();
                if id != "missing0001" {
                    let mut meta = sample_meta(id, "Tech Channel");
                    meta.category_id = if id == "other000001" { "10" } else { "28" }.into();
                    meta.duration_seconds = match id {
                        "long0000001" => 1801,
                        "zero0000001" => 0,
                        _ => 1800,
                    };
                    meta.unavailable = id == "gone0000001";
                    db.upsert_metadata_batch(&[meta]).unwrap();
                }
            }
            let selection = Selection {
                category_id: Some("28".into()),
                channel: Some("tech".into()),
                max_duration: Some(1800),
                ..Selection::default()
            };
            assert!(
                db.take_matching(&selection, SelectionOrder::Queue, Some("long0000001"))
                    .unwrap()
                    .is_none()
            );
            let expected = match order {
                SelectionOrder::Queue => Some("match000001"),
                SelectionOrder::Stack => Some("match000002"),
                SelectionOrder::Random => None,
            };
            let removed = db.take_matching(&selection, order, None).unwrap().unwrap();
            if let Some(expected) = expected {
                assert_eq!(removed.video.id, expected);
            }
            assert!(["match000001", "match000002"].contains(&removed.video.id.as_str()));
            let remaining = db
                .take_matching(&selection, SelectionOrder::Random, None)
                .unwrap()
                .unwrap();
            assert_ne!(remaining.video.id, removed.video.id);
            assert!(
                db.take_matching(&selection, SelectionOrder::Queue, None)
                    .unwrap()
                    .is_none()
            );
            assert_eq!(
                db.list_videos()
                    .unwrap()
                    .iter()
                    .map(|v| v.id.as_str())
                    .collect::<Vec<_>>(),
                [
                    "missing0001",
                    "long0000001",
                    "other000001",
                    "zero0000001",
                    "gone0000001"
                ]
            );
        }
    }

    #[test]
    fn search_matches_literal_unicode_text_and_ids_without_metadata() {
        let db = Db::open_in_memory().unwrap();
        db.add_video(&sample_video("abcdefghij1")).unwrap();
        db.add_video(&sample_video("abcdefghij2")).unwrap();
        let mut meta = sample_meta("abcdefghij1", "ÉCOLE Tech");
        meta.title = "Rust: 100%_safe's guide".into();
        db.upsert_metadata_batch(&[meta]).unwrap();
        for query in ["école", "rust", "%_safe's", "abcdefghij2"] {
            let selection = Selection {
                query: Some(query.into()),
                ..Selection::default()
            };
            let matches = db
                .search_videos(&selection, SelectionOrder::Queue, None)
                .unwrap();
            assert_eq!(matches.len(), 1, "{query}");
        }
        assert_eq!(
            db.search_videos(&Selection::default(), SelectionOrder::Stack, Some(1))
                .unwrap()[0]
                .id,
            "abcdefghij2"
        );
        assert_eq!(db.queue_len().unwrap(), 2);
    }

    #[test]
    fn open_file_initializes_schema() {
        let dir = temp_db_dir();
        let path = dir.join("ytq.db");
        let db = Db::open(&path).unwrap();

        assert_eq!(db.queue_len().unwrap(), 0);
        assert_eq!(db.metadata_len().unwrap(), 0);

        drop(db);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn add_video_returns_true_for_new_id() {
        let db = Db::open_in_memory().unwrap();
        let inserted = db.add_video(&sample_video("abc12345678")).unwrap();
        assert!(inserted);
        assert_eq!(db.queue_len().unwrap(), 1);
    }

    #[test]
    fn add_video_returns_false_for_duplicate() {
        let db = Db::open_in_memory().unwrap();
        let v = sample_video("abc12345678");
        assert!(db.add_video(&v).unwrap());
        assert!(!db.add_video(&v).unwrap());
        assert_eq!(db.queue_len().unwrap(), 1);
    }

    #[test]
    fn pop_front_returns_oldest_position() {
        let db = Db::open_in_memory().unwrap();
        for id in ["aaaaaaaaaa1", "bbbbbbbbbb2", "cccccccccc3"] {
            db.add_video(&sample_video(id)).unwrap();
        }
        let v = db.pop_front().unwrap().unwrap();
        assert_eq!(v.id, "aaaaaaaaaa1");
        assert_eq!(db.queue_len().unwrap(), 2);
    }

    #[test]
    fn pop_back_returns_newest_position() {
        let db = Db::open_in_memory().unwrap();
        for id in ["aaaaaaaaaa1", "bbbbbbbbbb2", "cccccccccc3"] {
            db.add_video(&sample_video(id)).unwrap();
        }
        let v = db.pop_back().unwrap().unwrap();
        assert_eq!(v.id, "cccccccccc3");
    }

    #[test]
    fn pop_random_removes_one() {
        let db = Db::open_in_memory().unwrap();
        for id in ["aaaaaaaaaa1", "bbbbbbbbbb2", "cccccccccc3"] {
            db.add_video(&sample_video(id)).unwrap();
        }
        let v = db.pop_random().unwrap().unwrap();
        assert!(["aaaaaaaaaa1", "bbbbbbbbbb2", "cccccccccc3"].contains(&v.id.as_str()));
        assert_eq!(db.queue_len().unwrap(), 2);
    }

    #[test]
    fn pop_front_on_empty_returns_none() {
        let db = Db::open_in_memory().unwrap();
        assert!(db.pop_front().unwrap().is_none());
    }

    #[test]
    fn remove_video_by_id() {
        let db = Db::open_in_memory().unwrap();
        db.add_video(&sample_video("aaaaaaaaaa1")).unwrap();
        db.add_video(&sample_video("bbbbbbbbbb2")).unwrap();
        let removed = db.remove_video("aaaaaaaaaa1").unwrap();
        assert!(removed.is_some());
        assert_eq!(db.queue_len().unwrap(), 1);
        let missing = db.remove_video("zzzzzzzzzz9").unwrap();
        assert!(missing.is_none());
    }

    #[test]
    fn restore_video_preserves_original_order() {
        let db = Db::open_in_memory().unwrap();
        for id in ["aaaaaaaaaa1", "bbbbbbbbbb2", "cccccccccc3"] {
            db.add_video(&sample_video(id)).unwrap();
        }

        let removed = db.take_video("bbbbbbbbbb2").unwrap().unwrap();
        db.add_video(&sample_video("dddddddddd4")).unwrap();
        db.restore_video(&removed).unwrap();

        assert_eq!(
            db.queue_ids().unwrap(),
            vec!["aaaaaaaaaa1", "bbbbbbbbbb2", "cccccccccc3", "dddddddddd4"]
        );
    }

    #[test]
    fn list_videos_in_fifo_order() {
        let db = Db::open_in_memory().unwrap();
        for id in ["aaaaaaaaaa1", "bbbbbbbbbb2", "cccccccccc3"] {
            db.add_video(&sample_video(id)).unwrap();
        }
        let ids: Vec<String> = db
            .list_videos()
            .unwrap()
            .into_iter()
            .map(|v| v.id)
            .collect();
        assert_eq!(ids, vec!["aaaaaaaaaa1", "bbbbbbbbbb2", "cccccccccc3"]);
    }

    #[test]
    fn peek_in_stack_mode_returns_newest_first() {
        let db = Db::open_in_memory().unwrap();
        for id in ["aaaaaaaaaa1", "bbbbbbbbbb2", "cccccccccc3"] {
            db.add_video(&sample_video(id)).unwrap();
        }
        let peeked: Vec<String> = db
            .peek_videos(2, &Mode::Stack)
            .unwrap()
            .into_iter()
            .map(|v| v.id)
            .collect();
        assert_eq!(peeked, vec!["cccccccccc3", "bbbbbbbbbb2"]);
    }

    #[test]
    fn peek_in_queue_mode_returns_oldest_first() {
        let db = Db::open_in_memory().unwrap();
        for id in ["aaaaaaaaaa1", "bbbbbbbbbb2", "cccccccccc3"] {
            db.add_video(&sample_video(id)).unwrap();
        }
        let peeked: Vec<String> = db
            .peek_videos(2, &Mode::Queue)
            .unwrap()
            .into_iter()
            .map(|v| v.id)
            .collect();
        assert_eq!(peeked, vec!["aaaaaaaaaa1", "bbbbbbbbbb2"]);
    }

    #[test]
    fn upsert_metadata_inserts_and_updates() {
        let db = Db::open_in_memory().unwrap();
        let mut m = sample_meta("aaaaaaaaaa1", "Chan A");
        db.upsert_metadata_batch(&[m.clone()]).unwrap();
        assert_eq!(db.metadata_len().unwrap(), 1);

        // Update channel and re-upsert
        m.channel = "Chan B".to_string();
        db.upsert_metadata_batch(&[m.clone()]).unwrap();
        assert_eq!(db.metadata_len().unwrap(), 1);

        let all = db.load_all_metadata().unwrap();
        assert_eq!(all["aaaaaaaaaa1"].channel, "Chan B");
        assert_eq!(all["aaaaaaaaaa1"].tags, vec!["a", "b"]);
    }

    #[test]
    fn upsert_metadata_preserves_unavailable_tombstone() {
        let db = Db::open_in_memory().unwrap();
        let mut m = sample_meta("aaaaaaaaaa1", "X");
        m.unavailable = true;
        db.upsert_metadata_batch(&[m]).unwrap();
        let all = db.load_all_metadata().unwrap();
        assert!(all["aaaaaaaaaa1"].unavailable);
    }

    #[test]
    fn upsert_metadata_rejects_duration_outside_sqlite_range() {
        let db = Db::open_in_memory().unwrap();
        let mut metadata = sample_meta("aaaaaaaaaa1", "X");
        metadata.duration_seconds = u64::MAX;

        let error = db.upsert_metadata_batch(&[metadata]).unwrap_err();
        assert!(error.to_string().contains("duration is too large"));
        assert_eq!(db.metadata_len().unwrap(), 0);
    }

    #[test]
    fn upsert_metadata_only_touches_ids_in_batch() {
        // Locks in the invariant that fetch relies on: upserting a batch that
        // does NOT contain id X must leave X's existing row untouched. The
        // fetch command depends on this to preserve good metadata when the
        // YouTube API fails to return a video on a --force refresh.
        let db = Db::open_in_memory().unwrap();

        let good = sample_meta("aaaaaaaaaa1", "Good Channel");
        let tombstone = {
            let mut m = sample_meta("bbbbbbbbbb2", "");
            m.title.clear();
            m.channel.clear();
            m.unavailable = true;
            m
        };
        db.upsert_metadata_batch(&[good.clone(), tombstone])
            .unwrap();

        // Now upsert a batch that only touches a third id. Neither of the
        // existing rows should change.
        let third = sample_meta("cccccccccc3", "Third Channel");
        db.upsert_metadata_batch(&[third]).unwrap();

        let all = db.load_all_metadata().unwrap();
        assert_eq!(all.len(), 3);
        assert_eq!(all["aaaaaaaaaa1"].channel, "Good Channel");
        assert!(!all["aaaaaaaaaa1"].unavailable);
        assert!(all["bbbbbbbbbb2"].unavailable);
        assert_eq!(all["cccccccccc3"].channel, "Third Channel");
    }

    #[test]
    fn load_all_metadata_roundtrips_tags_array() {
        let db = Db::open_in_memory().unwrap();
        let mut m = sample_meta("aaaaaaaaaa1", "X");
        m.tags = vec!["rust".to_string(), "sqlite".to_string(), "cli".to_string()];
        db.upsert_metadata_batch(&[m]).unwrap();
        let all = db.load_all_metadata().unwrap();
        assert_eq!(all["aaaaaaaaaa1"].tags, vec!["rust", "sqlite", "cli"]);
    }

    #[test]
    fn queue_ids_in_order() {
        let db = Db::open_in_memory().unwrap();
        for id in ["aaaaaaaaaa1", "bbbbbbbbbb2"] {
            db.add_video(&sample_video(id)).unwrap();
        }
        let ids = db.queue_ids().unwrap();
        assert_eq!(ids, vec!["aaaaaaaaaa1", "bbbbbbbbbb2"]);
    }
}

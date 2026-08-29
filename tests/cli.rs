//! End-to-end tests that drive the real `ytq` binary.
//!
//! The unit tests inside `src/` cover parsing, stats, and storage in isolation,
//! but every function in `commands.rs` resolves its own paths and prints its own
//! output, so it can only be exercised through the built binary. `YTQ_CONFIG_DIR`
//! and `YTQ_DATA_DIR` keep each test on a private directory so these never touch
//! the developer's real queue.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_TEMP_ID: AtomicUsize = AtomicUsize::new(0);

/// A private config/data directory pair, removed on drop.
struct TestEnv {
    root: PathBuf,
}

impl TestEnv {
    fn new(label: &str) -> Self {
        let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("ytq-cli-{label}-{}-{id}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("config")).unwrap();
        fs::create_dir_all(root.join("data")).unwrap();
        Self { root }
    }

    fn config_dir(&self) -> PathBuf {
        self.root.join("config")
    }

    fn data_dir(&self) -> PathBuf {
        self.root.join("data")
    }

    fn command(&self) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_ytq"));
        cmd.env("YTQ_CONFIG_DIR", self.config_dir())
            .env("YTQ_DATA_DIR", self.data_dir())
            // Keep assertions free of ANSI escapes.
            .env("NO_COLOR", "1");
        cmd
    }

    fn run(&self, args: &[&str]) -> Output {
        self.command().args(args).output().unwrap()
    }

    /// Runs a command expected to succeed and returns its stdout.
    fn ok(&self, args: &[&str]) -> String {
        let output = self.run(args);
        assert!(
            output.status.success(),
            "`ytq {}` failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    }

    /// Runs a command expected to fail and returns its stderr.
    fn err(&self, args: &[&str]) -> String {
        let output = self.run(args);
        assert!(
            !output.status.success(),
            "`ytq {}` unexpectedly succeeded",
            args.join(" ")
        );
        String::from_utf8(output.stderr).unwrap()
    }
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn video_url(id: &str) -> String {
    format!("https://www.youtube.com/watch?v={id}")
}

#[test]
fn add_then_list_shows_the_video() {
    let env = TestEnv::new("add-list");

    let added = env.ok(&["add", "dQw4w9WgXcQ"]);
    assert!(added.contains("Added:"), "unexpected output: {added}");

    let listed = env.ok(&["list"]);
    assert!(listed.contains("1 videos in queue"), "got: {listed}");
    assert!(listed.contains("dQw4w9WgXcQ"), "got: {listed}");
}

#[test]
fn add_accepts_every_supported_url_shape_as_the_same_video() {
    let env = TestEnv::new("add-shapes");

    env.ok(&["add", "dQw4w9WgXcQ"]);
    for input in [
        video_url("dQw4w9WgXcQ"),
        "https://youtu.be/dQw4w9WgXcQ".to_string(),
        "https://www.youtube.com/shorts/dQw4w9WgXcQ".to_string(),
        "https://m.youtube.com/watch?v=dQw4w9WgXcQ".to_string(),
    ] {
        let output = env.ok(&["add", &input]);
        assert!(
            output.contains("already in queue"),
            "`{input}` should have deduplicated, got: {output}"
        );
    }

    assert!(env.ok(&["list"]).contains("1 videos in queue"));
}

#[test]
fn add_rejects_invalid_input_without_creating_a_queue() {
    let env = TestEnv::new("add-invalid");

    let stderr = env.err(&["add", "https://vimeo.com/12345"]);
    assert!(stderr.contains("error:"), "got: {stderr}");

    assert!(env.ok(&["list"]).contains("Queue is empty"));
}

#[test]
fn remove_deletes_the_video_and_is_friendly_when_empty() {
    let env = TestEnv::new("remove");

    env.ok(&["add", "dQw4w9WgXcQ"]);
    let removed = env.ok(&["remove", "dQw4w9WgXcQ"]);
    assert!(removed.contains("Removed:"), "got: {removed}");
    assert!(env.ok(&["list"]).contains("Queue is empty"));

    // Removing from an empty queue stays a success, so idempotent scripts work.
    assert!(
        env.ok(&["remove", "dQw4w9WgXcQ"])
            .contains("Queue is empty")
    );
}

#[test]
fn remove_reports_a_video_that_is_not_queued() {
    let env = TestEnv::new("remove-missing");

    env.ok(&["add", "dQw4w9WgXcQ"]);
    let stderr = env.err(&["remove", "aaaaaaaaaaa"]);
    assert!(stderr.contains("not found in queue"), "got: {stderr}");
}

#[test]
fn peek_respects_queue_and_stack_mode() {
    let env = TestEnv::new("peek-mode");

    env.ok(&["add", "aaaaaaaaaa1"]);
    env.ok(&["add", "bbbbbbbbbb2"]);
    env.ok(&["add", "cccccccccc3"]);

    // Default queue mode is FIFO.
    let queue_peek = env.ok(&["peek", "2"]);
    let first_line = queue_peek
        .lines()
        .find(|line| line.contains("aaaaaaaaaa1") || line.contains("cccccccccc3"))
        .unwrap();
    assert!(first_line.contains("aaaaaaaaaa1"), "got: {queue_peek}");

    env.ok(&["config", "mode", "stack"]);

    // Stack mode is LIFO.
    let stack_peek = env.ok(&["peek", "2"]);
    let first_line = stack_peek
        .lines()
        .find(|line| line.contains("aaaaaaaaaa1") || line.contains("cccccccccc3"))
        .unwrap();
    assert!(first_line.contains("cccccccccc3"), "got: {stack_peek}");
}

#[test]
fn config_rejects_unknown_keys_and_values() {
    let env = TestEnv::new("config-invalid");

    assert!(
        env.err(&["config", "nope", "1"])
            .contains("unknown config key")
    );
    assert!(
        env.err(&["config", "mode", "sideways"])
            .contains("invalid mode")
    );
    assert!(
        env.err(&["config", "offline", "maybe"])
            .contains("invalid offline value")
    );
    assert!(
        env.err(&["config", "youtube_api_key", "   "])
            .contains("cannot be empty")
    );
}

#[test]
fn config_writes_the_api_key_to_an_owner_only_file() {
    let env = TestEnv::new("config-perms");

    env.ok(&["config", "youtube_api_key", "  secret-key  "]);

    let config_path = env.config_dir().join("config.json");
    let contents = fs::read_to_string(&config_path).unwrap();
    // Surrounding whitespace is trimmed before the key is stored.
    assert!(contents.contains("\"secret-key\""), "got: {contents}");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&config_path).unwrap().permissions().mode();
        assert_eq!(
            mode & 0o777,
            0o600,
            "config.json holds an API key and must not be readable by others"
        );
    }
}

#[test]
fn fetch_refuses_to_run_while_offline() {
    let env = TestEnv::new("fetch-offline");

    let stderr = env.err(&["fetch"]);
    assert!(
        stderr.contains("online features are disabled"),
        "got: {stderr}"
    );
}

#[test]
fn fetch_requires_an_api_key_when_online() {
    let env = TestEnv::new("fetch-no-key");

    env.ok(&["config", "offline", "false"]);
    let output = env
        .command()
        .args(["fetch"])
        .env_remove("YOUTUBE_DATA_API_KEY")
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("no YouTube Data API key"), "got: {stderr}");
}

#[test]
fn info_reports_the_overridden_paths_and_row_counts() {
    let env = TestEnv::new("info");

    env.ok(&["add", "dQw4w9WgXcQ"]);
    let info = env.ok(&["info"]);

    assert!(
        info.contains(&env.data_dir().join("ytq.db").display().to_string()),
        "info should honor YTQ_DATA_DIR, got: {info}"
    );
    assert!(info.contains("Queue Rows:    1"), "got: {info}");
}

#[test]
fn stats_defaults_to_the_current_year_and_counts_added_videos() {
    let env = TestEnv::new("stats");

    env.ok(&["add", "dQw4w9WgXcQ"]);

    let stats = env.ok(&["stats"]);
    assert!(stats.contains("YTQ Stats"), "got: {stats}");
    assert!(stats.contains("Videos Added:          1"), "got: {stats}");

    let all = env.ok(&["stats", "--all"]);
    assert!(all.contains("All Time"), "got: {all}");
}

#[test]
fn stats_separates_readds_and_viewing_sessions_without_inferring_rewatches() {
    let env = TestEnv::new("stats-readded");
    let history_dir = env.data_dir().join("history");
    fs::create_dir_all(&history_dir).unwrap();
    fs::write(
        history_dir.join("2026-01.jsonl"),
        concat!(
            "{\"timestamp\":\"2026-01-01T00:00:00Z\",\"action\":\"Queued\",\"video_id\":\"dQw4w9WgXcQ\",\"time_in_queue_sec\":null}\n",
            "{\"timestamp\":\"2026-01-02T00:00:00Z\",\"action\":\"Watched\",\"video_id\":\"dQw4w9WgXcQ\",\"time_in_queue_sec\":86400}\n",
            "{\"timestamp\":\"2026-01-03T00:00:00Z\",\"action\":\"Queued\",\"video_id\":\"dQw4w9WgXcQ\",\"time_in_queue_sec\":null}\n",
            "{\"timestamp\":\"2026-01-04T00:00:00Z\",\"action\":\"Watched\",\"video_id\":\"dQw4w9WgXcQ\",\"time_in_queue_sec\":86400}\n"
        ),
    )
    .unwrap();

    let stats = env.ok(&["stats", "--all"]);
    assert!(stats.contains("Videos Added:          1"), "got: {stats}");
    assert!(stats.contains("Videos Re-added:       1"), "got: {stats}");
    assert!(stats.contains("Unique Videos Opened:  1"), "got: {stats}");
    assert!(stats.contains("Viewing Sessions:      2"), "got: {stats}");

    let wrapped = env.ok(&["stats", "--wrapped", "--all"]);
    assert!(!wrapped.contains("Comfort Video"), "got: {wrapped}");
}

#[test]
fn stats_rejects_an_inverted_custom_range() {
    let env = TestEnv::new("stats-range");

    let stderr = env.err(&["stats", "--from", "2026-06-01", "--to", "2026-01-01"]);
    assert!(stderr.contains("must not be later than"), "got: {stderr}");
}

#[test]
fn history_survives_a_watched_video_and_feeds_stats() {
    let env = TestEnv::new("history");

    env.ok(&["add", "dQw4w9WgXcQ"]);
    // `next` launches a browser, so drive the removal through `remove`, which
    // writes a Skipped event through the same history path.
    env.ok(&["remove", "dQw4w9WgXcQ"]);

    let history_dir = env.data_dir().join("history");
    let files: Vec<PathBuf> = fs::read_dir(&history_dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "jsonl"))
        .collect();
    assert_eq!(files.len(), 1, "expected one monthly history file");

    let contents = fs::read_to_string(&files[0]).unwrap();
    assert!(contents.contains("\"Queued\""), "got: {contents}");
    assert!(contents.contains("\"Skipped\""), "got: {contents}");

    let stats = env.ok(&["stats"]);
    assert!(stats.contains("Videos Added:          1"), "got: {stats}");
    assert!(stats.contains("Removed Without Open:  1"), "got: {stats}");
}

#[test]
fn list_does_not_panic_when_the_reader_closes_the_pipe() {
    let env = TestEnv::new("broken-pipe");

    // Enough rows that the writer is still going when the reader exits.
    for i in 0..300 {
        env.ok(&["add", &format!("aaaaaaa{i:04}")]);
    }

    let mut producer = env
        .command()
        .arg("list")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    // Drop the read end immediately, which is what `ytq list | head -1` does.
    drop(producer.stdout.take());

    let output = producer.wait_with_output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("panicked"),
        "`ytq list` panicked on a closed pipe: {stderr}"
    );
}

#[test]
fn data_directories_are_created_on_first_use() {
    let env = TestEnv::new("bootstrap");

    // Remove the pre-created directories; ytq should recreate them.
    fs::remove_dir_all(env.data_dir()).unwrap();
    fs::remove_dir_all(env.config_dir()).unwrap();

    env.ok(&["add", "dQw4w9WgXcQ"]);

    assert!(Path::new(&env.data_dir().join("ytq.db")).exists());
    assert!(env.data_dir().join("history").is_dir());
}

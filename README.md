# ytq

**The YouTube Queue for the Terminal.**

**ytq** ("YouTube Queue") is an offline-first CLI for saving YouTube videos to a personal queue, opening the next or a random pick in your browser, and tracking what you watched over time. It supports queue and stack modes, multiple YouTube URL formats, optional metadata fetching via the YouTube Data API, and built-in stats so your backlog stays searchable, lightweight, and out of your tabs.

## Installation

### Prerequisites

You need **Rust 1.85+** installed (for the 2024 edition). If you don't have it, get it here: [rustup.rs](https://rustup.rs/)

### Install from Source

Clone the repo and install the binary to your global path:

```bash
# 1. Clone the repo
git clone https://github.com/jrzimerman/ytq

cd ytq

# 2. Install (Compiles release build & moves to ~/.cargo/bin)
cargo install --path .
```

_Note: Ensure `~/.cargo/bin` is in your system `$PATH`._

## Supported URL Formats

ytq accepts the following YouTube URL formats:

| Format | Example |
|--------|---------|
| Standard watch URL | `youtube.com/watch?v=VIDEO_ID` |
| Short link | `youtu.be/VIDEO_ID` |
| Shorts | `youtube.com/shorts/VIDEO_ID` |
| Live streams | `youtube.com/live/VIDEO_ID` |
| Embed | `youtube.com/embed/VIDEO_ID` |
| Legacy v/ | `youtube.com/v/VIDEO_ID` |
| Legacy e/ | `youtube.com/e/VIDEO_ID` |
| Mobile | `m.youtube.com/watch?v=VIDEO_ID` |
| YouTube Music | `music.youtube.com/watch?v=VIDEO_ID` |
| Direct ID | `VIDEO_ID` (11 characters) |

**Not supported:** Channel URLs, playlist URLs, and search result URLs. These will display a helpful error message suggesting you provide a direct video link instead.

## Quick Start

1. **Stash a video** - Works with full URLs, short links, shorts, live streams, or just the video ID.

```bash
ytq add https://www.youtube.com/watch?v=dQw4w9WgXcQ
ytq add https://www.youtube.com/shorts/dQw4w9WgXcQ
ytq add dQw4w9WgXcQ
```

2. **Watch the next video** - Opens your default browser with the next video in queue.

```bash
ytq next
```

3. **Feeling lucky?** - Pop and watch a random video from the queue.

```bash
ytq random
```

## Command Reference

| Command | Shortcut | Aliases | Description |
|---------|----------|---------|-------------|
| `ytq add <input>` | `a` | | Add video. Accepts URLs or IDs. |
| `ytq next [target]` | `n`, `p`, `w`, `o` | `play`, `watch`, `open` | Watch & pop. Opens browser, logs event, removes from queue. |
| `ytq random` | `r` | `lucky` | Pop and watch a random video from the queue. |
| `ytq peek [n]` | `k` | | Look ahead. Show the next n videos (default: 1). |
| `ytq list` | `l` | `ls` | List all. Shows the full queue. |
| `ytq remove <target>` | `d` | `rm`, `delete` | Delete. Removes item by ID or URL matching. |
| `ytq fetch [target]` | `f` | | Fetch video metadata from YouTube Data API v3. |
| `ytq stats` | `s` | | Metrics. Shows your viewing statistics. Supports `--wrapped`, `--week`, `--month`, `--year`, `--from`, `--to`. |
| `ytq config <key> <value>` | `c` | | Settings. Keys: `mode`, `offline`, `youtube_api_key`. |
| `ytq info` | `i` | | Debug. Prints the exact paths where your data is stored. |

## Configuration

Your preferences live in `config.json`. You can modify them via the CLI.

### Queue Mode

**Switch to "Stack" Mode (LIFO)** - Watch the most recently added video first.

```bash
ytq config mode stack
```

**Switch back to "Queue" Mode (FIFO)**

```bash
ytq config mode queue
```

### Online Features (Optional)

ytq is **offline by default** - no network requests are made unless you explicitly enable online features.

**Enable online features:**

```bash
ytq config offline false
```

**Set your YouTube Data API v3 key:**

```bash
ytq config youtube_api_key YOUR_KEY_HERE
```

Or use an environment variable (takes precedence over config):

```bash
export YOUTUBE_DATA_API_KEY=YOUR_KEY_HERE
```

### Fetching Metadata

When online features are enabled, the `fetch` command retrieves video metadata (title, channel, duration, tags, etc.) from the YouTube Data API v3.

```bash
# Fetch metadata for all queue videos missing metadata
ytq fetch

# Fetch with a limit (useful for testing)
ytq fetch --limit 5

# Fetch for a specific video (force-refresh)
ytq fetch dQw4w9WgXcQ

# Fetch for multiple videos (comma-separated, force-refresh)
ytq fetch dQw4w9WgXcQ,jNQXAC9IVRw

# Fetch for all videos (queue + history)
ytq fetch --all

# Fetch for history videos only
ytq fetch --history

# Force refresh video categories
ytq fetch --refresh-categories
```

Metadata is stored in the `metadata` table inside `ytq.db`, keeping reads indexed and writes fast. Video categories are cached in `categories.json` and only fetched on first run (or with `--refresh-categories`).

When metadata is available, `list` and `peek` show enriched output with video titles, channels, and durations:

```
4 videos in queue:
  #    ID            Title                                Channel         Duration  Added
  1    dQw4w9WgXcQ   Never Gonna Give You Up (Officia...  Rick Astley     3:34      2026-02-14 10:30
  2    jNQXAC9IVRw   Me at the zoo                        jawed           0:19      2026-02-13 09:15
  3    abc12345678   (run `ytq fetch`)                                              2026-02-12 08:00
  4    def12345678   (run `ytq fetch`)                                              2026-02-11 07:00
```

### Statistics

ytq tracks your queue behavior and viewing patterns. The `stats` command shows a summary of your activity:

```bash
# All-time overview
ytq stats

# Full "wrapped" deep dive with charts and leaderboards
ytq stats --wrapped
```

**Time filtering** lets you scope stats to any period:

```bash
ytq stats --week                          # Last 7 days
ytq stats --month                         # Last 30 days
ytq stats --month 2026-01                 # Specific month
ytq stats --year                          # Last 365 days
ytq stats --year 2025                     # Specific year
ytq stats --from 2025-06-01 --to 2025-12-31  # Custom range
ytq stats --wrapped --year 2025           # Combine with --wrapped
```

**Basic stats** (always available from the event log):
- Videos added, watched, skipped counts
- Completion rate and queue depth
- Average time in queue before watching
- Most active day of week

**Wrapped stats** (`--wrapped` flag adds):
- Monthly activity bar charts (added and watched)
- Time-of-day distribution (morning/afternoon/evening/night)
- Busiest day and longest watch streak
- Top channels and category breakdown with bar charts
- Top tags, skip rate, queue throughput
- Longest/shortest videos, fastest/slowest time-to-watch

When metadata is available (via `ytq fetch --history`), stats are enriched with total watch time, channel rankings, categories, tags, and video durations. Without metadata, core event-log stats still work — no network requests are ever made by `stats`.

## Data Storage

ytq uses platform-specific paths for data storage. Run `ytq info` to see where your data lives.

| File | Purpose |
|------|---------|
| `config.json` | User configuration (mode, offline, API key) |
| `ytq.db` | SQLite database holding the queue and metadata tables |
| `categories.json` | YouTube video category lookup table |
| `history/*.jsonl` | Event history logs (partitioned by month) |

Earlier versions of ytq stored the queue and metadata as `queue.json` and `metadata.json`. On first run after upgrading, those files are imported into `ytq.db` automatically — see [SQLite Migration](#sqlite-migration) below.

## SQLite Migration

ytq originally stored the queue and metadata as JSON files. As personal backlogs grew past several thousand videos, every `ytq add` had to rewrite the full `queue.json`, making batch tools (for example [`youtube-tab-manager`](https://github.com/jrzimerman/youtube-tab-manager)) painfully slow. Storage now lives in a single SQLite database at `ytq.db` with indexed primary keys, write-ahead logging, and O(1) inserts.

### When the migration runs

The migration is automatic and one-time. It runs the first time any `ytq` command opens the database after upgrading, provided both of the following are true:

- `ytq.db` is missing or has empty `queue` and `metadata` tables.
- A legacy `queue.json` and/or `metadata.json` exists in the data directory.

On success you'll see a one-line summary on stderr, for example:

```
Migrated 13472 video(s) and 13552 metadata entrie(s) to SQLite.
```

After the import:

- `queue.json` is renamed to `queue.json.bak`.
- `metadata.json` is renamed to `metadata.json.bak`.
- The legacy `queue.json.lock` (used by the previous file-locking layer) is removed.
- All future writes go to `ytq.db`. No code path ever reads the `.bak` files again.

If the import fails midway, the transaction rolls back and the `.bak` files are not created — you can re-run any `ytq` command to retry once the underlying problem is fixed.

### How to verify the migration

Run `ytq info` after upgrading. Pre-migration, the output ends with `Database File Exists? false`. Post-migration, it prints row counts:

```
Data Paths
---------------
Config:     /Users/you/.config/ytq/config.json
Database:   /Users/you/.local/share/ytq/ytq.db
Categories: /Users/you/.local/share/ytq/categories.json
History:    /Users/you/.local/share/ytq/history
Queue Rows:    13472
Metadata Rows: 13552
```

Cross-check by comparing against the legacy files:

```bash
# Count videos in the legacy queue.json.bak vs the migrated db
jq 'length' ~/.local/share/ytq/queue.json.bak
ytq info | grep "Queue Rows"

# Count metadata entries
jq 'length' ~/.local/share/ytq/metadata.json.bak
ytq info | grep "Metadata Rows"

# Spot-check ordering
ytq list | head -5
```

The row counts should match exactly.

### How to test the migration safely

If you want to dry-run the migration without touching your real data:

```bash
# 1. Snapshot your current data dir
cp -a ~/.local/share/ytq ~/.local/share/ytq.pre-migration-backup

# 2. Run any ytq command (info is the safest — it does nothing else)
ytq info

# 3. Verify the migration summary printed to stderr and row counts look right
ytq info

# 4. If anything looks off, roll back by restoring the snapshot:
rm -rf ~/.local/share/ytq
mv ~/.local/share/ytq.pre-migration-backup ~/.local/share/ytq
```

On Windows, substitute `%APPDATA%\ytq` for `~/.local/share/ytq`.

### How to clean up after the migration

Once you've confirmed the migration worked and your queue looks right, the `.bak` files are safe to delete. They are never read again by ytq.

```bash
# Remove the legacy JSON files
rm ~/.local/share/ytq/queue.json.bak
rm ~/.local/share/ytq/metadata.json.bak

# If you took a pre-migration snapshot during testing, remove that too
rm -rf ~/.local/share/ytq.pre-migration-backup
```

There's no rush — the `.bak` files are kept indefinitely so you can hold on to them as long as you like. They sit alongside `ytq.db` but don't grow or interfere with operations.

## Development

Want to hack on `ytq`?

```bash
# Fast compile check
cargo check

# Build
cargo build
cargo build --release

# Format code
cargo fmt
cargo fmt --check

# Lint
cargo clippy -- -W clippy::all

# Match CI locally
cargo clippy -- -D warnings

# Run the test suite
cargo test

# List all tests
cargo test -- --list

# Run a single test by name fragment
cargo test valid_video_id_direct
cargo test basic_stats_counts

# Run tests in one module
cargo test youtube::tests
cargo test stats::tests

# Show test stdout
cargo test valid_video_id_direct -- --nocapture

# Run locally without installing
cargo run -- list
cargo run -- add https://www.youtube.com/watch?v=dQw4w9WgXcQ
```

CI currently runs:

```bash
cargo test
cargo fmt --check
cargo clippy -- -D warnings
```

Before opening a PR, run:

```bash
cargo fmt
cargo clippy -- -D warnings
cargo test
```

## Uninstallation

To remove `ytq` and all associated data, follow these steps. Windows users may need to adjust paths.

1. Remove the binary:

```bash
cargo uninstall ytq
```

2. Clear your data and history (run `ytq info` to confirm these paths first):

```bash
rm -rf ~/.local/share/ytq
rm -rf ~/.config/ytq
```

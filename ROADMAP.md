# Roadmap

ytq is a fully functional offline-first CLI for managing a YouTube watch queue. This document outlines planned features and enhancements.

## Current Features

- [x] Add videos via URL (watch, shorts, live, embed, v/, e/), short link (youtu.be), or video ID
- [x] Supports mobile URLs (m.youtube.com) and YouTube Music URLs (music.youtube.com)
- [x] Queue (FIFO) and Stack (LIFO) modes
- [x] Watch next video (opens browser) with optional target selection
- [x] Random video selection (`ytq random` / `ytq lucky`)
- [x] List, peek, and remove videos
- [x] Event history logging (partitioned by month as JSONL)
- [x] Enhanced statistics with time filtering and "wrapped" deep dive
- [x] Basic statistics (added, watched, skipped counts)
- [x] Explicit error messages for unsupported URLs (channels, playlists, search)
- [x] SQLite-backed storage for queue and metadata (WAL mode handles concurrency)
- [x] Platform-specific paths (XDG on Linux/macOS, AppData on Windows)
- [x] Single-letter aliases for all commands (`a`, `n`, `p`, `w`, `o`, `l`, `k`, `d`, `f`, `s`, `c`, `i`, `r`)

---

## Implemented: Optional YouTube Metadata Fetching

Fetch video metadata (title, channel, duration, tags) via the YouTube Data API v3 for enhanced display and future analytics.

### Architecture: Sidecar Enrichment Pattern

Metadata is stored separately from queue and history data to keep core operations instant:

| File | Purpose | Format |
|------|---------|--------|
| `ytq.db` | Video queue and metadata cache | SQLite |
| `categories.json` | YouTube video category lookup table | JSON object (ID -> name) |
| `history/*.jsonl` | Event history logs | Append-only JSONL |

- `add`, `remove`, `next` remain instant (no network I/O)
- `fetch` is the only command that makes network requests
- `list` and `peek` join queue data with metadata at display time (local, fast)
- Queue and metadata writes use indexed SQLite operations
- Video categories are stored separately for stats/wrapped analytics

### Design Principles

1. **Offline by default** — The `offline` config defaults to `true`. No network requests are made unless explicitly enabled.
2. **`add` is always instant** — The `add` command never makes network requests. Metadata is fetched separately via `fetch`.
3. **Graceful degradation** — If `offline: false` but no API key is configured, `fetch` shows a clear error with setup instructions.
4. **Opt-in messaging** — Only show "run `ytq fetch` for metadata" hints when `offline: false`, so offline-first users aren't nagged.
5. **Decoupled metadata** — Video metadata lives in its own SQLite table, not embedded in queue or history. This keeps core data structures unchanged and enables independent refresh/update cycles.

### Configuration Behavior

| `offline` | API Key Set | Behavior                                                    |
|-----------|-------------|-------------------------------------------------------------|
| `true`    | —           | No network requests. No metadata hints. Default experience. |
| `false`   | No          | `fetch` command warns about missing API key.                |
| `false`   | Yes         | `fetch` command enabled. Hints shown after `add`.           |

API key can be configured via `ytq config youtube_api_key <key>` or the `YOUTUBE_DATA_API_KEY` environment variable. Environment variable takes precedence.

### Implementation Phases

- [x] **Phase 1: Configuration**
  - [x] `offline` config option (default: `true`)
  - [x] `youtube_api_key` config option
  - [x] `YOUTUBE_DATA_API_KEY` environment variable support (takes precedence)

- [x] **Phase 2: Models & Storage**
  - [x] `VideoMeta` struct: id, title, channel, channel_id, duration_seconds, published_at, category_id, tags, fetched_at
  - [x] SQLite metadata table keyed by video ID
  - [x] `categories.json` — separate lookup table for YouTube video categories
  - [x] `Video` and `Event` structs unchanged — metadata fully decoupled

- [x] **Phase 3: Fetch Command**
  - [x] `ytq fetch` — fetch metadata for queue videos missing metadata
  - [x] `ytq fetch <id>` or `ytq fetch <id1>,<id2>` — fetch/refresh specific videos (force-refresh, bypasses diff)
  - [x] Scope flags: `--queue` (default), `--history`, `--all`
  - [x] `--limit N` flag for testing and quota management
  - [x] `--refresh-categories` flag to force category refresh
  - [x] Categories auto-fetched on first run, cached thereafter
  - [x] Progress indicator ("Fetching 1-50 of N...")
  - [x] Metadata deduplication via transactional SQLite upserts
  - [ ] Respect YouTube API rate limits with exponential backoff

- [x] **Phase 4: Enhanced Display**
  - [x] `list` shows tabular output with ID always visible
  - [x] Online mode: ID, title, channel, duration, added date
  - [x] Offline mode: ID, added date
  - [x] `peek` shows enriched output when metadata available
  - [x] Graceful fallback: "(run `ytq fetch`)" hint in title column when metadata missing

---

## Implemented: Enhanced Statistics ("YouTube Wrapped")

The `stats` command supports time-based filtering and a `--wrapped` flag for a full deep-dive analysis. Analytics focus on **your interaction and usage patterns**, not YouTube's popularity metrics. No new dependencies were added — all analytics are pure Rust using existing crates.

### Design Principles

1. **Offline-first** — Core stats (counts, streaks, queue time, trends) always work from the event log alone. No network requests are ever made by `stats`.
2. **Metadata enrichment** - When metadata is available, stats are enriched with total video duration, channel rankings, categories, tags, and video durations. Opening a video does not imply completion.
3. **Graceful hints** — When metadata would improve results, a hint is shown: "Run `ytq fetch --history` for richer stats."

### Basic Stats (`ytq stats`)

Always available from the event log:

- [x] First-time additions and re-additions
- [x] Unique videos opened and total viewing sessions
- [x] Queue exits opened versus removed without opening
- [x] Current queue depth
- [x] Average time in queue before the first open
- [x] Most active day of week for adding videos
- [x] Total duration of uniquely opened videos (when metadata is available)
- [x] Top 3 channels (when metadata available)

### Wrapped Stats (`ytq stats --wrapped`)

All basic stats plus:

- [x] Monthly activity bar charts (first added, re-added, and viewing sessions)
- [x] Viewing-session time-of-day distribution (morning/afternoon/evening/night)
- [x] Busiest single day
- [x] Longest viewing-session streak (consecutive days)
- [x] Top 10 channels with bar chart
- [x] Category breakdown with bar chart (joined against `categories.json`)
- [x] Top 10 tags (normalized, case-insensitive)
- [x] Average video duration
- [x] Longest and shortest uniquely opened videos
- [x] Queue exits opened versus removed without opening
- [x] Fastest and slowest time-to-first-open
- [x] Viewing sessions per week

### Time Filtering

- [x] `ytq stats` — Current-year statistics (default)
- [x] `ytq stats --all` — All-time statistics
- [x] `ytq stats --week` — Last 7 days
- [x] `ytq stats --month` — Last 30 days
- [x] `ytq stats --month 2026-01` — Specific month
- [x] `ytq stats --year` — Last 365 days
- [x] `ytq stats --year 2025` — Specific year
- [x] `ytq stats --from 2025-06-01 --to 2025-12-31` — Custom date range
- [x] All period flags composable with `--wrapped`
- [x] Conflicting period flags rejected with clear errors

### Real-Usage Review and Next Wrapped Iteration

The 2026-08-29 review of the complete history found 23,499 first additions,
148 re-additions, 236 uniquely opened videos, 258 viewing sessions, and a
23,356-video queue. See [`STATS.md`](STATS.md) for the complete semantics and
observations.

Completed as part of the review:

- [x] Separate first-time additions from re-additions
- [x] Separate unique first opens from viewing sessions
- [x] Stop describing queue exits as completions
- [x] Remove the Comfort Video metric because repeated opens do not establish
  rewatch intent
- [x] Use full history before applying date-range first/repeated classification
- [x] Rename metadata-derived watch time to total video duration
- [x] Add end-to-end coverage for pop, re-add, and later-open history

Prioritized future Wrapped insights:

- [ ] Queue age profile with median, 90th percentile, and age buckets
- [ ] First-open funnel for first additions versus unique first opens
- [ ] Channel and category open rates with minimum sample thresholds
- [ ] Duration preference lift comparing added and uniquely opened videos
- [ ] First-open latency percentiles and same-day/week/month buckets
- [ ] Active viewing days and sessions per active day
- [ ] Re-add follow-through without inferring continuation or rewatch intent
- [ ] Monthly backlog trajectory and clearly labeled queue-runway projection
- [ ] Content freshness based on publication-year distributions
- [ ] Multi-signal personality summaries that can show mixed patterns instead
  of allowing one time-of-day bucket to dominate the result
- [ ] Record addition source so imports and bulk workflows can be separated from
  deliberate individual additions in trend reports
- [ ] Explore explicit completion or resume actions before adding any completion
  or rewatch analytics

---

## Implemented: SQLite Storage Backend

Queue and metadata storage migrated from JSON files to a single SQLite database to fix slow writes at scale. Driven by the 13,000+ video queue that made every `ytq add` rewrite a 1.7 MB JSON file, making batch tools like `youtube-tab-manager` painfully slow.

### Architecture

| File | Purpose |
|------|---------|
| `ytq.db` | SQLite database holding queue + metadata tables |
| `categories.json` | Tiny, rarely written category cache |
| `config.json` | User configuration |
| `history/*.jsonl` | Append-only event logs |

### Design Principles

1. **Hot-path writes are O(1)** — `add`, `next`, `remove`, `random` issue a single indexed SQL statement instead of rewriting the full queue file.
2. **No async runtime** — Uses `rusqlite` (synchronous), matching ytq's existing synchronous design. No tokio dependency.
3. **Bundled SQLite** — `rusqlite` is built with `features = ["bundled", "chrono"]` so users have zero runtime dependencies and `DateTime<Utc>` round-trips natively.
4. **Event log stays as JSONL** — Append-only history doesn't benefit from a database; partitioned monthly files remain.

### Implementation Phases

- [x] **Phase 1: Database module**
  - [x] Add `rusqlite` with bundled SQLite and chrono support
  - [x] Remove `fd-lock` dependency
  - [x] New `src/db.rs` with `Db` struct, schema init, PRAGMAs (WAL, foreign_keys)
  - [x] In-memory test harness via `Connection::open_in_memory()`

- [x] **Phase 2: Schema**
  - [x] `queue` table: id PK, url, added_at, position (monotonic for FIFO/LIFO)
  - [x] `metadata` table: id PK, full VideoMeta columns, tags as JSON TEXT with `CHECK (json_valid(tags))`
  - [x] Indexes on `queue.position`, `metadata.channel`, `metadata.category_id`

- [x] **Phase 3: Command integration**
  - [x] `add` -> `INSERT OR IGNORE` (dedup via PRIMARY KEY)
  - [x] `next` (default) -> `DELETE ... WHERE position = MIN/MAX(position) RETURNING *`
  - [x] `next <target>` / `remove` -> `DELETE WHERE id = ? RETURNING *`
  - [x] `random` -> `ORDER BY RANDOM() LIMIT 1` then delete
  - [x] `list` / `peek` -> indexed reads, optional join with metadata
  - [x] `fetch` upserts -> single transaction with `INSERT ... ON CONFLICT`
  - [x] `stats` -> materialize metadata HashMap from db (stats.rs unchanged)
  - [x] `info` -> show db path + row counts

- [x] **Phase 4: Tests**
  - [x] Dedup, FIFO/LIFO ordering, random pop, remove
  - [x] Metadata upsert preserves tombstones
  - [x] Tags array round-trip through JSON TEXT column

---

## Future Considerations

Ideas that may be explored later:

- Fuzzy search within queue (by ID, or title/channel when metadata available)
- Paginated list output — Show first 100 videos by default, with `--limit N` and `--all` flags
- Exponential backoff for YouTube API rate limits
- Additional metadata sources that don't require an API key
- Rewrite `stats` aggregations as SQL `GROUP BY` queries (currently materialize a HashMap from SQLite then aggregate in Rust)
- `ytq export` command to dump the SQLite database back to JSON for portability
- `ytq vacuum` wrapper around SQLite `VACUUM` for users who heavily churn the queue
- Migrate event history from JSONL to SQLite if append-only log scans ever become a bottleneck (currently bounded by `stats` invocations only)

---

## Contributing

Contributions are welcome! If you'd like to work on any roadmap item, please open an issue first to discuss the approach.

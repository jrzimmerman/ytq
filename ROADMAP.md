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
- [x] Basic statistics (added, watched, skipped counts)
- [x] Explicit error messages for unsupported URLs (channels, playlists, search)
- [x] File locking for concurrent access protection (fd-lock)
- [x] Platform-specific paths (XDG on Linux/macOS, AppData on Windows)
- [x] Single-letter aliases for all commands (`a`, `n`, `p`, `w`, `o`, `l`, `k`, `d`, `f`, `s`, `c`, `i`, `r`)

---

## Implemented: Optional YouTube Metadata Fetching

Fetch video metadata (title, channel, duration, tags) via the YouTube Data API v3 for enhanced display and future analytics.

### Architecture: Sidecar Enrichment Pattern

Metadata is stored separately from queue and history data to keep core operations instant:

| File | Purpose | Format |
|------|---------|--------|
| `queue.json` | Video queue (ID, URL, added_at) | JSON array |
| `metadata.json` | Video metadata cache (title, channel, duration, tags, etc.) | JSON object keyed by ID |
| `categories.json` | YouTube video category lookup table | JSON object (ID -> name) |
| `history/*.jsonl` | Event history logs | Append-only JSONL |

- `add`, `remove`, `next` remain instant (no network I/O)
- `fetch` is the only command that makes network requests
- `list` and `peek` join queue data with metadata at display time (local, fast)
- Video categories are stored separately for future stats/wrapped analytics

### Design Principles

1. **Offline by default** — The `offline` config defaults to `true`. No network requests are made unless explicitly enabled.
2. **`add` is always instant** — The `add` command never makes network requests. Metadata is fetched separately via `fetch`.
3. **Graceful degradation** — If `offline: false` but no API key is configured, `fetch` shows a clear error with setup instructions.
4. **Opt-in messaging** — Only show "run `ytq fetch` for metadata" hints when `offline: false`, so offline-first users aren't nagged.
5. **Decoupled metadata** — Video metadata lives in `metadata.json`, not embedded in queue or history. This keeps core data structures unchanged and enables independent refresh/update cycles.

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
  - [x] `metadata.json` sidecar file — JSON object keyed by video ID, read-modify-write with atomic temp-file-then-rename
  - [x] `categories.json` — separate lookup table for YouTube video categories
  - [x] `Video` and `Event` structs unchanged — metadata fully decoupled
  - [x] No migration required — existing data continues to work

- [x] **Phase 3: Fetch Command**
  - [x] `ytq fetch` — fetch metadata for queue videos missing metadata
  - [x] `ytq fetch <id>` or `ytq fetch <id1>,<id2>` — fetch/refresh specific videos (force-refresh, bypasses diff)
  - [x] Scope flags: `--queue` (default), `--history`, `--all`
  - [x] `--limit N` flag for testing and quota management
  - [x] `--refresh-categories` flag to force category refresh
  - [x] Categories auto-fetched on first run, cached thereafter
  - [x] Progress indicator ("Fetching 1-50 of N...")
  - [x] Metadata deduplication via read-modify-write (upsert into HashMap, write full file)
  - [ ] Respect YouTube API rate limits with exponential backoff

- [x] **Phase 4: Enhanced Display**
  - [x] `list` shows tabular output with ID always visible
  - [x] Online mode: ID, title, channel, duration, added date
  - [x] Offline mode: ID, added date
  - [x] `peek` shows enriched output when metadata available
  - [x] Graceful fallback: "(run `ytq fetch`)" hint in title column when metadata missing

---

## Planned Features

### Enhanced Statistics ("YouTube Wrapped")

Expand the `stats` command with time-based filtering and richer metrics when metadata is available. Analytics focus on **your interaction and usage patterns**, not YouTube's popularity metrics.

#### Basic Stats (Always Available)

These stats work regardless of metadata availability:

- Videos added / watched / skipped counts
- Average time in queue before watching
- Queue throughput (watched vs added over time)
- Most active days/times for adding videos
- Completion rate (watched / total removed)

#### Enhanced Stats (When Metadata Available)

When videos have metadata, additional statistics become available:

- Total watch time (sum of durations)
- Average video length
- Favorite channels (by watch count)
- Longest/shortest videos watched
- Category breakdown (joined against `categories.json`)
- Tag analysis

#### Time Filtering

- [ ] `ytq stats` — All-time statistics (default)
- [ ] `ytq stats --week` — Last 7 days
- [ ] `ytq stats --month` — Last 30 days
- [ ] `ytq stats --month 2026-01` — Specific month
- [ ] `ytq stats --year` — Last 365 days
- [ ] `ytq stats --year 2025` — Specific year
- [ ] `ytq stats --from 2025-06-01 --to 2025-12-31` — Custom date range

---

## Future Considerations

Ideas that may be explored later:

- Fuzzy search within queue (by ID, or title/channel when metadata available)
- Paginated list output — Show first 100 videos by default, with `--limit N` and `--all` flags
- Exponential backoff for YouTube API rate limits
- Additional metadata sources that don't require an API key

---

## Contributing

Contributions are welcome! If you'd like to work on any roadmap item, please open an issue first to discuss the approach.

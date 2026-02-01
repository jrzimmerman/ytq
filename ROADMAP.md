# Roadmap

ytq is a fully functional offline-first CLI for managing a YouTube watch queue. This document outlines planned features and enhancements.

## Current Features

- [x] Add videos via URL (watch, shorts, live, embed, v/), short link (youtu.be), or video ID
- [x] Queue (FIFO) and Stack (LIFO) modes
- [x] Watch next video (opens browser)
- [x] List, peek, and remove videos
- [x] Event history logging
- [x] Basic statistics (added, watched, skipped counts)
- [x] Explicit error messages for unsupported URLs (channels, playlists, search)

---

## Planned Features

### Optional YouTube Metadata Fetching

Fetch video metadata (title, channel, duration, tags) via the YouTube Data API v3 for enhanced statistics and display.

#### Design Principles

1. **Offline by default** — The `offline` config defaults to `true`. No network requests are made unless explicitly enabled.
2. **`add` is always instant** — The `add` command never makes network requests. Metadata is fetched separately via a new `fetch` command.
3. **Graceful degradation** — If `offline: false` but no API key is configured, show a warning and continue operating in offline mode.
4. **Opt-in messaging** — Only show "run `ytq fetch` for metadata" hints when `offline: false`, so offline-first users aren't nagged.
5. **Forwards compatible data** — All new fields use `Option<T>` with `#[serde(default)]` so old queue/history files parse correctly without migration.

#### Configuration Behavior

| `offline` | API Key Set | Behavior                                                    |
|-----------|-------------|-------------------------------------------------------------|
| `true`    | —           | No network requests. No metadata hints. Default experience. |
| `false`   | No          | `fetch` command warns about missing API key.                |
| `false`   | Yes         | `fetch` command enabled. Hints shown after `add`.           |

API key can be configured via `ytq config youtube_api_key <key>` or the `YOUTUBE_API_KEY` environment variable. Environment variable takes precedence.

#### Implementation Phases

- [ ] **Phase 1: Configuration**
  - [ ] Re-add `offline` config option (default: `true`)
  - [ ] Add `youtube_api_key` config option
  - [ ] Support `YOUTUBE_API_KEY` environment variable (takes precedence)

- [ ] **Phase 2: Models**
  - [ ] Add `VideoMeta` struct (title, channel, duration_seconds, tags, thumbnail_url)
  - [ ] Add `meta: Option<VideoMeta>` field to `Video` struct
  - [ ] Add metadata fields to `Event` struct for historical stats
  - [ ] Use `#[serde(default)]` on all new `Option<T>` fields for forwards compatibility
  - [ ] Ensure stats/display code checks for `Some(meta)` before using metadata
  - [ ] No migration required — existing data continues to work; metadata populates gradually via `fetch`

- [ ] **Phase 3: Fetch Command**
  - [ ] New `ytq fetch` command to retrieve metadata from YouTube Data API v3
  - [ ] Batch mode (default): fetch all videos missing metadata
  - [ ] Single video mode: `ytq fetch <video_id>`
  - [ ] Scope flags: `--queue` (default), `--history`, `--all`
  - [ ] Progress indicator ("Fetching 5/23...")
  - [ ] Respect YouTube API rate limits with exponential backoff
  - [ ] Cache results to avoid redundant API calls

- [ ] **Phase 4: Enhanced Display**
  - [ ] Show video titles in `list` and `peek` commands (when available)
  - [ ] Show channel name and duration in video details
  - [ ] Graceful fallback to video ID when metadata unavailable

---

### Enhanced Statistics

Expand the `stats` command with time-based filtering and richer metrics when metadata is available.

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
- Tag/category breakdown

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

- `ytq random` (alias: `lucky`) — Pop and watch a random video from the queue
- Fuzzy search within queue (by ID, or title/channel when metadata available)
- Paginated list output — Show first 100 videos by default, with `--limit N` and `--all` flags

---

## Contributing

Contributions are welcome! If you'd like to work on any roadmap item, please open an issue first to discuss the approach.

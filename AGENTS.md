# ytq

Guidance for coding agents working in this repository.

## Project Summary

- `ytq` is a Rust CLI for managing a personal YouTube queue.
- Minimum supported Rust version (MSRV): `1.95`, declared as `rust-version` in `Cargo.toml`
  and verified by the `msrv` CI job. It is the real floor, not the latest release: `1.94`
  fails because `libsqlite3-sys` (pulled in by `rusqlite`'s `bundled` feature) uses
  `cfg_select!`. Raise it only when something actually requires it, and update the
  pinned toolchain in `.github/workflows/ci.yml` in the same change.
- The app is offline-first. Network access is only used for explicit metadata/category
  fetch operations.

## Before Considering Work Complete

Run the same gates CI runs:

```bash
cargo fmt
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
```

## Testing Notes

- Unit tests live in `#[cfg(test)] mod tests` blocks at the bottom of each file.
- End-to-end tests live in `tests/cli.rs` and run the real binary via `CARGO_BIN_EXE_ytq`.
  Everything in `commands.rs` resolves its own paths and prints its own output, so that
  is the only place it can be covered. Add coverage there when changing command behavior.
- `tests/cli.rs` uses `TestEnv`, which points `YTQ_CONFIG_DIR`/`YTQ_DATA_DIR` at a private
  temp directory and removes it on drop. Never write a test that uses the default paths.
- Cover both success and failure paths when editing parsing, stats, or persistence logic.

## Code Style Overview

Follow existing patterns in the repo rather than introducing a new style.
Always use `cargo fmt`; do not manually preserve line wrapping that `rustfmt`
wants to change.

### Imports

Group imports in this order, separated by blank lines:

1. Standard library
2. Crate-local imports (`crate::...`)
3. External crates

Example:

```rust
use std::sync::LazyLock;

use crate::models::{Config, Event, Video};
use crate::{paths, store};

use anyhow::{bail, Result};
use regex::Regex;
```

Let `cargo fmt` handle intra-group ordering.

### Types and Data Modeling

- Use `#[serde(default)]` for backward-compatible config/model evolution.
- Use `#[serde(rename_all = "lowercase")]` for enums exposed in JSON config.
- Keep serialized shapes stable; this app persists user data locally.

### Error Handling

- Use `anyhow::Result<T>` for fallible functions, `bail!(...)` for early
  user-facing failures, and `?` to propagate.
- Add context with `.context(...)` or `.with_context(...)` around I/O and parsing
  that can fail opaquely.
- Start messages lowercase and without trailing punctuation. They are rendered as
  `error: {message}` and chained with `{:#}`, so a capitalized message reads wrong
  mid-chain. The exception is a leading proper noun, as in "YouTube API returned HTTP 500".

## Project-Specific Implementation Patterns

### CLI and Command Flow

- CLI definitions use `clap` derive macros in `src/main.rs`.
- Help text is usually written as doc comments on enum variants and fields.
- Aliases and visible aliases are common; preserve existing command ergonomics.
- `main()` prints colored errors and exits non-zero; command logic lives in `run()`
  and `src/commands.rs`.

### Printing

- Use `outln!` (from `src/output.rs`), never `println!`, for anything written to stdout.
  `println!` panics with "failed printing to stdout" when the reader closes the pipe,
  which happens for something as ordinary as `ytq list | head`. `outln!` exits quietly instead.
- Keep stdout for data the user asked for and `eprintln!` for warnings and progress,
  so `ytq list > file` stays clean and pipelines keep working.

### Persistence

- Queue and metadata mutations go through the `Db` struct in `src/db.rs`.
- Open one `Db` per command via `Db::open(&paths.db_file)`.
- Concurrency is handled by SQLite WAL mode (no file locking layer).
- Config and categories are still JSON; history is append-only monthly JSONL.
- Persistence helpers (for non-DB files) return defaults instead of failing on missing files.
- JSON files are written through `store::write_atomic` (temp file + rename), never `fs::write`.
  `config.json` is written owner-only because it can hold an API key.
- `YTQ_CONFIG_DIR` and `YTQ_DATA_DIR` override the platform paths. Keep them working;
  the end-to-end tests depend on them to avoid touching the developer's real queue.

### Parsing and Validation

- URL/ID parsing lives in `src/youtube.rs`. Keep validation messages specific and
  user-friendly.
- Preserve support for multiple YouTube URL formats and explicit rejection of
  unsupported ones.

### Platform-Specific Code

- `src/paths.rs` uses `#[cfg(target_os = "windows")]` and `#[cfg(not(target_os = "windows"))]`.
- Continue using `etcetera` strategy selection rather than hand-rolled platform path logic.

### Stats and Time Handling

Read `STATS.md` before changing stats behavior. It defines the reporting
semantics and records findings from the real history.

## Practical Advice For Agents

- Preserve backward compatibility for local data files where practical.
- When changing command behavior, check `README.md` and CLI help text for drift.
- When changing developer workflow expectations, keep this file aligned with
  `.github/workflows/ci.yml`.

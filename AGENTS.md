# AGENTS.md - ytq

Guidance for coding agents working in this repository.

## Project Summary

- `ytq` is a Rust CLI for managing a personal YouTube queue.
- Rust edition: `2024`
- Minimum toolchain: Rust `1.85+`
- Main crates in use: `clap`, `anyhow`, `serde`, `serde_json`, `chrono`, `colored`, `regex`, `url`, `ureq`, `etcetera`, `fd-lock`, `open`, `rand`
- The app is offline-first. Network access is only used for explicit metadata/category fetch operations.

## Repository Layout

```text
src/main.rs        CLI definition and command dispatch
src/commands.rs    Command implementations
src/models.rs      Core data types and serde models
src/store.rs       Queue/config/history persistence and locking
src/stats.rs       Stats computation and rendering
src/paths.rs       Platform-specific path resolution
src/youtube.rs     YouTube URL and ID parsing
src/youtube_api.rs YouTube Data API client and duration parsing
```

## Build, Lint, and Test Commands

```bash
# Build
cargo build
cargo build --release

# Fast compile check
cargo check

# Format
cargo fmt
cargo fmt --check

# Lint
cargo clippy -- -W clippy::all

# Match CI's stricter clippy gate
cargo clippy -- -D warnings

# Full test suite
cargo test

# List all tests
cargo test -- --list

# Run a single test by exact name fragment
cargo test valid_video_id_direct
cargo test basic_stats_counts

# Run tests in one module
cargo test youtube::tests
cargo test stats::tests
cargo test youtube_api::tests

# Show test stdout
cargo test valid_video_id_direct -- --nocapture

# Run the CLI locally
cargo run -- list
cargo run -- add https://youtube.com/watch?v=dQw4w9WgXcQ
```

## CI Expectations

GitHub Actions currently runs:

- `cargo test`
- `cargo fmt --check`
- `cargo clippy -- -D warnings`

Before considering work complete, run:

```bash
cargo fmt
cargo clippy -- -D warnings
cargo test
```

## Testing Notes

- Tests live in `#[cfg(test)] mod tests` blocks at the bottom of each file.
- This project is a binary crate, so test names are typically namespaced like `youtube::tests::valid_video_id_direct`.
- `cargo test <substring>` is the normal way to run one test or a small group.
- Prefer descriptive test names that state behavior, not implementation details.
- Cover both success and failure paths when editing parsing, stats, or persistence logic.

## Code Style Overview

Follow existing patterns in the repo rather than introducing a new style.

### Naming

- Functions and variables: `snake_case`
- Modules/files: `snake_case`
- Structs/enums/traits: `PascalCase`
- Constants/statics: `SCREAMING_SNAKE_CASE`
- CLI flags/subcommands: `kebab-case`
- Tests: descriptive `snake_case`, often behavior-oriented like `config_serde_roundtrip`

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

### Formatting

- Always use `cargo fmt`.
- Do not manually preserve line wrapping that `rustfmt` wants to change.
- Keep functions and match arms readable; prefer the formatter's defaults.
- Use comments sparingly; most modules already rely on clear naming and short doc comments.

### Types and Data Modeling

- Use `struct` and `enum` definitions with derive macros where appropriate.
- Common derives in this codebase: `Debug`, `Clone`, `Serialize`, `Deserialize`, `PartialEq`, `Default`.
- Use `#[serde(default)]` for backward-compatible config/model evolution.
- Use `#[serde(rename_all = "lowercase")]` for enums exposed in JSON config.
- Prefer explicit domain types such as `Video`, `VideoMeta`, `Event`, `Config`, `Mode`.
- Keep serialized shapes stable; this app persists user data locally.

### Error Handling

- Use `anyhow::Result<T>` for fallible functions.
- Use `bail!(...)` for early user-facing failures.
- Use `anyhow!(...)` or `ok_or_else(...)` for inline error creation.
- Add context with `.context(...)` or `.with_context(...)` around I/O and parsing that can fail opaquely.
- Favor propagating errors with `?`.

Example:

```rust
use anyhow::{Context, Result, bail};

fn load(path: &Path) -> Result<String> {
    let data = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    if data.is_empty() {
        bail!("file is empty");
    }
    Ok(data)
}
```

## Project-Specific Implementation Patterns

### CLI and Command Flow

- CLI definitions use `clap` derive macros in `src/main.rs`.
- Help text is usually written as doc comments on enum variants and fields.
- Aliases and visible aliases are common; preserve existing command ergonomics.
- `main()` prints colored errors and exits non-zero; command logic lives in `run()` and `src/commands.rs`.

### Persistence and Locking

- Queue mutations must go through `store::with_queue(...)`.
- Queue reads should use `store::with_queue_read(...)`.
- Do not bypass locking for queue operations.
- Queue/config/metadata data is JSON; history is monthly JSONL.
- Persistence helpers often return defaults instead of failing on missing files.

### Parsing and Validation

- URL/ID parsing lives in `src/youtube.rs`.
- Regex statics use `std::sync::LazyLock`.
- Keep validation messages specific and user-friendly.
- Preserve support for multiple YouTube URL formats and explicit rejection of unsupported ones.

### Platform-Specific Code

- `src/paths.rs` uses `#[cfg(target_os = "windows")]` and `#[cfg(not(target_os = "windows"))]`.
- Continue using `etcetera` strategy selection rather than hand-rolled platform path logic.

### Stats and Time Handling

- Stats operate on UTC timestamps internally.
- Convert to local time for user-facing grouping such as weekdays, dates, and time-of-day buckets.
- If you change reporting logic, update both computation and rendering tests.

### Clippy Preferences Seen in This Repo

- Prefer `.is_some_and(...)` over older `map_or(false, ...)` patterns.
- Collapse nested `if`/`if let` when it improves clarity.
- Prefer iterator-based code over indexing loops.
- If a function truly needs many parameters, the repo uses targeted `#[allow(clippy::too_many_arguments)]` on that function.

## Test Authoring Guidelines

- Put tests at the bottom of the file they cover.
- Use small builder/helper functions inside test modules when setup is repetitive.
- Keep assertions direct and specific.
- Favor deterministic inputs over clock- or environment-sensitive behavior unless the test is explicitly about that.

## Practical Advice For Agents

- Read the neighboring module before editing; behavior is split cleanly by concern.
- Preserve backward compatibility for local data files where practical.
- When changing command behavior, check `README.md` and CLI help text for drift.
- When changing developer workflow expectations, keep this file aligned with `.github/workflows/ci.yml`.

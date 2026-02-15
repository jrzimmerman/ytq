# AGENTS.md - ytq

Guidelines for AI coding agents working in this repository.

## Project Overview

**ytq** is a Rust CLI tool for managing a YouTube video queue. Built with Rust 2024 edition (requires **Rust 1.85+**).

**Key dependencies:** clap (CLI parsing), serde/serde_json (serialization), anyhow (errors), chrono (timestamps), colored (terminal output), regex (URL parsing), url (URL parsing), ureq (HTTP client), etcetera (XDG/platform paths), fd-lock (file locking), open (browser launching), rand (random selection), either (iterator utilities)

## Build, Test, and Lint Commands

```bash
# Build
cargo build                    # Debug build
cargo build --release          # Release build

# Type checking (fast, no codegen)
cargo check

# Run all tests
cargo test

# Run a single test by name
cargo test valid_video_id_direct

# Run tests in a specific module
cargo test youtube::tests
cargo test models::tests

# Format code
cargo fmt

# Lint with clippy (use project's standard warnings)
cargo clippy -- -W clippy::all

# Run locally during development
cargo run -- <command> [args]
cargo run -- add https://youtube.com/watch?v=dQw4w9WgXcQ
cargo run -- list
```

## Project Structure

```
src/
├── main.rs        # Entry point, CLI definition, command dispatch
├── commands.rs    # Command implementations (add, next, list, etc.)
├── models.rs      # Data structures (Video, Config, Event, Mode)
├── stats.rs       # Statistics computation and rendering (stats, wrapped)
├── store.rs       # File I/O for queue, config, and history
├── paths.rs       # Platform-specific path resolution
├── youtube.rs     # YouTube URL/ID parsing and validation
│                  # Supports: watch, shorts, live, embed, v/, youtu.be
│                  # Rejects with helpful errors: channels, playlists, search
└── youtube_api.rs # YouTube Data API v3 client (metadata, categories)
```

## Code Style

### Naming Conventions

| Element               | Convention        | Example                     |
| --------------------- | ----------------- | --------------------------- |
| Functions, variables  | `snake_case`      | `extract_video_id`          |
| Types, enums, structs | `PascalCase`      | `Video`, `Mode`, `AppPaths` |
| Constants             | `SCREAMING_SNAKE` | `VIDEO_ID_RE`               |
| CLI commands/args     | `kebab-case`      | `--files-with-matches`      |
| Modules               | `snake_case`      | `youtube.rs`                |

### Import Organization

Group imports in this order, separated by blank lines:

1. Standard library (`use std::...`)
2. Crate modules (`use crate::...`)
3. External crates (alphabetically)

Within a `use` group, `cargo fmt` sorts items: lowercase identifiers first (alphabetically), then uppercase (alphabetically). For example, `{bail, Result}` not `{Result, bail}`.

```rust
use std::sync::LazyLock;

use crate::models::{Config, Event, Video};
use crate::{paths, store};

use anyhow::{bail, Result};
use chrono::Utc;
use regex::Regex;
```

### Error Handling

Use `anyhow` for all fallible functions:

```rust
use anyhow::{anyhow, bail, Context, Result};

pub fn example() -> Result<()> {
    // Use ? operator for propagation
    let data = fs::read_to_string(path)?;

    // Use bail! for early error returns
    if data.is_empty() {
        bail!("file is empty");
    }

    // Use anyhow! for inline errors
    let id = extract_id(&url)
        .ok_or_else(|| anyhow!("invalid URL: {url}"))?;

    // Add context to errors
    fs::create_dir_all(&dir)
        .with_context(|| format!("failed to create dir: {}", dir.display()))?;

    Ok(())
}
```

**Patterns:**

- Return `Result<T>` (alias for `anyhow::Result<T>`)
- Use `bail!("message")` instead of `return Err(anyhow!("message"))`
- Use `?` operator for error propagation
- Add context with `.context()` or `.with_context(|| format!(...))`

### Type Definitions

Use derive macros liberally:

```rust
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Video {
    pub id: String,
    pub url: String,
    pub added_at: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    Queue,
    Stack,
}
```

**Common derive traits:** `Debug`, `Clone`, `Serialize`, `Deserialize`, `PartialEq`, `Default`

### CLI Structure (clap)

```rust
#[derive(Parser)]
#[command(name = "ytq", version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Help text becomes doc comment
    #[command(alias = "a")]
    Add { input: String },
}
```

### Static Patterns

Use `LazyLock` for compiled regex and other static initialization:

```rust
use std::sync::LazyLock;

use regex::Regex;

static VIDEO_ID_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-zA-Z0-9_-]{11}$").unwrap());
```

### Platform-Specific Code

Use conditional compilation for platform differences:

```rust
#[cfg(target_os = "windows")]
use etcetera::app_strategy::Windows as Strategy;

#[cfg(not(target_os = "windows"))]
use etcetera::app_strategy::Xdg as Strategy;
```

## Formatting and Linting

**Always run `cargo fmt` and `cargo clippy` before considering any change complete.** Code must pass both without warnings.

```bash
# Format — always run after editing code
cargo fmt

# Lint — must pass with zero warnings
cargo clippy -- -W clippy::all
```

**Clippy rules to follow:**

- Prefer `.is_some_and(...)` over `.map_or(false, ...)`.
- Collapse nested `if` / `if let` into a single `if` with `&&` chains when possible.
- Use `for (i, item) in iter.enumerate()` instead of indexing with `for i in 0..len`.
- Respect the default argument limit (7). If a function needs more, add `#[allow(clippy::too_many_arguments)]` explicitly.
- Let `cargo fmt` handle all whitespace, line-wrapping, and trailing-comma decisions — do not fight the formatter.

**Workflow:** After any code change, run `cargo fmt` first, then `cargo clippy -- -W clippy::all`, then `cargo test`. Fix any issues before moving on.

## Testing

Place tests in a `#[cfg(test)]` module at the bottom of each file:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name_describes_behavior() {
        // Arrange
        let input = "test input";

        // Act
        let result = function_under_test(input);

        // Assert
        assert_eq!(result.unwrap(), expected);
    }
}
```

**Test naming:** Use descriptive names like `valid_video_id_direct`, `config_serde_roundtrip`. Test both success and failure cases.

```bash
cargo test                           # All tests
cargo test video                     # Tests matching "video"
cargo test youtube::tests            # Tests in youtube module
cargo test valid_video_id_direct     # Specific test
cargo test -- --nocapture            # Show println! output
```

## Common Idioms

### Result Handling in main()

```rust
fn main() {
    if let Err(e) = run() {
        eprintln!("{} {e:#}", "error:".red());
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    // ... command dispatch
}
```

### Optional Fields with Serde

```rust
#[derive(Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub mode: Mode,  // Uses Mode::default() if missing
}
```

### User-Facing Output

```rust
use colored::Colorize;

println!("{} {id}", "Added:".green());
println!("{}", "Queue is empty.".yellow());
eprintln!("{} {e:#}", "error:".red());
```

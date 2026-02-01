# ytq

**The YouTube Queue for the Terminal.**

**ytq** ("YouTube Queue") is a local CLI tool designed to cure "Browser Tab Fatigue." It lets you stash videos for later, watch them in your browser when you're ready, and finally close those tabs.

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

| Command                    | Aliases                  | Description                                                   |
| -------------------------- | ------------------------ | ------------------------------------------------------------- |
| `ytq add <input>`          | `a`                      | Add video. Accepts URLs or IDs.                               |
| `ytq next [target]`        | `play`, `watch`, `open`  | Watch & Pop. Opens browser, logs event, removes from queue. Optionally specify a video ID/URL to watch a specific video. |
| `ytq random`               | `lucky`                  | Pop and watch a random video from the queue.                  |
| `ytq peek [n]`             |                          | Look ahead. Show the next n videos (default: 1).              |
| `ytq list`                 | `ls`                     | List all. Shows the full queue with local timestamps.         |
| `ytq remove <target>`      | `rm`, `delete`           | Delete. Removes item by ID or URL matching.                   |
| `ytq stats`                |                          | Metrics. Shows your viewing statistics (added, watched, skipped). |
| `ytq config <key> <value>` |                          | Settings. Keys: `mode` (stack/queue).                         |
| `ytq info`                 |                          | Debug. Prints the exact paths where your data is stored.      |

## Configuration

Your preferences live in `config.json`. You can modify them via the CLI.

**Switch to "Stack" Mode (LIFO)**

Tired of watching old videos? Switch to Stack mode to always watch the most recently added video first.

```bash
ytq config mode stack
```

**Switch back to "Queue" Mode (FIFO)**

```bash
ytq config mode queue
```

## Development

Want to hack on `ytq`?

```bash
# Run locally without installing
cargo run -- add https://www.youtube.com/watch?v=dQw4w9WgXcQ

# Check for errors
cargo check

# Lint with clippy
cargo clippy -- -W clippy::all

# Run the test suite
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

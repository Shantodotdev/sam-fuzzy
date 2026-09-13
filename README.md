# SAM-FUZZY ◈ SamOnline Millisecond Media Explorer

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-2024_Edition-orange.svg)](https://www.rust-lang.org/)
[![Platform](https://img.shields.io/badge/Platform-Linux%20%7C%20macOS%20%7C%20Windows-lightgrey.svg)]()

A blazing-fast, cyberpunk terminal user interface (TUI) for exploring, fuzzy-searching, and streaming movies, web series, foreign cinema, and anime hosted on **SamOnline (DhakaFlix)** FTP servers.

Built with **Rust**, **Ratatui**, **Crossterm**, **Rayon**, and **Nucleo-Matcher**, styled with the **Blacksparrow** pink/maroon cybernetic aesthetic.

---

## Features

- ⚡ **Zero-Lag Search (<10ms)**: Multi-tiered search engine utilizing 64-bit character presence bitmasks for sub-nanosecond candidate rejection, `rayon` multi-core SIMD scoring, and a dedicated non-blocking background worker thread with locked 60 FPS UI responsiveness.
- 🎬 **104,650+ Media Items Indexed**: Complete dataset scraped across SamOnline DhakaFlix servers (7 / 14 / 12), with human-readable file sizes (MB/GB), release years, and normalized resolutions (`4K`, `1080p`, `720p`, etc.).
- 🔢 **Clean Sequential Indexing & Natural Sorting**: Features 1-based index numbers aligned without dot artifacts, coupled with natural alphanumeric sorting (`natural_cmp`) ensuring seasons and episodes (`S01E02` before `S01E10`) are strictly ordered.
- 📐 **Streamlined Dual-Pane View**: Minimalist search results list with right-aligned resolution tags alongside an Inspector pane displaying release year, quality, file size, category, server mirror, and directory breadcrumbs with automatic text wrapping.
- 🌐 **Instant Browser Launch**: Press `Enter` on any title to open its DhakaFlix stream or directory directly in your default browser.
- 🎥 **MPV / VLC Player Integration**: Press `Alt+P` or `F3` to launch and stream videos directly in your desktop media player.
- 📋 **One-Key Clipboard Integration**: Press `Alt+C`, `Ctrl+Y`, or `F2` to copy direct streaming HTTP links directly to your system clipboard.
- 🗂️ **Categorical Navigation**: Instantly filter across `All`, `English`, `TV Series`, `Korean`, `Hindi`, and `Animation` tabs with `Tab` / `Shift+Tab`.

---

## Architecture & Performance

Searching through 104,000+ items on every keystroke without UI stutter requires a multi-stage pipeline:

```
[ Keystroke Input ] (Main Thread - 60 FPS)
         │
         ▼ (MPSC Channel with atomic sequence cancellation)
[ Worker Thread ]
         │
         ├── Stage 1: Fast 64-bit Bitmask Filter (<1ms)
         │     Rejects items missing any required query character:
         │     (item_mask & query_mask) == query_mask
         │
         ├── Stage 2: Rayon Parallel Fuzzy Scoring (~3-8ms)
         │     Distributes surviving candidates across all CPU cores
         │     via nucleo-matcher SIMD algorithms
         │
         └── Stage 3: Top-K Extraction & Natural Sort
         │
         ▼ (MPSC Channel)
[ Render Engine ] (Ratatui terminal buffer sync)
```

1. **64-bit Character Presence Bitmask**: Each indexed media title precomputes a 64-bit bitmask encoding presence of lowercase alphanumeric characters. A query's bitmask immediately eliminates >80% of irrelevant titles via single CPU bitwise `AND` instructions without heap allocations or string comparisons.
2. **Rayon Data-Parallel Scoring**: Surviving titles are evaluated in parallel chunks across all CPU cores using the `nucleo-matcher` fuzzy algorithm (the same algorithm powering modern code editors).
3. **Non-Blocking Background Worker**: Search operations run on a dedicated worker thread via Rust `std::sync::mpsc` channels. Keystrokes cancel prior in-flight searches via monotonic sequence IDs, guaranteeing 0ms input latency on the main event loop.

---

## Keyboard Shortcuts

| Key | Action |
|---|---|
| `Enter` | **Open in default web browser** (stream / DhakaFlix player) |
| `Alt + C` / `F2` / `Ctrl + Y` | **Copy direct URL** to system clipboard |
| `Alt + P` / `F3` | **Stream in MPV / VLC** external player |
| `Alt + F` / `F4` | **Open parent folder** in browser |
| `Tab` / `Shift + Tab` | Switch category tabs |
| `↑` / `↓` (`Ctrl + P` / `Ctrl + N`) | Navigate search results |
| `PageUp` / `PageDown` | Scroll results page-by-page |
| `Home` / `End` | Jump to top / bottom of results |
| `Ctrl + U` | Clear search query |
| `Esc` | Clear query or quit |
| `Ctrl + C` / `Ctrl + D` | Quit application |

---

## Building and Running

### Prerequisites

- [Rust toolchain](https://www.rust-lang.org/tools/install) (2024 edition or 1.85+)
- Optional: `mpv` or `vlc` for external video streaming

### Running the Optimized Release Binary

```bash
cargo run --release
```

Or execute the compiled binary directly:
```bash
./target/release/sam-fuzzy
```

### CLI Options

```text
Usage: sam-fuzzy [OPTIONS]

Options:
  -d, --data <DATA>          Path to sam_media.json dataset (defaults to data/sam_media.json)
  -q, --query <QUERY>        Initial search query (e.g. -q "witcher")
  -c, --category <CATEGORY>  Initial category filter (e.g. -c "Korean")
  -h, --help                 Print help
  -V, --version              Print version
```

---

## Testing

Run the full automated test suite with `cargo test` or `cargo-nextest`:

```bash
cargo nextest run
```

All 27 unit tests, state machine tests, fuzzy performance benchmarks, and real-dataset integration tests pass in ~1 second.

---

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

Copyright (c) 2026 KR Shanto

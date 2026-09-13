# SAM-FUZZY ◈ SamOnline Millisecond Media Explorer

[![CI](https://github.com/Shantodotdev/sam-fuzzy/actions/workflows/ci.yml/badge.svg)](https://github.com/Shantodotdev/sam-fuzzy/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/Shantodotdev/sam-fuzzy?color=blue)](https://github.com/Shantodotdev/sam-fuzzy/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-2024_Edition-orange.svg)](https://www.rust-lang.org/)
[![Binary Size](https://img.shields.io/badge/Binary_Size-~6.7_MB-success.svg)]()

A blazing-fast, cyberpunk terminal user interface (TUI) for exploring, fuzzy-searching, and streaming movies, web series, foreign cinema, and anime hosted on **SamOnline (DhakaFlix)** FTP servers.

Built with **Rust**, **Ratatui**, **Crossterm**, **Rayon**, and **Nucleo-Matcher**, styled with the **Blacksparrow** pink/maroon cybernetic aesthetic.

---

## Quick Installation

### Linux & macOS (One-Line Installer)
Install the pre-compiled, optimized standalone binary directly into `/usr/local/bin`:

```bash
curl -fsSL https://raw.githubusercontent.com/Shantodotdev/sam-fuzzy/main/install.sh | bash
```

### Pre-Compiled Standalone Binaries
Every release comes bundled as a self-contained executable with the full 104,650+ media index embedded inside (~6.7 MB total). No installation, extra folders, or dependencies needed.

| Operating System | Architecture | Binary Type | Direct Download |
|---|---|---|---|
| **Windows** | x86_64 (Windows 10/11) | Executable (`.exe`) | [Download `sam-fuzzy-windows-x86_64.exe`](https://github.com/Shantodotdev/sam-fuzzy/releases/latest/download/sam-fuzzy-windows-x86_64.exe) |
| **macOS** | Apple Silicon (M1/M2/M3/M4) | Executable (Mach-O) | [Download `sam-fuzzy-macos-arm64`](https://github.com/Shantodotdev/sam-fuzzy/releases/latest/download/sam-fuzzy-macos-arm64) |
| **Linux** | x86_64 | Executable (ELF) | [Download `sam-fuzzy-linux-x86_64`](https://github.com/Shantodotdev/sam-fuzzy/releases/latest/download/sam-fuzzy-linux-x86_64) |

---

## Features

- ⚡ **Zero-Lag Search (<10ms)**: Multi-tiered search engine utilizing 64-bit character presence bitmasks for sub-nanosecond candidate rejection, `rayon` multi-core SIMD scoring, and a dedicated non-blocking background worker thread with locked 60 FPS UI responsiveness.
- 📦 **Zero-Dependency Standalone Executable (~6.7 MB)**: Bundles the complete 104,650+ media dataset using pure-Rust streaming gzip decompression. Run it anywhere from a single binary with zero external files or network setup required.
- 🎬 **104,650+ Media Items Indexed**: Complete dataset scraped across SamOnline DhakaFlix servers (7 / 14 / 12), with human-readable file sizes (MB/GB), release years, and normalized resolutions (`4K`, `1080p`, `720p`, etc.).
- 🔢 **Clean Sequential Indexing & Natural Sorting**: Features 1-based index numbers aligned without dot artifacts, coupled with natural alphanumeric sorting (`natural_cmp`) ensuring seasons and episodes (`S01E02` before `S01E10`) are strictly ordered.
- 📐 **Streamlined Dual-Pane View**: Minimalist search results list with right-aligned resolution tags alongside an Inspector pane displaying release year, quality, file size, category, server mirror, and directory breadcrumbs with automatic text wrapping.
- 🌐 **Instant Browser Launch**: Press `Enter` on any title to open its DhakaFlix stream or directory directly in your default browser.
- 🎥 **MPV / VLC Player Integration**: Press `Alt+P` or `F3` to launch and stream videos directly in your desktop media player.
- 📋 **One-Key Clipboard Integration**: Press `Alt+C`, `Ctrl+Y`, or `F2` to copy direct streaming HTTP links directly to your system clipboard.
- 🗂️ **Categorical Navigation**: Instantly filter across `All`, `English`, `TV Series`, `Korean`, `Hindi`, and `Animation` tabs with `Tab` / `Shift+Tab`.

---

## Architecture & Data Strategy

### 1. Minimal Distribution Strategy
The raw JSON dataset is **93.3 MB**. Distributing 95+ MB packages causes heavy bandwidth and disk footprint overhead. `sam-fuzzy` implements a 3-tier distribution strategy:

```
[ Packaging ]
  Raw Dataset (93.3 MB) ──> Gzip Stream Compression ──> sam_media.json.gz (5.1 MB)
                                                                 │
                                                 Embedded via include_bytes!
                                                                 │
                                                                 ▼
                                                    Standalone Binary (~6.7 MB)

[ Runtime Execution ]
  sam-fuzzy launch
        │
        ├── 1. CLI Override: --data <PATH> (.json or .gz)
        │
        ├── 2. Local File: Check ./data/sam_media.json(.gz)
        │
        └── 3. Embedded Fallback: Stream-decompress embedded 5.1 MB dataset in-memory
```

- **Decompression Speed**: Streamed into memory in ~150ms at startup.
- **Portability**: The binary can be moved anywhere on your system or shared with friends without copying data folders.
- **Customizability**: Users can supply their own updated dataset at any time via `--data <PATH>` or by placing a newer `sam_media.json` / `sam_media.json.gz` file in `./data/`.

### 2. Search Engine Pipeline

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
2. **Rayon Data-Parallel Scoring**: Surviving titles are evaluated in parallel chunks across all CPU cores using the `nucleo-matcher` fuzzy algorithm.
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

## Building from Source

### Prerequisites

- [Rust toolchain](https://www.rust-lang.org/tools/install) (2024 edition or 1.85+)
- Optional: `mpv` or `vlc` for external video streaming

### Running the Optimized Release Binary

```bash
cargo run --release
```

Or build and execute the binary directly:
```bash
cargo build --release
./target/release/sam-fuzzy
```

### CLI Options

```text
Usage: sam-fuzzy [OPTIONS]

Options:
  -d, --data <DATA>          Custom path to dataset (.json or .json.gz)
  -q, --query <QUERY>        Initial search query (e.g. -q "witcher")
  -c, --category <CATEGORY>  Initial category filter (e.g. -c "Korean")
  -h, --help                 Print help
  -V, --version              Print version
```

---

## Automated CI/CD & Maintenance

- **Continuous Integration (`ci.yml`)**: Automatically validates formatting, clippy linter, and full test suites across Linux, macOS, and Windows runners on every push and PR.
- **Cross-Platform Releases (`release.yml`)**: Builds, strips, packages, and signs binaries across 5 platform targets upon pushing a semantic version tag (e.g. `git tag v0.1.0 && git push origin v0.1.0`).
- **Dataset Auto-Refresh (`update-dataset.yml`)**: Scheduled workflow crawls DhakaFlix servers monthly for newly added media, re-compresses the dataset, and opens a Pull Request with validation tests.

---

## Testing

Run the full automated test suite with `cargo test` or `cargo-nextest`:

```bash
cargo nextest run
```

All 29 unit tests, state machine tests, fuzzy performance benchmarks, gzip streaming, and real-dataset integration tests pass in ~1.9 seconds.

---

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

Copyright (c) 2026 KR Shanto

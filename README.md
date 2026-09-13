# SAM-FUZZY ◈ SamOnline Millisecond Media Explorer

A blazing-fast, cyberpunk terminal user interface (TUI) for searching movies, web series, Bengali films, foreign cinema, and anime on **SamOnline (DhakaFlix)** FTP servers using parallel multi-core fuzzy matching (like `fzf`).

Built with **Rust**, **Ratatui**, **Crossterm**, **Rayon**, and **Nucleo-Matcher**, styled with the **Blacksparrow** pink/maroon cybernetic aesthetic.

---

## Features

- ⚡ **Zero-Lag Search (<10ms)**: Uses a multi-tiered search architecture with 64-bit character presence bitmasks for sub-nanosecond rejection, `rayon` parallel multi-core SIMD scoring, and a dedicated non-blocking background worker thread running at locked 60 FPS.
- 🎬 **104,650+ Media Items Indexed**: Complete mirror dataset of all SamOnline DhakaFlix servers (7 / 14 / 12) with accurate human-readable file sizes (MB/GB) and normalized resolutions (`4K`, `1080p`, `720p`, etc.).
- 🔢 **Clean Sequential Indexing & Natural Sorting**: Features 1-based index numbers right-aligned without dots for perfect column alignment, and natural alphanumeric sorting (`natural_cmp`) so multi-season series and episodes (`S01E02` before `S01E10`) are sequenced logically.
- 📐 **Streamlined Dual-Pane View**: Minimalist search results list with flush right-aligned resolution badges alongside a clean, focused Inspector panel displaying release year, quality, file size, category, server mirror, and directory hierarchy with automatic text wrapping.
- 🌐 **Instant Browser Launch**: Press `Enter` on any title to open its DhakaFlix stream/player or folder directly in your browser.
- 🎥 **MPV / VLC Integration**: Press `Alt+P` or `F3` to stream videos directly in your favorite desktop player.
- 📋 **One-Key Clipboard**: Press `Alt+C`, `Ctrl+Y`, or `F2` to copy direct streaming HTTP links.
- 🗂️ **Categorical Navigation**: Instantly switch between `All`, `English`, `TV Series`, `Korean`, `Hindi`, and `Animation` tabs with `Tab` / `Shift+Tab`.

---

## Keyboard Shortcuts

| Key | Action |
|---|---|
| `Enter` | **Open in default web browser** (stream / DhakaFlix player) |
| `Alt + C` / `F2` / `Ctrl + Y` | **Copy direct URL** to clipboard |
| `Alt + P` / `F3` | **Stream in MPV / VLC** external player |
| `Alt + F` / `F4` | **Open parent folder** in browser |
| `Tab` / `Shift + Tab` | Switch category tabs |
| `↑` / `↓` (`Ctrl + P` / `Ctrl + N`) | Navigate search results |
| `PageUp` / `PageDown` | Scroll results page-by-page |
| `Home` / `End` | Jump to top / bottom of results |
| `Ctrl + U` | Clear search query |
| `Esc` | Clear query or quit |
| `Ctrl + C` / `Ctrl + D` | Quit |

---

## Building and Running

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

Run the test suite with `cargo-nextest`:
```bash
cargo nextest run
```
All 27 unit, state-machine, fuzzy performance, and real-dataset integration tests pass in ~1 second.

# SAM-FUZZY ◈ SamOnline Millisecond Media Explorer

A blazing-fast, cyberpunk terminal user interface (TUI) for searching movies, web series, Bengali films, foreign cinema, and anime on **SamOnline (DhakaFlix)** FTP servers using millisecond fuzzy matching (like `fzf`).

Built with **Rust**, **Ratatui**, **Crossterm**, and **Nucleo-Matcher**, styled with the **Blacksparrow** pink/maroon cybernetic aesthetic.

---

## Features

- ⚡ **Sub-Millisecond Search**: Uses `nucleo-matcher` (the Helix/Zed fuzzy engine) to filter and rank across **38,000+** media titles in single-digit milliseconds.
- 🎨 **Blacksparrow Aesthetic**: Cyberpunk hot pink (`#ff007f`), maroon (`#c7005f`), neon green, and cyan palette with ASCII banner and live telemetry stats.
- 🌐 **Instant Browser Launch**: Press `Enter` on any title to open its DhakaFlix stream/player or folder directly in your browser.
- 🎬 **MPV / VLC Integration**: Press `Alt+P` or `F3` to stream videos directly in your favorite desktop player.
- 📋 **One-Key Clipboard**: Press `Alt+C` or `F2` to copy the direct streaming HTTP link.
- 🗂️ **Categorical Tabs**: Instantly switch between `All`, `English`, `Bangla`, `Korean`, `Chinese/Japanese`, `Foreign`, `3D`, `Files Only`, and `Folders Only` with `Tab` / `Shift+Tab`.
- 📁 **100% Self-Contained Local Dataset**: Bundles `data/sam_media.json` (38,368 items extracted from Blacksparrow's SamOnline crawl).

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

### Running in Development
```bash
cargo run
```

### Running the Optimized Release Binary
```bash
cargo run --release
```

Or run the compiled binary directly:
```bash
./target/release/sam-fuzzy
```

### CLI Options
```text
Usage: sam-fuzzy [OPTIONS]

Options:
  -d, --data <DATA>          Path to sam_media.json dataset (defaults to data/sam_media.json)
  -q, --query <QUERY>        Initial search query (e.g. -q "dark knight")
  -c, --category <CATEGORY>  Initial category filter (e.g. -c "Bangla")
  -h, --help                 Print help
  -V, --version              Print version
```

---

## Testing

Run tests with `cargo-nextest`:
```bash
cargo nextest run
```
All 16 unit, state-machine, fuzzy performance, and real-dataset integration tests run in ~0.3 seconds.

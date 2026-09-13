//! CLI entrypoint and event loop for `sam-fuzzy`.
//!
//! Orchestrates application lifecycle:
//! 1. Installs a terminal-restoring panic hook to prevent shell corruption on panic.
//! 2. Parses command-line arguments and resolves the dataset location.
//! 3. Loads and indexes media items into memory.
//! 4. Configures raw mode and switches to the alternate screen buffer.
//! 5. Runs the event loop with responsive 50ms polling.
//! 6. Restores the terminal cleanly upon exit or error.

use anyhow::Context;
use clap::Parser;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use sam_fuzzy::app::App;
use sam_fuzzy::fuzzy::SearchEngine;
use sam_fuzzy::models::load_dataset;
use sam_fuzzy::ui::render_ui;
use std::io::{self, stdout};
use std::path::PathBuf;
use std::time::Duration;

/// Command-line configuration flags.
#[derive(Parser, Debug)]
#[command(
    name = "sam-fuzzy",
    author = "KR Shanto",
    version = "0.1.0",
    about = "Lightning-fast fzf-style TUI for SamOnline FTP media discovery"
)]
struct Args {
    /// Custom path to the `sam_media.json` dataset.
    #[arg(short, long)]
    data: Option<PathBuf>,

    /// Initial search query to execute immediately on launch.
    #[arg(short, long)]
    query: Option<String>,

    /// Initial category filter tab to activate on launch.
    #[arg(short, long)]
    category: Option<String>,
}

/// Resolves the absolute or relative path to the media dataset JSON file.
///
/// Evaluates candidate locations in order of precedence:
/// 1. User-supplied CLI argument (`--data <PATH>`).
/// 2. Common relative directory paths (`data/sam_media.json`, `../data/sam_media.json`, `sam_media.json`).
/// 3. Path relative to the binary's executable location (useful for standalone installs).
fn resolve_data_path(custom: Option<PathBuf>) -> anyhow::Result<PathBuf> {
    // 1. Explicit user override
    if let Some(p) = custom {
        if p.exists() {
            return Ok(p);
        }
        anyhow::bail!("Specified dataset path not found: {:?}", p);
    }

    // 2. Relative candidate paths from current working directory
    let candidates = [
        PathBuf::from("data/sam_media.json"),
        PathBuf::from("../data/sam_media.json"),
        PathBuf::from("sam_media.json"),
    ];

    for c in &candidates {
        if c.exists() {
            return Ok(c.clone());
        }
    }

    // 3. Fallback: Check folder relative to the executable
    if let Ok(exe_path) = std::env::current_exe()
        && let Some(parent) = exe_path.parent()
    {
        let exe_data = parent.join("data/sam_media.json");
        if exe_data.exists() {
            return Ok(exe_data);
        }
    }

    anyhow::bail!(
        "Could not find 'data/sam_media.json'. Ensure the local dataset exists or pass --data <PATH>"
    )
}

/// Initializes the terminal in raw mode on an alternate screen.
///
/// Raw mode disables terminal line-buffering and local echo, allowing the application
/// to handle keystrokes immediately. The alternate screen prevents overwriting shell scrollback.
fn setup_terminal() -> anyhow::Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode().context("Failed to enable raw mode")?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen).context("Failed to enter alternate screen")?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend).context("Failed to initialize terminal")?;
    Ok(terminal)
}

/// Restores the terminal to its standard cooked state and primary screen buffer.
fn restore_terminal(mut terminal: Terminal<CrosstermBackend<io::Stdout>>) -> anyhow::Result<()> {
    disable_raw_mode().context("Failed to disable raw mode")?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)
        .context("Failed to leave alternate screen")?;
    terminal.show_cursor().context("Failed to show cursor")?;
    Ok(())
}

/// Installs a safety panic hook that restores the terminal before printing panic diagnostics.
///
/// Without this, a panic would leave the user's terminal stuck in raw mode with no echo
/// and alternate screen active, requiring a manual `reset` command.
fn install_panic_hook() {
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = disable_raw_mode();
        let _ = execute!(stdout(), LeaveAlternateScreen);
        original_hook(panic_info);
    }));
}

fn main() -> anyhow::Result<()> {
    install_panic_hook();
    let args = Args::parse();

    // Resolve dataset location and load items into memory
    let data_path = resolve_data_path(args.data)?;
    println!("Loading SamOnline media index from {:?}...", data_path);
    let items = load_dataset(&data_path)
        .context(format!("Failed to parse dataset from {:?}", data_path))?;
    println!("Loaded {} items into memory.", items.len());

    // Initialize fuzzy engine and application state
    let engine = SearchEngine::new(items);
    let mut app = App::new(engine);

    // Apply CLI query filter if provided
    if let Some(q) = args.query {
        app.query = q;
        app.perform_search();
    }

    // Apply CLI category filter if provided
    if let Some(cat) = args.category {
        app.set_category_by_name(&cat);
    }

    // Enter TUI alternate screen
    let mut terminal = setup_terminal()?;

    // Spawn non-blocking background search worker thread
    app.spawn_search_worker();

    // Run the main event loop
    let run_res = run_app(&mut terminal, &mut app);

    // Always restore the terminal cleanly, even if run_app encountered an error
    restore_terminal(terminal)?;

    if let Err(e) = run_res {
        eprintln!("Application error: {:?}", e);
    }

    Ok(())
}

/// Main application event loop.
///
/// Draws UI frames on every iteration and polls keyboard events with a 16ms
/// timeout (60 FPS) to maintain zero input lag while keeping CPU usage near zero.
/// Filters out non-press events (e.g. key release) to avoid double-firing actions.
fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> anyhow::Result<()> {
    loop {
        // Poll for completed background search results without blocking
        app.poll_search_results();

        // Redraw current terminal frame
        terminal.draw(|f| render_ui(f, app))?;

        if app.should_quit {
            break;
        }

        // Poll for keyboard input with 16ms timeout (60 FPS responsiveness)
        if event::poll(Duration::from_millis(16))?
            && let Event::Key(key) = event::read()?
        {
            // Ignore key release/repeat events on platforms supporting the Kitty keyboard protocol
            if key.kind != KeyEventKind::Press {
                continue;
            }

                // Global keyboard event routing
                match (key.modifiers, key.code) {
                    // Quit: Ctrl+C or Ctrl+D
                    (KeyModifiers::CONTROL, KeyCode::Char('c'))
                    | (KeyModifiers::CONTROL, KeyCode::Char('d')) => {
                        app.should_quit = true;
                        break;
                    }

                    // Clear search query: Ctrl+U
                    (KeyModifiers::CONTROL, KeyCode::Char('u')) => {
                        app.on_clear_query();
                    }

                    // Emacs-style list navigation: Ctrl+N (Next), Ctrl+P (Previous)
                    (KeyModifiers::CONTROL, KeyCode::Char('n')) => {
                        app.select_next();
                    }
                    (KeyModifiers::CONTROL, KeyCode::Char('p')) => {
                        app.select_prev();
                    }

                    // Copy streaming URL to clipboard: Alt+C, Ctrl+Y, Ctrl+L, or F2
                    (KeyModifiers::ALT, KeyCode::Char('c') | KeyCode::Char('C'))
                    | (KeyModifiers::CONTROL, KeyCode::Char('y') | KeyCode::Char('Y'))
                    | (KeyModifiers::CONTROL, KeyCode::Char('l') | KeyCode::Char('L'))
                    | (_, KeyCode::F(2)) => {
                        app.copy_selected_link();
                    }

                    // Direct stream launch in MPV/VLC external player: Alt+P, Ctrl+O, or F3
                    (KeyModifiers::ALT, KeyCode::Char('p') | KeyCode::Char('P'))
                    | (KeyModifiers::CONTROL, KeyCode::Char('o') | KeyCode::Char('O'))
                    | (_, KeyCode::F(3)) => {
                        app.play_selected_video();
                    }

                    // Open parent folder listing in web browser: Alt+F or F4
                    (KeyModifiers::ALT, KeyCode::Char('f') | KeyCode::Char('F'))
                    | (_, KeyCode::F(4)) => {
                        app.open_folder_in_browser();
                    }

                    // Toggle help modal: F1
                    (_, KeyCode::F(1)) => {
                        app.toggle_help();
                    }

                    // [Enter] -> Primary action: Opens target URL directly in web browser!
                    (_, KeyCode::Enter) => {
                        app.open_selected_in_browser();
                    }

                    // Vertical arrow navigation
                    (_, KeyCode::Up) => {
                        app.select_prev();
                    }
                    (_, KeyCode::Down) => {
                        app.select_next();
                    }

                    // Paging navigation (15 items per page)
                    (_, KeyCode::PageUp) => {
                        app.select_prev_page(15);
                    }
                    (_, KeyCode::PageDown) => {
                        app.select_next_page(15);
                    }

                    // Boundary navigation: Home (top) and End (bottom)
                    (_, KeyCode::Home) => {
                        app.select_first();
                    }
                    (_, KeyCode::End) => {
                        app.select_last();
                    }

                    // Category cycling: Tab (next category), Shift+Tab / BackTab (previous category)
                    (_, KeyCode::Tab) => {
                        app.next_category();
                    }
                    (_, KeyCode::BackTab) => {
                        app.prev_category();
                    }

                    // Backspace: delete previous character from search query
                    (_, KeyCode::Backspace) => {
                        app.on_backspace();
                    }

                    // Escape: clear active query first; if already empty, quit application
                    (_, KeyCode::Esc) => {
                        if !app.query.is_empty() {
                            app.on_clear_query();
                        } else {
                            app.should_quit = true;
                            break;
                        }
                    }

                    // Normal typing: append character to query and execute fuzzy search
                    (KeyModifiers::NONE | KeyModifiers::SHIFT, KeyCode::Char(c)) => {
                        app.on_key_char(c);
                    }

                    _ => {}
                }
            }
        }

    Ok(())
}

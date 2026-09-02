//! Anti-library — read a plain text file like a book, in the terminal.

#[path = "../app.rs"]
mod app;
#[path = "../ui.rs"]
mod ui;

use anti_library::{library, text};

use anyhow::{bail, Result};
use app::{App, Mode};
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
    KeyModifiers, MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use library::Library;
use ratatui::prelude::*;
use std::io::stdout;
use std::path::PathBuf;
use std::time::Duration;
use text::Document;

const USAGE: &str = "\
anti-library — read a document like a book, in the terminal

USAGE:
    antilib <file>
    antilib --recent            list books you have opened before
    antilib --forget <file>     take a book off the shelf
    antilib --backup <file>     write the whole shelf out, marks and all
    antilib --restore <file>    fold a backup back in (nothing is removed)
    antilib --diagnostics [file]  what to attach to a bug report
    antilib --version
    antilib --help

Reads .txt, .md, Word (.docx), PDF, EPUB, ODT, RTF and HTML.
Progress and bookmarks are shared with the desktop reader, which is also
where passages are highlighted.";

fn main() -> Result<()> {
    anti_library::crash::install();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut lib = Library::load();
    if let Some(kept) = lib.damaged_store() {
        eprintln!(
            "note: the library file could not be read and was set aside as {}. \
             Your marks are in it — this run starts a new one.",
            kept.display()
        );
    }

    match args.first().map(String::as_str) {
        None | Some("-h") | Some("--help") => {
            println!("{USAGE}");
            return Ok(());
        }
        Some("-V") | Some("--version") => {
            println!("anti-library {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Some("--forget") => {
            let Some(target) = args.get(1) else {
                bail!("--forget needs the path of a book to take off the shelf");
            };
            let key = library::key_for(std::path::Path::new(target));
            if lib.forget(&key) {
                lib.save()?;
                println!("Took {key} off the shelf. The file itself is untouched.");
            } else {
                println!("No book on the shelf at {key}.");
            }
            return Ok(());
        }
        Some("--backup") => {
            let Some(target) = args.get(1) else {
                bail!("--backup needs a file to write the shelf to");
            };
            let path = PathBuf::from(target);
            lib.export_to(&path)?;
            println!(
                "Wrote {} book(s) to {}.",
                lib.books.len(),
                path.display()
            );
            return Ok(());
        }
        Some("--restore") => {
            let Some(source) = args.get(1) else {
                bail!("--restore needs the backup file to read");
            };
            let report = lib.import_from(std::path::Path::new(source))?;
            lib.save()?;
            println!("{report}.");
            return Ok(());
        }
        Some("--diagnostics") => {
            let report = anti_library::diagnostics::report(&lib);
            match args.get(1) {
                Some(target) => {
                    std::fs::write(target, &report)?;
                    println!("Wrote {target}. Read it before you send it on.");
                }
                None => print!("{report}"),
            }
            return Ok(());
        }
        Some("--recent") => {
            if lib.books.is_empty() {
                println!("No books read yet.");
            }
            for line in recent_lines(&lib) {
                println!("{line}");
            }
            return Ok(());
        }
        _ => {}
    }

    let path = PathBuf::from(&args[0]);
    if !path.is_file() {
        bail!("not a readable file: {}", path.display());
    }
    let path = std::fs::canonicalize(&path).unwrap_or(path);
    let doc = Document::load(&path)?;
    if doc.rtl {
        eprintln!(
            "note: {} is written right to left, which this reader cannot set — \
the words will appear in the wrong order.",
            path.display()
        );
    }
    let mut app = App::new(doc, lib);

    run(&mut app)?;
    app.persist();
    Ok(())
}


/// The shelf, written out in the order the listing promises.
///
/// `--recent` says "books you have opened before" and used to walk the map
/// itself, which is a `BTreeMap` — so the order was alphabetical by path and
/// had nothing to do with when anything was read. The desktop reader's start
/// screen had this same defect fixed, and `recent()` was written for it; this
/// caller was simply never moved over.
fn recent_lines(lib: &Library) -> Vec<String> {
    lib.recent()
        .into_iter()
        .map(|(path, rec)| {
            format!(
                "{:<40} {} bookmark(s)  @{}",
                rec.title,
                rec.bookmarks.len(),
                path
            )
        })
        .collect()
}

/// Put the terminal back the way it was found.
///
/// Idempotent on purpose: it runs from the guard below and from the panic
/// hook, and whichever gets there first is the one that mattered.
fn restore_terminal() {
    let _ = disable_raw_mode();
    let _ = execute!(
        stdout(),
        LeaveAlternateScreen,
        DisableMouseCapture,
        crossterm::cursor::Show
    );
}

/// Restores the terminal however the reader leaves — returning, erroring, or
/// panicking.
///
/// The cleanup used to sit after the event loop, so it ran only when the loop
/// came back normally. A panic went straight past it and left the reader in a
/// shell with no echo, where Ctrl+C does nothing and the only way out is to
/// close the window.
struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore_terminal();
    }
}

/// Put the screen back *before* the panic message is written.
///
/// Unwinding runs the guard too, but it runs after the hooks — and a message
/// printed onto the alternate screen goes away with it, which is how a crash
/// ends up leaving no account of itself anywhere the reader can look.
fn restore_terminal_on_panic() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal();
        previous(info);
    }));
}

fn run(app: &mut App) -> Result<()> {
    restore_terminal_on_panic();
    enable_raw_mode()?;
    let mut out = stdout();
    execute!(out, EnterAlternateScreen, EnableMouseCapture)?;
    let _guard = TerminalGuard;
    let mut terminal = Terminal::new(CrosstermBackend::new(out))?;

    let result = event_loop(&mut terminal, app);

    // The guard does the rest as it goes out of scope, on this path and on
    // every other one.
    let _ = terminal.show_cursor();
    result
}

fn event_loop<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()> {
    let mut pending = String::new();
    loop {
        terminal.draw(|f| ui::draw(f, app))?;
        if !event::poll(Duration::from_millis(250))? {
            continue;
        }
        match event::read()? {
            Event::Key(key) if key.kind != KeyEventKind::Release => {
                handle_key(app, key, &mut pending);
            }
            Event::Mouse(m) => match m.kind {
                MouseEventKind::ScrollDown => app.scroll(3),
                MouseEventKind::ScrollUp => app.scroll(-3),
                _ => {}
            },
            _ => {}
        }
        if app.should_quit {
            return Ok(());
        }
    }
}

/// Translate a key press into a state change. `pending` holds a numeric prefix.
fn handle_key(app: &mut App, key: KeyEvent, pending: &mut String) {
    if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c')) {
        app.should_quit = true;
        return;
    }

    if app.mode == Mode::Search {
        match key.code {
            KeyCode::Esc => {
                app.query.clear();
                app.matches.clear();
                app.mode = Mode::Reading;
            }
            KeyCode::Enter => {
                app.mode = Mode::Reading;
                app.submit_search();
            }
            KeyCode::Backspace => {
                app.query.pop();
            }
            KeyCode::Char(c) => app.query.push(c),
            _ => {}
        }
        return;
    }

    if matches!(app.mode, Mode::Contents | Mode::Bookmarks) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => app.mode = Mode::Reading,
            KeyCode::Down | KeyCode::Char('j') => app.move_cursor(1),
            KeyCode::Up | KeyCode::Char('k') => app.move_cursor(-1),
            KeyCode::Home | KeyCode::Char('g') => app.list_cursor = 0,
            KeyCode::End | KeyCode::Char('G') => {
                app.list_cursor = app.list_len().saturating_sub(1)
            }
            KeyCode::Enter => app.activate(),
            KeyCode::Char('d') if app.mode == Mode::Bookmarks => {
                let i = app.list_cursor;
                app.delete_bookmark(i);
            }
            _ => {}
        }
        return;
    }

    if app.mode == Mode::Help {
        app.mode = Mode::Reading;
        return;
    }

    app.status = None;
    match key.code {
        KeyCode::Char(c @ '0'..='9') => {
            pending.push(c);
            app.status = Some(format!("{pending}% …press % to jump"));
            return;
        }
        KeyCode::Char('%') => {
            let pct: usize = pending.parse().unwrap_or(0);
            pending.clear();
            app.go_percent(pct);
            app.status = Some(format!("Jumped to {pct}%"));
            return;
        }
        _ => pending.clear(),
    }

    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
        KeyCode::Right | KeyCode::Char('l') | KeyCode::Char(' ') | KeyCode::PageDown => {
            app.next_page()
        }
        KeyCode::Left | KeyCode::Char('h') | KeyCode::PageUp | KeyCode::Backspace => {
            app.prev_page()
        }
        KeyCode::Down => app.scroll(1),
        KeyCode::Up => app.scroll(-1),
        KeyCode::Char('j') => app.scroll(1),
        KeyCode::Char('k') => app.scroll(-1),
        KeyCode::Char('g') | KeyCode::Home => app.go_start(),
        KeyCode::Char('G') | KeyCode::End => app.go_end(),
        KeyCode::Char('/') => {
            app.query.clear();
            app.mode = Mode::Search;
        }
        KeyCode::Char('n') => app.next_match(),
        KeyCode::Char('N') => app.prev_match(),
        KeyCode::Char('b') => app.toggle_bookmark(),
        KeyCode::Char('m') => app.open(Mode::Bookmarks),
        KeyCode::Char('c') => app.open(Mode::Contents),
        KeyCode::Char('t') => {
            app.theme = app.theme.next();
            app.status = Some(format!("Theme: {}", app.theme.name()));
        }
        KeyCode::Char('s') => app.toggle_two_page(),
        KeyCode::Char('i') => app.toggle_indent(),
        KeyCode::Char('+') | KeyCode::Char('=') => app.widen(4),
        KeyCode::Char('-') | KeyCode::Char('_') => app.widen(-4),
        KeyCode::Char('?') => app.open(Mode::Help),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(app: &mut App, c: char) {
        let mut pending = String::new();
        handle_key(
            app,
            KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE),
            &mut pending,
        );
    }

    fn book() -> App {
        let doc = Document::from_string(
            (0..300).map(|i| format!("line {i} of text\n")).collect(),
            &PathBuf::from("mem.txt"),
            "UTF-8",
        );
        let mut app = App::new(doc, Library::default());
        app.relayout(60, 10);
        app
    }

    #[test]
    fn space_turns_the_page_and_h_turns_back() {
        let mut app = book();
        press(&mut app, ' ');
        assert!(app.top > 0);
        press(&mut app, 'h');
        assert_eq!(app.top, 0);
    }

    #[test]
    fn q_quits() {
        let mut app = book();
        press(&mut app, 'q');
        assert!(app.should_quit);
    }

    #[test]
    fn slash_opens_search_and_enter_runs_it() {
        let mut app = book();
        let mut pending = String::new();
        press(&mut app, '/');
        assert_eq!(app.mode, Mode::Search);
        for c in "line 120".chars() {
            handle_key(
                &mut app,
                KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE),
                &mut pending,
            );
        }
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &mut pending,
        );
        assert_eq!(app.mode, Mode::Reading);
        assert_eq!(app.matches.len(), 1);
        assert!(app.top > 0);
    }

    #[test]
    fn escape_cancels_a_search_without_moving() {
        let mut app = book();
        press(&mut app, '/');
        press(&mut app, 'x');
        let mut pending = String::new();
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
            &mut pending,
        );
        assert_eq!(app.mode, Mode::Reading);
        assert!(app.query.is_empty());
        assert_eq!(app.top, 0);
    }

    #[test]
    fn digits_then_percent_jump() {
        let mut app = book();
        let mut pending = String::new();
        for c in "50".chars() {
            handle_key(
                &mut app,
                KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE),
                &mut pending,
            );
        }
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('%'), KeyModifiers::NONE),
            &mut pending,
        );
        assert!(app.top > 0 && app.top < app.max_top());
        assert!(pending.is_empty());
    }

    #[test]
    fn j_in_the_reader_scrolls_but_j_in_a_list_moves_the_cursor() {
        let mut app = book();
        press(&mut app, 'j');
        assert_eq!(app.top, 1);
        press(&mut app, 'b'); // bookmark, so the list is not empty
        press(&mut app, 'b'); // and a second one after moving
        app.next_page();
        press(&mut app, 'b');
        press(&mut app, 'm');
        assert_eq!(app.mode, Mode::Bookmarks);
        let before = app.top;
        press(&mut app, 'j');
        assert_eq!(app.top, before, "list navigation must not scroll the text");
    }

    #[test]
    fn ctrl_c_quits_from_any_mode() {
        let mut app = book();
        app.mode = Mode::Search;
        let mut pending = String::new();
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
            &mut pending,
        );
        assert!(app.should_quit);
    }

    #[test]
    fn help_closes_on_any_key() {
        let mut app = book();
        press(&mut app, '?');
        assert_eq!(app.mode, Mode::Help);
        press(&mut app, 'x');
        assert_eq!(app.mode, Mode::Reading);
    }

    #[test]
    fn theme_cycles_back_to_the_start() {
        let mut app = book();
        let start = app.theme;
        for _ in 0..4 {
            press(&mut app, 't');
        }
        assert_eq!(app.theme, start);
    }

    /// The listing must be in the order its own heading claims.
    ///
    /// Named so the two orders disagree: alphabetically by path the oldest
    /// book comes first, by when it was read it comes last. A listing that
    /// walks the map instead of `recent()` gets this exactly backwards.
    #[test]
    fn the_recent_listing_puts_the_last_book_read_first() {
        let mut lib = Library::default();
        {
            let old = lib.record("a-read-long-ago.txt");
            old.title = "Long ago".into();
            old.last_opened = 100;
        }
        {
            let new = lib.record("z-read-today.txt");
            new.title = "Today".into();
            new.last_opened = 300;
        }
        let lines = recent_lines(&lib);
        assert_eq!(lines.len(), 2);
        assert!(
            lines[0].contains("Today"),
            "the listing is in path order, not reading order: {lines:?}"
        );
        assert!(lines[1].contains("Long ago"), "{lines:?}");
    }
}

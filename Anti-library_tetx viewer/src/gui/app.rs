//! The reader window.
//!
//! Layout of the window, from the outside in: a slim title bar, an optional
//! left drawer (contents / bookmarks), a footer with the progress rule, and in
//! the middle the page itself — a sheet of paper with a running head, a
//! justified text block and a folio.

use crate::gui::layout::{self, Metrics, RowKind};
use crate::gui::settings::{Settings, ViewMode, MAX_FONT, MIN_FONT};
use crate::gui::theme::{self, sans, serif, Palette, Skin};
use crate::library::{Bookmark, Highlight, Ink, Library};
use crate::text::Document;
use egui::{
    Align2, Color32, CornerRadius, Context, CursorIcon, FontId, Key, Pos2, Rect, Sense, Stroke,
    StrokeKind, Vec2,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Speed of an auto-scroll, in points per second, for a pointer sitting `dy`
/// points from the anchor. Nothing happens inside the dead zone; past it the
/// speed grows with the distance, so the reader steers with small movements.
fn autoscroll_speed(dy: f32) -> f32 {
    if dy.abs() <= AUTOSCROLL_DEADZONE {
        return 0.0;
    }
    let past = dy.signum() * (dy.abs() - AUTOSCROLL_DEADZONE);
    (past * 3.0).clamp(-2400.0, 2400.0)
}

/// How far the pointer may move between press and release and still count as
/// a click rather than a drag.
const CLICK_SLOP: f32 = 6.0;
/// Pointer distance from the anchor that auto-scroll ignores, so the page
/// holds still when the hand is resting.
const AUTOSCROLL_DEADZONE: f32 = 18.0;

/// Wheel movement that adds up to one page turn. A mouse notch reports about
/// 50 points in egui, a trackpad far less per event.
const NOTCH: f32 = 48.0;

/// Type size of a drop cap, as a multiple of the body size. The layout and
/// the painter must agree on this or the text collides with the cap.
const DROP_CAP_SCALE: f32 = 2.9;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Drawer {
    Contents,
    Bookmarks,
    Highlights,
}

/// Where a row was painted, so the pointer can be turned back into a
/// character offset in the document.
#[derive(Debug, Clone)]
struct RowHit {
    row: usize,
    rect: Rect,
    /// Left edge of the text itself (past any indent or drop-cap well).
    x0: f32,
    font: FontId,
    /// Column width available to the row, for the justification maths.
    width: f32,
}

/// An in-progress or finished selection, in document character offsets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Selection {
    anchor: usize,
    cursor: usize,
}

impl Selection {
    fn range(&self) -> (usize, usize) {
        (self.anchor.min(self.cursor), self.anchor.max(self.cursor))
    }

    fn is_empty(&self) -> bool {
        self.anchor == self.cursor
    }
}

/// What the layout depends on. When it changes, the book is set again.
#[derive(Debug, Clone, Copy, PartialEq)]
struct LayoutKey {
    width: i32,
    height: i32,
    font: i32,
    leading: i32,
    justify: bool,
    drop_caps: bool,
    breaks: bool,
    hyphenate: bool,
}

pub struct ReaderApp {
    doc: Option<Document>,
    library: Library,
    settings: Settings,
    /// Names of the faces that were actually found at start-up. Shown in the
    /// reading settings, because which face is on the page is worth knowing on
    /// a machine that has none of the preferred ones.
    fonts: Vec<String>,
    skin: Skin,
    ts: layout::Typesetter,
    key: Option<LayoutKey>,
    /// Glyph widths, filled in as the typesetter reaches new text.
    body_w: HashMap<char, f32>,
    head_w: HashMap<char, f32>,
    /// Character offset the reader is waiting for while the book is still
    /// being set (restored progress can point past what is set so far).
    pending_anchor: Option<usize>,
    body_font: FontId,
    head_font: FontId,
    page: usize,
    /// Offset to restore after the next relayout.
    anchor: usize,
    drawer: Option<Drawer>,
    show_settings: bool,
    show_search: bool,
    search: String,
    /// Every occurrence of `search` in the document, in order. Character
    /// ranges, not row numbers: rows move when the column does.
    matches: Vec<crate::text::Match>,
    match_cursor: usize,
    focus_search: bool,
    toast: Option<(String, f64)>,
    /// Page-turn animation: −1 turning back, +1 turning forward, decaying to 0.
    turn: f32,
    /// The heading and the body of the notice on screen, if there is one.
    ///
    /// The heading used to be a constant in the window itself, so a failed save
    /// and a right-to-left book were both announced as a file that would not
    /// open. A notice whose heading contradicts its text is worse than none:
    /// the reader has to work out which half to believe.
    error: Option<(String, String)>,
    /// Rows painted this frame, used for pointer hit testing.
    hits: Vec<RowHit>,
    selection: Option<Selection>,
    selecting: bool,
    /// Colour the next highlight will use.
    ink: Ink,
    /// Only show highlights of this colour in the drawer.
    ink_filter: Option<Ink>,
    /// Scroll position, in points from the top of the text (Scroll mode).
    scroll: f32,
    /// Height of the text area, remembered from the last frame.
    rows_height: f32,
    /// Wheel movement not yet spent on turning a page.
    wheel_accum: f32,
    /// Movement banked by dragging or auto-scrolling, waiting to add up to a
    /// page. Kept apart from `wheel_accum`, which decays between notches —
    /// applying that decay here would eat a slow auto-scroll alive.
    pan_accum: f32,
    /// Anchor of the running auto-scroll, set by a click of the wheel.
    autoscroll: Option<Pos2>,
    /// Where the page was drawn last frame. The wheel is read from the raw
    /// input before any widget has claimed it, so this is the only way to ask
    /// whether it was rolled over the book or over something else.
    page_area: Rect,
    /// Where the wheel button went down, and how far it has moved since, so a
    /// click can be told from a drag.
    middle_drag: Option<(Pos2, f32)>,
    /// The highlight whose note is being written, and the text so far.
    note_editor: Option<(usize, String)>,
    /// What the book's marks looked like before each of the last few changes.
    ///
    /// Everything in the library is the reader's own work, and until now every
    /// way of changing it was one keystroke and no way back: `Delete` over a
    /// selection erased whatever was under it, and `0` did the same by mistake.
    undo: Vec<(String, crate::library::BookRecord)>,
    /// How tall the start-screen card has to be to hold what is on it.
    ///
    /// Measured at the end of the frame that drew it and used by the next one.
    /// A fixed height meant the shelf simply ran off the bottom of the card and
    /// carried on down the background.
    empty_card: f32,
    /// A book being read on another thread, and the name to show while it is.
    ///
    /// Opening used to happen inside the frame, which is fine for a text file
    /// and not fine for a PDF: extracting a few hundred pages takes seconds,
    /// and every one of them was a frozen, unpainted window that Windows
    /// eventually offered to close for the reader.
    loading: Option<(String, std::sync::mpsc::Receiver<Result<Document, String>>)>,
}

impl ReaderApp {
    /// The reader as the desktop app runs it, reading the settings and the
    /// library from this machine.
    pub fn new(ctx: &Context, book: Option<PathBuf>) -> ReaderApp {
        Self::with_state(ctx, book, Settings::load().sanitised(), Library::load())
    }

    /// The reader built on the settings and library it is handed.
    ///
    /// Everything the window does to its state — turning, searching, marking,
    /// remembering a place — is reachable from here without a window, and a
    /// caller that passes the defaults gets a reader that writes nothing: both
    /// `Settings` and `Library` know they came from nowhere and save to nowhere.
    pub fn with_state(
        ctx: &Context,
        book: Option<PathBuf>,
        settings: Settings,
        library: Library,
    ) -> ReaderApp {
        let fonts = theme::install_fonts(ctx);
        let skin: Skin = settings.skin.into();
        theme::apply(ctx, skin);

        let mut app = ReaderApp {
            doc: None,
            library,
            fonts,
            settings,
            skin,
            ts: layout::Typesetter::new(layout::Setup {
                width: 400.0,
                height: 600.0,
                metrics: Metrics::default(),
                justify: true,
                drop_caps: true,
                chapter_breaks: true,
                hyphenate: true,
            }),
            key: None,
            body_w: HashMap::new(),
            head_w: HashMap::new(),
            pending_anchor: None,
            body_font: serif(18.0),
            head_font: serif(26.0),
            page: 0,
            anchor: 0,
            drawer: None,
            show_settings: false,
            show_search: false,
            search: String::new(),
            matches: Vec::new(),
            match_cursor: 0,
            focus_search: false,
            toast: None,
            turn: 0.0,
            error: None,
            hits: Vec::new(),
            selection: None,
            selecting: false,
            ink: Ink::Yellow,
            ink_filter: None,
            scroll: 0.0,
            rows_height: 600.0,
            wheel_accum: 0.0,
            pan_accum: 0.0,
            autoscroll: None,
            page_area: Rect::NOTHING,
            middle_drag: None,
            note_editor: None,
            undo: Vec::new(),
            empty_card: 360.0,
            loading: None,
        };
        // If the reader closed itself last time, say so — quietly, once, and
        // with the file to look in. A program that vanishes and never mentions
        // it again is a program the reader stops trusting.
        if let Some(last) = crate::crash::last() {
            let where_to_look = crate::crash::log_path()
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            app.error = Some((
                "The reader closed unexpectedly last time".into(),
                format!("{last}\n\nThe record is at {where_to_look}"),
            ));
            crate::crash::clear();
        }

        // A library that could not be read has been set aside rather than
        // written over. Say where it went: the marks in it are the reader's
        // own work, and a file they can point somebody at beats an apology.
        if let Some(kept) = app.library.damaged_store().map(|p| p.display().to_string()) {
            app.error = Some((
                // Short enough to survive the title bar: the window sizes
                // itself to the body, and a longer heading is simply cut off
                // mid-word — which is how "not lost" became "not l…".
                "Your library was set aside".into(),
                format!(
                    "It could not be read, so it has been kept as\n\n{kept}\n\n\
                     and this session has started a new one. Nothing was deleted."
                ),
            ));
        } else if app.library.is_sealed() {
            app.error = Some((
                "Nothing will be saved this session".into(),
                "Your library file cannot be read and could not be moved aside, so \
                 bookmarks and highlights made now will not be kept — writing over it \
                 is the one thing that cannot be undone. Move the file yourself to \
                 start a fresh one."
                    .into(),
            ));
        }

        match book {
            // Named on the command line, or handed over by the file
            // association. If it is not there, the reader is owed the reason:
            // this used to fall through to the start screen without a word,
            // which looks exactly like the program ignoring the double-click.
            Some(p) => app.open(&p),
            None => {
                if let Some(p) = app.settings.last_book.as_ref().map(PathBuf::from) {
                    // Not named by anyone — merely where we left off. A book
                    // that has since been moved is not worth an error.
                    if p.is_file() {
                        app.open(&p);
                    }
                }
            }
        }
        app
    }

    // ---- book handling --------------------------------------------------

    /// Start reading a file. The work happens on another thread; the window
    /// keeps painting and [`Self::poll_loading`] takes delivery.
    fn open(&mut self, path: &Path) {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string());
        let (tx, rx) = std::sync::mpsc::channel();
        let owned = path.to_path_buf();
        // A failed spawn is not a reason to refuse the book: fall back to
        // reading it here, exactly as the reader used to.
        match std::thread::Builder::new()
            .name("open-book".into())
            .spawn(move || {
                let _ = tx.send(Document::load(&owned).map_err(|e| format!("{e}")));
            }) {
            Ok(_) => {
                self.error = None;
                self.loading = Some((name, rx));
            }
            Err(_) => match Document::load(path) {
                Ok(doc) => self.receive(doc),
                Err(e) => self.fail_to_open(format!("{e}")),
            },
        }
    }

    /// Take delivery of a book that finished loading.
    fn poll_loading(&mut self, ctx: &Context) {
        let Some((_, rx)) = &self.loading else { return };
        match rx.try_recv() {
            Ok(Ok(doc)) => {
                self.loading = None;
                self.receive(doc);
            }
            Ok(Err(e)) => {
                self.loading = None;
                self.fail_to_open(e);
            }
            // The thread died without answering — never seen, but silence here
            // would leave the window saying "Opening…" for ever.
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.loading = None;
                self.fail_to_open("the reader could not finish opening that file".into());
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(60));
            }
        }
    }

    /// Settle a freshly read book into the reader.
    fn receive(&mut self, doc: Document) {
        {
            {
                self.anchor = self.library.get(&doc.path).map(|r| r.offset).unwrap_or(0);
                self.settings.last_book = Some(doc.path.clone());
                self.settings.save();
                self.toast(format!("Opened {}", doc.title));
                // Stamp it now so the start screen can list books by when they
                // were last read rather than by where they happen to sit on disk.
                let (key, title) = (doc.path.clone(), doc.title.clone());
                let rec = self.library.record(&key);
                rec.last_opened = crate::library::now();
                if rec.title.is_empty() {
                    rec.title = title;
                }
                self.doc = Some(doc);
                self.realign_highlights();
                self.save_library();
                self.key = None;
                self.error = None;
                if self.doc.as_ref().is_some_and(|d| d.rtl) {
                    self.error = Some((
                        "This book reads right to left".into(),
                        "This reader cannot set that yet — the words would be drawn in \
the wrong order. The book is open, and searching and copying work, but do not \
trust what you see on the page."
                            .into(),
                    ));
                }
                // The old book's hits mean nothing here; if the reader left a
                // query in the box, answer it against the book now in hand.
                self.run_search(false);
            }
        }
    }

    fn pick_file(&mut self) {
        let mut dialog = rfd::FileDialog::new()
            .add_filter("Documents", crate::import::Format::EXTENSIONS)
            .add_filter("Text", &["txt", "md", "log", "text"])
            .add_filter("Word", &["docx"])
            .add_filter("PDF", &["pdf"])
            .add_filter("EPUB", &["epub"])
            .add_filter("All files", &["*"]);
        if let Some(dir) = self
            .settings
            .last_book
            .as_ref()
            .and_then(|p| Path::new(p).parent().map(Path::to_path_buf))
        {
            dialog = dialog.set_directory(dir);
        }
        if let Some(path) = dialog.pick_file() {
            self.save_progress();
            self.open(&path);
        }
    }

    fn save_progress(&mut self) {
        let Some(doc) = &self.doc else { return };
        let offset = self.visible_offset();
        let (key, title) = (doc.path.clone(), doc.title.clone());
        let rec = self.library.record(&key);
        rec.offset = offset;
        rec.title = title;
        self.save_library();
    }

    /// Character offset of the first thing the reader can see.
    fn visible_offset(&self) -> usize {
        if self.scrolling() {
            self.ts.layout
                .rows
                .get(self.ts.layout.row_at_height(self.scroll))
                .map(|r| r.offset)
                .unwrap_or(0)
        } else {
            self.ts.layout.offset_of_page(self.page)
        }
    }

    /// Say that a document would not open. The commonest notice, and the only
    /// one the old fixed heading was ever right about.
    fn fail_to_open(&mut self, why: String) {
        self.error = Some(("Could not open that file".into(), why));
    }

    /// Write the library out, telling the reader if it could not be saved —
    /// a lost highlight that reports nothing is worse than one that complains.
    fn save_library(&mut self) {
        if let Err(e) = self.library.save() {
            self.error = Some((
                "Your marks could not be saved".into(),
                format!("{e}"),
            ));
        }
    }

    /// Write the whole shelf somewhere the reader chooses.
    fn back_up_library(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Library backup", &["json"])
            .set_file_name("anti-library shelf.json")
            .save_file()
        else {
            return;
        };
        match self.library.export_to(&path) {
            Ok(()) => {
                let n = self.library.books.len();
                self.toast(format!("Backed up {n} book(s)"));
            }
            Err(e) => {
                self.error = Some(("Could not write the backup".into(), format!("{e}")));
            }
        }
    }

    /// Fold a backup back in, and say what actually came back.
    fn restore_library(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Library backup", &["json"])
            .pick_file()
        else {
            return;
        };
        match self.library.import_from(&path) {
            Ok(report) => {
                self.save_library();
                // Marks restored onto the book in hand may sit at offsets from
                // a copy of the file that has since been edited.
                self.realign_highlights();
                self.toast(format!("{report}"));
            }
            Err(e) => {
                self.error = Some(("Could not read that backup".into(), format!("{e:#}")));
            }
        }
    }

    fn write_diagnostics(&mut self) {
        let text = crate::diagnostics::report(&self.library);
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Text", &["txt"])
            .set_file_name("anti-library diagnostics.txt")
            .save_file()
        else {
            return;
        };
        match std::fs::write(&path, text) {
            Ok(()) => self.toast("Saved. Have a look at it before sending it on"),
            Err(e) => {
                self.error = Some(("Could not write that file".into(), format!("{e}")));
            }
        }
    }

    /// Open the release page, if this build has been told where one is.
    ///
    /// The reader does not fetch anything itself — it has no business on the
    /// network, and a program that quietly calls home is not one you hand
    /// somebody's reading to.
    fn open_release_page(&mut self) {
        let Some(url) = self.settings.updates_url.clone() else {
            return;
        };
        // Checked again here, not only when the settings were read: this is
        // the line that hands a string to the shell.
        if !crate::gui::settings::is_web_url(&url) {
            self.error = Some((
                "That release page is not a web address".into(),
                format!("{url}\n\nOnly http and https addresses are opened."),
            ));
            return;
        }
        let opened = std::process::Command::new("rundll32.exe")
            .args(["url.dll,FileProtocolHandler", &url])
            .spawn();
        match opened {
            Ok(_) => self.toast("Opened the release page in your browser"),
            Err(e) => {
                self.error = Some(("Could not open the release page".into(), format!("{e}")));
            }
        }
    }

    /// Keep the book's marks as they stand, so the next change can be undone.
    fn remember(&mut self) {
        let Some(doc) = &self.doc else { return };
        let key = doc.path.clone();
        let before = self.library.get(&key).cloned().unwrap_or_default();
        self.undo.push((key, before));
        // Deep enough to cover a session's mistakes, shallow enough that a
        // book with thousands of marks does not sit in memory forty times over.
        if self.undo.len() > 40 {
            self.undo.remove(0);
        }
    }

    /// Put the marks back the way they were before the last change.
    fn undo(&mut self) {
        let Some((key, before)) = self.undo.pop() else {
            self.toast("Nothing to undo");
            return;
        };
        let here = self.doc.as_ref().map(|d| d.path.clone());
        if here.as_deref() != Some(key.as_str()) {
            // The change was made in another book. Undoing it silently while
            // this one is open would look like nothing happened.
            self.toast("That change belongs to another book");
            return;
        }
        *self.library.record(&key) = before;
        self.save_library();
        self.selection = None;
        self.toast("Undone");
    }

    fn toast(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), 2.6));
    }

    // ---- pagination -----------------------------------------------------

    fn columns(&self) -> usize {
        self.settings.mode.columns()
    }

    fn scrolling(&self) -> bool {
        self.settings.mode == ViewMode::Scroll
    }

    /// Height of the visible text area in Scroll mode; set while painting.
    fn viewport(&self) -> f32 {
        self.rows_height.max(1.0)
    }

    fn max_scroll(&self) -> f32 {
        (self.ts.layout.total_height() - self.viewport()).max(0.0)
    }

    fn scroll_by(&mut self, delta: f32) {
        self.scroll = (self.scroll + delta).clamp(0.0, self.max_scroll());
        self.anchor = self
            .ts
            .layout
            .rows
            .get(self.ts.layout.row_at_height(self.scroll))
            .map(|r| r.offset)
            .unwrap_or(0);
    }

    /// First row the reader can see.
    fn top_row(&self) -> usize {
        visible_rows(&self.ts.layout, self.view()).0
    }

    /// The rows the reader can see, end exclusive.
    fn visible_rows(&self) -> (usize, usize) {
        visible_rows(&self.ts.layout, self.view())
    }

    /// Everything [`visible_rows`] needs to know about where the reader is.
    fn view(&self) -> View {
        View {
            scrolling: self.scrolling(),
            scroll: self.scroll,
            viewport: self.viewport(),
            page: self.page,
            columns: self.columns(),
        }
    }

    fn relayout(&mut self, ctx: &Context, col_width: f32, col_height: f32) {
        if self.doc.is_none() {
            return;
        }
        let s = &self.settings;
        // In Scroll mode the text is one endless page, so the column height
        // does not enter the layout key.
        let col_height = if self.scrolling() { 1.0e9 } else { col_height };
        let key = LayoutKey {
            width: (col_width * 4.0) as i32,
            height: (col_height * 4.0) as i32,
            font: (s.font_size * 4.0) as i32,
            leading: (s.line_height * 100.0) as i32,
            justify: s.justify,
            drop_caps: s.drop_caps,
            breaks: s.chapter_breaks,
            hyphenate: s.hyphenate,
        };
        if self.key == Some(key) {
            return;
        }

        let body = serif(s.font_size);
        let heading = serif(s.font_size * 1.45);
        let metrics = Metrics {
            body_height: s.font_size * s.line_height,
            heading_height: s.font_size * s.line_height * 1.9,
            blank_height: s.font_size * s.line_height * 0.5,
            indent: s.font_size * 1.4,
            drop_cap_rows: if s.drop_caps { 3 } else { 0 },
            drop_cap_scale: DROP_CAP_SCALE,
            drop_cap_gap: s.font_size * 0.34,
        };

        let setup = layout::Setup {
            width: col_width,
            height: col_height,
            metrics,
            justify: s.justify,
            drop_caps: s.drop_caps,
            chapter_breaks: s.chapter_breaks && s.mode != ViewMode::Scroll,
            hyphenate: s.hyphenate,
        };
        self.body_font = body;
        self.head_font = heading;
        self.body_w.clear();
        self.head_w.clear();
        self.ts = layout::Typesetter::new(setup);
        self.pending_anchor = Some(self.anchor);
        self.key = Some(key);

        // Set enough of the book to fill the first screen (and to reach the
        // place the reader stopped at) before this frame is drawn. The rest
        // catches up while the reader is on the first pages.
        self.pump(ctx, std::time::Duration::from_millis(90));
        self.settle_anchor();
    }

    /// Set more of the book, for at most `budget`. Cheap to call every frame.
    fn pump(&mut self, ctx: &Context, budget: std::time::Duration) {
        if self.ts.done {
            return;
        }
        // Move the document out for the duration of the pump: cloning it would
        // copy every paragraph of the book, every frame.
        let Some(doc) = self.doc.take() else { return };
        const CHUNK: usize = 256;
        let start = std::time::Instant::now();
        loop {
            // Only the glyphs the next chunk actually uses get measured, so a
            // step costs what the step is worth and no more.
            let from = self.ts.next_paragraph();
            let to = (from + CHUNK).min(doc.paragraphs.len());
            if to > from {
                let (body, head) = (self.body_font.clone(), self.head_font.clone());
                let (bw, hw) = (&mut self.body_w, &mut self.head_w);
                ctx.fonts_mut(|f| {
                    for p in &doc.paragraphs[from..to] {
                        for c in p.text.chars() {
                            // Zero for a combining mark, so the cached widths
                            // say the same thing `measure` does.
                            let zero = crate::text::is_combining(c);
                            bw.entry(c)
                                .or_insert_with(|| if zero { 0.0 } else { f.glyph_width(&body, c) });
                            hw.entry(c)
                                .or_insert_with(|| if zero { 0.0 } else { f.glyph_width(&head, c) });
                        }
                    }
                    bw.entry(' ').or_insert_with(|| f.glyph_width(&body, ' '));
                });
            }
            let done = {
                let (bw, hw) = (&self.body_w, &self.head_w);
                let m = |t: &str| -> f32 { t.chars().filter_map(|c| bw.get(&c)).sum() };
                let mh = |t: &str| -> f32 { t.chars().filter_map(|c| hw.get(&c)).sum() };
                self.ts.step(&doc, CHUNK, &m, &mh)
            };
            if done || start.elapsed() >= budget {
                break;
            }
            // Stop early once the reader's place is set and a screen is ready.
            if let Some(anchor) = self.pending_anchor {
                if self.ts.set_upto(&doc) > anchor
                    && self.ts.layout.pages.len() > self.columns() + 1
                {
                    break;
                }
            }
        }
        self.doc = Some(doc);
        if !self.ts.done {
            ctx.request_repaint();
        }
    }

    /// Set the rest of the book now. Used when the reader asks for something
    /// that only makes sense against the whole text, such as its last page.
    fn finish_typesetting(&mut self, ctx: &Context) {
        if self.ts.done {
            return;
        }
        self.toast("Setting the rest of the book\u{2026}");
        while !self.ts.done {
            self.pump(ctx, std::time::Duration::from_millis(50));
        }
    }

    /// Once the anchor is inside the text that has been set, go to it.
    fn settle_anchor(&mut self) {
        let Some(anchor) = self.pending_anchor else {
            return;
        };
        let Some(doc) = self.doc.as_ref() else { return };
        if !self.ts.done && self.ts.set_upto(doc) <= anchor {
            return; // not there yet; try again next frame
        }
        self.pending_anchor = None;
        self.page = self.ts.layout.page_of_offset(anchor);
        self.align_page();
        if self.scrolling() {
            let row = self.ts.layout.row_of_offset(anchor);
            self.scroll = self
                .ts
                .layout
                .tops
                .get(row)
                .copied()
                .unwrap_or(0.0)
                .min(self.max_scroll());
        }
    }

    /// With a two page spread the left leaf is always an even page.
    fn align_page(&mut self) {
        let step = self.columns();
        self.page = self.page / step * step;
        self.page = self.page.min(self.last_page());
    }

    fn last_page(&self) -> usize {
        let step = self.columns();
        let n = self.ts.layout.pages.len().saturating_sub(1);
        n / step * step
    }

    fn turn_page(&mut self, dir: isize) {
        if self.scrolling() {
            // A "page" here is a screenful, less two lines of overlap so the
            // eye can pick up where it left off.
            let step = self.viewport() - self.settings.font_size * self.settings.line_height * 2.0;
            let before = self.scroll;
            self.scroll_by(step * dir as f32);
            if (self.scroll - before).abs() < 0.5 {
                self.toast(if dir > 0 {
                    "End of the book"
                } else {
                    "Beginning of the book"
                });
            }
            self.save_progress();
            return;
        }
        let step = self.columns() as isize;
        let target = self.page as isize + dir * step;
        let last = self.last_page() as isize;
        let clamped = target.clamp(0, last) as usize;
        if clamped == self.page {
            self.toast(if dir > 0 {
                "End of the book"
            } else {
                "Beginning of the book"
            });
            return;
        }
        self.page = clamped;
        if !self.selecting {
            // A page turned in the middle of a drag is part of that drag.
            self.selection = None;
        }
        self.anchor = self.ts.layout.offset_of_page(self.page);
        if self.settings.page_animation {
            self.turn = if dir > 0 { 1.0 } else { -1.0 };
        }
        self.save_progress();
    }

    fn goto_page(&mut self, page: usize) {
        self.selection = None;
        self.pan_accum = 0.0;
        self.page = page.min(self.ts.layout.pages.len().saturating_sub(1));
        self.align_page();
        self.anchor = self.ts.layout.offset_of_page(self.page);
        self.save_progress();
    }

    /// Jump to a fraction of the book, whatever the mode.
    ///
    /// The end of a book that is still being set is not where the last page
    /// happens to be right now, so the caller finishes the typesetting first
    /// (see [`Self::finish_typesetting`]).
    fn goto_fraction(&mut self, t: f32) {
        let t = t.clamp(0.0, 1.0);
        if self.scrolling() {
            self.selection = None;
            self.scroll = 0.0;
            self.scroll_by(self.max_scroll() * t);
            self.save_progress();
        } else {
            let target = (t * (self.ts.layout.pages.len().saturating_sub(1)) as f32).round() as usize;
            self.goto_page(target);
        }
    }

    fn goto_offset(&mut self, offset: usize) {
        self.anchor = offset;
        if self.scrolling() {
            self.selection = None;
            let row = self.ts.layout.row_of_offset(offset);
            self.scroll = self
                .ts
                .layout
                .tops
                .get(row)
                .copied()
                .unwrap_or(0.0)
                .min(self.max_scroll());
            self.save_progress();
        } else {
            self.goto_page(self.ts.layout.page_of_offset(offset));
        }
    }

    fn progress(&self) -> f32 {
        // While the book is still being set the page count keeps growing, so
        // measure progress against the document itself — it does not move.
        if !self.ts.done {
            if let Some(doc) = &self.doc {
                if doc.chars > 0 {
                    return (self.visible_offset() as f32 / doc.chars as f32).clamp(0.0, 1.0);
                }
            }
        }
        if self.scrolling() {
            let max = self.max_scroll();
            return if max <= 0.0 {
                1.0
            } else {
                (self.scroll / max).clamp(0.0, 1.0)
            };
        }
        let last = self.last_page();
        if last == 0 {
            return 1.0;
        }
        (self.page as f32 / last as f32).clamp(0.0, 1.0)
    }

    /// Where a row sits on the progress rule. See [`fraction_of_row`].
    fn fraction_of_row(&self, row: usize) -> f32 {
        fraction_of_row(
            &self.ts.layout,
            self.view(),
            self.ts.done,
            self.doc.as_ref().map(|d| d.chars).unwrap_or(0),
            row,
        )
    }

    fn chapter_here(&self) -> Option<String> {
        let doc = self.doc.as_ref()?;
        let idx = self.chapter_index_here()?;
        doc.chapters.get(idx).map(|c| c.title.clone())
    }

    /// Index of the chapter the reader is inside, whatever the view mode.
    fn chapter_index_here(&self) -> Option<usize> {
        self.ts.layout.rows.get(self.top_row())?.chapter
    }

    // ---- search ---------------------------------------------------------

    /// Find every occurrence in the *document*.
    ///
    /// This used to scan the typeset rows, and so answered a different question
    /// than the reader asked: a row is where the column broke the text, not
    /// where the words are. A phrase split across a line break was reported as
    /// no match at all, and Korean — which wraps between glyphs — lost whole
    /// words that way. Searching the paragraphs makes the answer independent of
    /// the column width, of the type size, and of how much of the book has been
    /// set so far, which is why the search no longer has to be re-run as the
    /// typesetter catches up.
    fn run_search(&mut self, jump: bool) {
        self.matches = match &self.doc {
            Some(doc) => doc.search(&self.search),
            None => Vec::new(),
        };
        self.match_cursor = 0;
        if !jump {
            return;
        }
        if self.matches.is_empty() {
            self.toast(format!("No match for \u{201c}{}\u{201d}", self.search));
            return;
        }
        // Start from the hit nearest to what is on screen.
        let here = self.visible_offset();
        self.match_cursor = self
            .matches
            .iter()
            .position(|m| m.start >= here)
            .unwrap_or(0);
        self.jump_to_match();
    }

    /// Put the search away, query and hits and all.
    fn close_search(&mut self) {
        self.show_search = false;
        self.search.clear();
        self.matches.clear();
    }

    fn step_match(&mut self, dir: isize) {
        if self.matches.is_empty() {
            return;
        }
        let n = self.matches.len() as isize;
        self.match_cursor = ((self.match_cursor as isize + dir + n) % n) as usize;
        self.jump_to_match();
    }

    fn jump_to_match(&mut self) {
        let Some(&m) = self.matches.get(self.match_cursor) else {
            return;
        };
        // The search sees the whole book, but only the part that has been set
        // can be shown. A hit past that point is not lost — it is the same
        // situation as a restored reading position that lies ahead of the
        // typesetter, so it takes the same road: wait at the anchor and settle
        // there the moment the text arrives.
        let unset = self
            .doc
            .as_ref()
            .is_some_and(|d| !self.ts.done && self.ts.set_upto(d) <= m.start);
        if unset {
            self.anchor = m.start;
            self.pending_anchor = Some(m.start);
        } else {
            self.goto_offset(m.start);
        }
        self.toast(format!(
            "{} of {} matches",
            self.match_cursor + 1,
            self.matches.len()
        ));
    }

    // ---- bookmarks ------------------------------------------------------

    fn bookmarks(&self) -> Vec<Bookmark> {
        self.doc
            .as_ref()
            .and_then(|d| self.library.get(&d.path))
            .map(|r| r.bookmarks.clone())
            .unwrap_or_default()
    }

    /// Is one of this book's bookmarks on the screen?
    ///
    /// Not "does a bookmark sit at exactly the top character" — it used to be,
    /// and so the mark went dark whenever the type size, the window or the view
    /// mode moved the row boundaries by a character. The reader can see a
    /// screenful; the button should answer for the screenful.
    fn bookmarked_here(&self) -> bool {
        let (start, end) = self.visible_span();
        self.bookmarks().iter().any(|b| b.offset >= start && b.offset < end.max(start + 1))
    }

    /// The character range the reader can see.
    fn visible_span(&self) -> (usize, usize) {
        let (first, last) = self.visible_rows();
        let rows = &self.ts.layout.rows;
        let start = rows.get(first).map(|r| r.offset).unwrap_or(0);
        let end = rows
            .get(last.saturating_sub(1))
            .map(|r| r.range().1)
            .unwrap_or(start);
        (start, end)
    }

    // ---- highlights -----------------------------------------------------

    fn highlights(&self) -> Vec<Highlight> {
        self.doc
            .as_ref()
            .and_then(|d| self.library.get(&d.path))
            .map(|r| r.highlights.clone())
            .unwrap_or_default()
    }

    /// Mark the selection. Marking over an existing highlight replaces it, so
    /// re-marking a passage in another colour does not leave two layers.
    fn mark_selection(&mut self, ink: Ink) {
        let Some(sel) = self.selection else { return };
        if sel.is_empty() {
            return;
        }
        self.remember();
        let (start, end) = sel.range();
        let text = layout::extract(&self.ts.layout.rows, start, end);
        let Some(doc) = &self.doc else { return };
        let key = doc.path.clone();
        let rec = self.library.record(&key);
        // Marking over a passage replaces it, but a note the reader wrote on it
        // is not ours to drop: carry it onto the new mark.
        let note = rec
            .highlights
            .iter()
            .filter(|h| h.overlaps(start, end) && !h.note.is_empty())
            .map(|h| h.note.clone())
            .collect::<Vec<_>>()
            .join("\n");
        rec.highlights.retain(|h| !h.overlaps(start, end));
        rec.highlights.push(Highlight {
            start,
            end,
            ink,
            text,
            note,
            stale: false,
        });
        rec.highlights.sort_by_key(|h| h.start);
        self.save_library();
        self.ink = ink;
        self.selection = None;
        self.toast(format!("Marked in {}", ink.name()));
    }

    /// Remove any highlight the selection (or, with none, the current page)
    /// touches.
    fn unmark_selection(&mut self) {
        let (start, end) = match self.selection {
            Some(sel) if !sel.is_empty() => sel.range(),
            _ => return,
        };
        self.remember();
        let Some(doc) = &self.doc else { return };
        let key = doc.path.clone();
        let rec = self.library.record(&key);
        let before = rec.highlights.len();
        rec.highlights.retain(|h| !h.overlaps(start, end));
        let removed = before - rec.highlights.len();
        self.save_library();
        self.selection = None;
        self.toast(if removed > 0 {
            format!("Removed {removed} highlight(s)")
        } else {
            "Nothing marked here".into()
        });
    }

    /// Re-anchor highlights whose offsets no longer point at the text they
    /// were made on — the file may have been edited since.
    ///
    /// The saved words are searched for near the old position first, so a
    /// paragraph inserted earlier in the book shifts the mark instead of
    /// losing it. What cannot be found is kept and marked stale rather than
    /// deleted: a highlight is the reader's work, not ours to throw away.
    fn realign_highlights(&mut self) {
        let Some(doc) = self.doc.as_ref() else { return };
        let key = doc.path.clone();
        let Some(rec) = self.library.books.get_mut(&key) else {
            return;
        };
        let mut moved = 0usize;
        let mut lost = 0usize;
        for h in rec.highlights.iter_mut() {
            if h.text.is_empty() {
                continue;
            }
            if doc.slice(h.start, h.end) == h.text {
                h.stale = false;
                continue;
            }
            // Search on the first paragraph of the mark: a multi-paragraph
            // selection is stored with blank lines between the pieces.
            let needle = h.text.split("\n\n").next().unwrap_or("").trim();
            if needle.chars().count() < 4 {
                h.stale = true;
                lost += 1;
                continue;
            }
            match doc.find_near(needle, h.start) {
                Some(at) => {
                    // Saturating because the file is not always ours: a store
                    // edited by hand, or half-written by a machine that went
                    // down, can hold a mark that ends before it starts, and
                    // `end - start` on that is a panic in the middle of
                    // opening a book.
                    let len = h.end.saturating_sub(h.start);
                    h.start = at;
                    h.end = (at + len).min(doc.chars);
                    h.stale = false;
                    moved += 1;
                }
                None => {
                    h.stale = true;
                    lost += 1;
                }
            }
        }
        if moved > 0 || lost > 0 {
            self.save_library();
            self.toast(match (moved, lost) {
                (m, 0) => format!("Moved {m} highlight(s) to their new place"),
                (0, l) => format!("{l} highlight(s) no longer match this file"),
                (m, l) => format!("Moved {m}, lost {l} highlight(s)"),
            });
        }
    }

    fn delete_highlight(&mut self, index: usize) {
        self.remember();
        let Some(doc) = &self.doc else { return };
        let key = doc.path.clone();
        let rec = self.library.record(&key);
        if index < rec.highlights.len() {
            rec.highlights.remove(index);
            self.save_library();
            self.toast("Highlight deleted");
        }
    }

    /// Select everything the reader can see — the leaves on screen in the paged
    /// modes, the rows inside the viewport when scrolling.
    fn select_visible_pages(&mut self) {
        let (start, end) = self.visible_rows();
        if end == 0 || end <= start {
            return;
        }
        // A page with nothing on it has nothing to select — and `end - 1`
        // would run off the front of the row list.
        let layout = &self.ts.layout;
        let (Some(from), Some(to)) = (layout.rows.get(start), layout.rows.get(end - 1)) else {
            return;
        };
        self.selection = Some(Selection {
            anchor: from.offset,
            cursor: to.range().1,
        });
    }

    /// The text on the leaves the reader can see, as prose.
    ///
    /// Written for the accessibility tree, so it is the words and nothing else:
    /// the rows are joined back into sentences the way a copied selection is.
    fn visible_text(&self) -> String {
        let (start, end) = self.visible_span();
        if end <= start {
            return String::new();
        }
        layout::extract(&self.ts.layout.rows, start, end)
    }

    fn selected_text(&self) -> String {
        match self.selection {
            Some(sel) if !sel.is_empty() => {
                let (a, b) = sel.range();
                layout::extract(&self.ts.layout.rows, a, b)
            }
            _ => String::new(),
        }
    }

    fn copy_selection(&mut self, ctx: &Context) {
        let text = self.selected_text();
        if text.is_empty() {
            self.toast("Nothing selected");
            return;
        }
        let chars = text.chars().count();
        ctx.copy_text(text);
        self.toast(format!("Copied {chars} characters"));
    }

    /// All highlights as Markdown, ready for a notebook.
    fn highlights_markdown(&self) -> String {
        let Some(doc) = &self.doc else {
            return String::new();
        };
        let mut out = format!("# {}\n\n", doc.title);
        for h in self.highlights() {
            let pct = if doc.chars == 0 {
                0
            } else {
                h.start * 100 / doc.chars
            };
            out.push_str(&format!(
                "- **{}** ({pct}%) — {}\n",
                h.ink.name(),
                h.text.trim()
            ));
            // A note is why the passage was marked. Leaving it out of the
            // export writes it into a file nobody ever reads again.
            for line in h.note.trim().lines() {
                out.push_str(&format!("  > {line}\n"));
            }
        }
        out
    }

    fn toggle_bookmark(&mut self) {
        if self.doc.is_none() {
            return;
        }
        self.remember();
        let Some(doc) = &self.doc else { return };
        let offset = self.visible_offset();
        // Label it with the first words the reader can actually see, so a mark
        // made while scrolling does not come back named after another page.
        let (start, end) = self.visible_rows();
        let label = self.ts.layout.rows[start.min(self.ts.layout.rows.len())..end.min(self.ts.layout.rows.len())]
            .iter()
            .find(|r| r.kind != RowKind::Blank && !r.text.trim().is_empty())
            .map(|r| r.text.trim().chars().take(56).collect::<String>())
            .filter(|s: &String| !s.is_empty())
            .unwrap_or_else(|| format!("At {offset}"));
        let key = doc.path.clone();
        let rec = self.library.record(&key);
        if let Some(i) = rec.bookmarks.iter().position(|b| b.offset == offset) {
            rec.bookmarks.remove(i);
            self.toast("Bookmark removed");
        } else {
            rec.bookmarks.push(Bookmark { offset, label });
            rec.bookmarks.sort_by_key(|b| b.offset);
            self.toast("Bookmark added");
        }
        self.save_library();
    }
}

// =========================================================================
// Painting
// =========================================================================

impl ReaderApp {
    fn palette(&self) -> Palette {
        self.skin.palette()
    }

    fn set_skin(&mut self, ctx: &Context, skin: Skin) {
        self.skin = skin;
        self.settings.skin = skin.into();
        self.settings.save();
        theme::apply(ctx, skin);
    }

    /// The page sheet: shadow, stock, edge, and a hairline at the spine.
    fn paint_sheet(&self, ui: &egui::Ui, rect: Rect) {
        let p = self.palette();
        let painter = ui.painter();
        let radius = CornerRadius::same(3);
        // A soft drop shadow, built from three offset rectangles.
        for (i, alpha) in [(6.0f32, 10u8), (3.0, 14), (1.5, 18)] {
            painter.rect_filled(
                rect.translate(Vec2::new(0.0, i * 0.6)).expand(i),
                radius,
                Color32::from_black_alpha(alpha),
            );
        }
        painter.rect_filled(rect, radius, p.page);
        painter.rect_stroke(rect, radius, Stroke::new(1.0, p.page_edge), StrokeKind::Inside);
    }

    /// Draw one page of text into `rect`. Returns nothing; all state is read.
    /// Draw one page of text into `rect`, recording where each row landed so
    /// the pointer can be mapped back onto the text.
    fn paint_page(
        &self,
        ui: &egui::Ui,
        rect: Rect,
        page: usize,
        alpha: f32,
        hits: &mut Vec<RowHit>,
    ) {
        let Some(doc) = &self.doc else { return };
        let Some(&pg) = self.ts.layout.pages.get(page) else {
            return;
        };
        let p = self.palette();
        // The running head and the folio sit above the text block, so the clip
        // rect must take in the margin they live in: painting straight into
        // `rect` swallowed them.
        let painter = ui.painter_at(rect.expand2(Vec2::new(6.0, 64.0)));
        let s = &self.settings;
        let fade = |c: Color32| c.gamma_multiply(alpha);

        // Running head: chapter title on the left leaf, book title on the right.
        let head_font = sans(s.font_size * 0.62);
        let head_text = self
            .ts
            .layout
            .rows
            .get(pg.start)
            .and_then(|r| r.chapter)
            .and_then(|i| doc.chapters.get(i))
            .map(|c| c.title.clone())
            .unwrap_or_else(|| doc.title.clone());
        painter.text(
            Pos2::new(rect.left(), rect.top() - s.font_size * 1.9),
            Align2::LEFT_TOP,
            head_text.to_uppercase(),
            head_font.clone(),
            fade(p.ink_faint),
        );
        painter.text(
            Pos2::new(rect.right(), rect.top() - s.font_size * 1.9),
            Align2::RIGHT_TOP,
            format!("{}", page + 1),
            head_font,
            fade(p.ink_faint),
        );
        painter.hline(
            rect.left()..=rect.right(),
            rect.top() - s.font_size * 0.75,
            Stroke::new(1.0, fade(p.hairline)),
        );

        self.paint_rows(ui, rect, pg.start, pg.end, rect.top(), alpha, hits);
    }

    /// Paint rows `first..last`, starting with the top of `first` at `y_start`.
    /// Rows outside `rect` are skipped, so a long book scrolls at the cost of
    /// one screenful.
    #[allow(clippy::too_many_arguments)]
    fn paint_rows(
        &self,
        ui: &egui::Ui,
        rect: Rect,
        first: usize,
        last: usize,
        y_start: f32,
        alpha: f32,
        hits: &mut Vec<RowHit>,
    ) {
        let p = self.palette();
        let painter = ui.painter_at(rect.expand2(Vec2::new(6.0, 2.0)));
        let s = &self.settings;
        let body = serif(s.font_size);
        let focus_y = rect.center().y;
        // Borrowed, not cloned: this runs for every page of every frame, and
        // a book with a few thousand marks in it would otherwise copy all of
        // them — strings and all — sixty times a second.
        let marks: &[Highlight] = self
            .doc
            .as_ref()
            .and_then(|d| self.library.get(&d.path))
            .map(|r| r.highlights.as_slice())
            .unwrap_or(&[]);
        let selection = self.selection.map(|sel| sel.range());
        let mut y = y_start;
        for (i, row) in self.ts.layout.rows[first..last.min(self.ts.layout.rows.len())]
            .iter()
            .enumerate()
        {
            let row_idx = first + i;
            let top = y;
            y += row.height;
            if y < rect.top() - row.height {
                continue;
            }
            if top > rect.bottom() {
                break;
            }
            // Focus mode fades everything but the line being read.
            let alpha = if s.focus {
                let d = ((top + row.height * 0.5) - focus_y).abs();
                let near = (1.0 - (d / (rect.height() * 0.28)).min(1.0)).powf(0.7);
                alpha * (0.34 + 0.66 * near)
            } else {
                alpha
            };
            let fade = |c: Color32| c.gamma_multiply(alpha);
            if row.kind == RowKind::Blank || row.text.is_empty() {
                continue;
            }
            let font = match row.kind {
                RowKind::Heading => serif(s.font_size * 1.45),
                _ => body.clone(),
            };
            let colour = match row.kind {
                RowKind::Heading => fade(p.accent),
                _ => fade(p.ink),
            };
            let baseline = top + (row.height - font.size) * 0.5;
            let x0 = rect.left() + row.indent;
            let width = rect.width() - row.indent;

            // A hyphenated row ends with a mark that is drawn but not stored,
            // and it occupies the measure like any other glyph. Justifying to
            // the full column without counting it pushes the hyphen past the
            // margin.
            let hyphen_w = if row.hyphen {
                measure(ui.ctx(), &font, "-")
            } else {
                0.0
            };
            let fill = if row.justify {
                let w = measure(ui.ctx(), &font, &row.text) + hyphen_w;
                layout::stretch(
                    &row.text,
                    w,
                    width,
                    s.font_size * 0.22,
                    s.font_size * 0.08,
                )
            } else {
                layout::Stretch::None
            };
            // One source of truth for where each character sits: the painter,
            // the highlighter and the pointer all read these positions.
            let xs = char_positions(ui.ctx(), &font, &row.text, fill);

            hits.push(RowHit {
                row: row_idx,
                rect: Rect::from_min_size(
                    Pos2::new(rect.left(), top),
                    Vec2::new(rect.width(), row.height),
                ),
                x0,
                font: font.clone(),
                width,
            });

            let band = |from: usize, to: usize| {
                Rect::from_min_max(
                    Pos2::new(x0 + xs[from] - 1.0, baseline - font.size * 0.16),
                    Pos2::new(x0 + xs[to] + 1.0, baseline + font.size * 1.18),
                )
            };

            // Marked passages, then the live selection, then search hits: all
            // under the text so the letters stay crisp.
            for h in highlights_touching(marks, row.range().0, row.range().1) {
                if h.stale {
                    continue;
                }
                if let Some((from, to)) = row.clip(h.start, h.end) {
                    painter.rect_filled(
                        band(from, to),
                        CornerRadius::same(2),
                        fade(theme::ink_colour(h.ink, self.skin.is_dark())),
                    );
                }
            }
            if let Some((a, b)) = selection {
                if let Some((from, to)) = row.clip(a, b) {
                    painter.rect_filled(
                        band(from, to),
                        CornerRadius::same(2),
                        fade(p.accent.gamma_multiply(0.22)),
                    );
                }
            }
            // Search hits. The matches are document ranges, so the row simply
            // clips them — no second, differently-spelled search on the row's
            // own text, which is what used to make the paint disagree with the
            // count in the search box (and skip any row whose lower case was a
            // different number of bytes, such as one holding `İ`).
            let (a, b) = row.range();
            for m in crate::text::matches_in(&self.matches, a, b) {
                let Some((from, to)) = row.clip(m.start, m.end) else {
                    continue;
                };
                // The hit the reader is standing on is drawn stronger than
                // the rest, so `3 of 57` points at something.
                let current = self
                    .matches
                    .get(self.match_cursor)
                    .is_some_and(|c| c == m);
                let wash = if current {
                    fade(p.accent.gamma_multiply(0.45))
                } else {
                    fade(p.highlight)
                };
                painter.rect_filled(band(from, to), CornerRadius::same(2), wash);
            }

            // A drop cap opens the chapter.
            if let Some(cap) = &row.drop_cap {
                let cap_font = serif(s.font_size * DROP_CAP_SCALE);
                painter.text(
                    Pos2::new(rect.left(), top - s.font_size * 0.35),
                    Align2::LEFT_TOP,
                    cap,
                    cap_font,
                    fade(p.accent),
                );
            }

            match fill {
                // Latin: place each word by hand so the right edge is flush.
                layout::Stretch::WordGaps(_) => {
                    let chars: Vec<char> = row.text.chars().collect();
                    let mut i = 0usize;
                    while i < chars.len() {
                        if chars[i] == ' ' {
                            i += 1;
                            continue;
                        }
                        let start = i;
                        while i < chars.len() && chars[i] != ' ' {
                            i += 1;
                        }
                        let word: String = chars[start..i].iter().collect();
                        painter.text(
                            Pos2::new(x0 + xs[start], baseline),
                            Align2::LEFT_TOP,
                            word,
                            font.clone(),
                            colour,
                        );
                    }
                }
                // Korean: open the same small gap between every glyph.
                layout::Stretch::Letters(_) => {
                    for (i, c) in row.text.chars().enumerate() {
                        painter.text(
                            Pos2::new(x0 + xs[i], baseline),
                            Align2::LEFT_TOP,
                            c,
                            font.clone(),
                            colour,
                        );
                    }
                }
                layout::Stretch::None => {
                    painter.text(
                        Pos2::new(x0, baseline),
                        Align2::LEFT_TOP,
                        &row.text,
                        font.clone(),
                        colour,
                    );
                }
            }

            // The hyphen sits at the end of the row, past its last character.
            if row.hyphen {
                painter.text(
                    Pos2::new(x0 + xs[xs.len() - 1], baseline),
                    Align2::LEFT_TOP,
                    "-",
                    font.clone(),
                    colour,
                );
            }

            if row.chapter_start {
                painter.hline(
                    rect.left()..=rect.left() + s.font_size * 2.2,
                    top + row.height * 0.92,
                    Stroke::new(2.0, fade(p.accent)),
                );
            }
        }
    }

    /// Is the pointer on the page itself?
    ///
    /// The wheel and the wheel button are taken straight from the raw input at
    /// the top of the frame, before the drawer or any window has had a chance
    /// to claim them. Without this the wheel did two jobs at once: rolling it
    /// over the highlight drawer scrolled that list *and* turned the leaf
    /// underneath it.
    fn pointer_on_the_page(&self, ctx: &Context) -> bool {
        let Some(pos) = ctx.input(|i| i.pointer.latest_pos()) else {
            return false;
        };
        if !self.page_area.contains(pos) {
            return false;
        }
        // A floating window — the reading settings, a note — sits above the
        // page and keeps whatever is rolled over it.
        ctx.layer_id_at(pos)
            .is_none_or(|l| l.order == egui::Order::Background)
    }

    /// Character offset under the pointer, or `None` when it is not over text.
    fn offset_at(&self, ctx: &Context, pos: Pos2) -> Option<usize> {
        // Nearest row vertically, so a drag that strays into a margin still
        // extends the selection instead of dropping it.
        let hit = self
            .hits
            .iter()
            .filter(|h| pos.x >= h.rect.left() - 24.0 && pos.x <= h.rect.right() + 24.0)
            .min_by(|a, b| {
                let d = |r: &Rect| (pos.y - r.center().y).abs();
                d(&a.rect).total_cmp(&d(&b.rect))
            })?;
        let row = self.ts.layout.rows.get(hit.row)?;
        let fill = if row.justify {
            let hyphen_w = if row.hyphen { measure(ctx, &hit.font, "-") } else { 0.0 };
            let w = measure(ctx, &hit.font, &row.text) + hyphen_w;
            layout::stretch(
                &row.text,
                w,
                hit.width,
                self.settings.font_size * 0.22,
                self.settings.font_size * 0.08,
            )
        } else {
            layout::Stretch::None
        };
        let xs = char_positions(ctx, &hit.font, &row.text, fill);
        let local = pos.x - hit.x0;
        // Snap to the nearest character boundary, the way a text cursor does.
        let mut best = 0usize;
        let mut best_d = f32::INFINITY;
        for (i, x) in xs.iter().enumerate() {
            let d = (local - x).abs();
            if d < best_d {
                best_d = d;
                best = i;
            }
        }
        Some(row.offset + best)
    }
}

/// Where the reader is looking, in the terms each view mode moves in.
#[derive(Debug, Clone, Copy, PartialEq)]
struct View {
    scrolling: bool,
    scroll: f32,
    viewport: f32,
    page: usize,
    columns: usize,
}

/// The rows on screen, end exclusive.
///
/// Scroll mode sets the whole book as one endless page, so `page` says nothing
/// there — the scroll position is the only thing that moves. Everything that
/// asks "where are we?" — the running head, the contents drawer, a new
/// bookmark's label, `Ctrl+A` — has to come through here, or it goes on
/// describing a leaf nobody is reading.
fn visible_rows(layout: &layout::Layout, view: View) -> (usize, usize) {
    if !view.scrolling {
        let first = layout.pages.get(view.page).map(|p| p.start).unwrap_or(0);
        let last = (view.page + view.columns.max(1) - 1).min(layout.pages.len().saturating_sub(1));
        let end = layout.pages.get(last).map(|p| p.end).unwrap_or(first);
        return (first, end.max(first));
    }
    let first = layout.row_at_height(view.scroll);
    let bottom = view.scroll + view.viewport;
    let mut end = first;
    while end < layout.rows.len() && layout.tops.get(end).copied().unwrap_or(f32::MAX) < bottom {
        end += 1;
    }
    (first, end.max(first))
}

/// Where a row sits on the progress rule, on the scale the knob is drawn on.
///
/// This is the number [`ReaderApp::progress`] reports with the reader standing
/// on that row, and it has to be: the knob and the chapter ticks are drawn on
/// the same rule, so a tick measured any other way points at a different book.
/// They used to be placed by `page_of_row(row) / pages.len()` — a third scale,
/// after the knob's and the one a drag on the rule uses. In Scroll mode that
/// was not merely off by a little: the whole book is set as one endless page
/// there, `page_of_row` answers 0 for every row in it, and so every chapter in
/// the book stacked up on the left end of the rule.
///
/// A free function, like [`visible_rows`] and for the same reason: the window
/// is not needed to answer it, and so a test does not need one either.
fn fraction_of_row(
    layout: &layout::Layout,
    view: View,
    done: bool,
    chars: usize,
    row: usize,
) -> f32 {
    // While the book is still being set the page count keeps growing, so
    // measure against the document — it does not move.
    if !done && chars > 0 {
        let at = layout.rows.get(row).map(|r| r.offset).unwrap_or(0);
        return (at as f32 / chars as f32).clamp(0.0, 1.0);
    }
    if view.scrolling {
        let max = (layout.total_height() - view.viewport).max(0.0);
        if max <= 0.0 {
            return 1.0;
        }
        let top = layout.tops.get(row).copied().unwrap_or(0.0);
        return (top / max).clamp(0.0, 1.0);
    }
    let step = view.columns.max(1);
    let last = layout.pages.len().saturating_sub(1) / step * step;
    if last == 0 {
        return 1.0;
    }
    // A spread turns two leaves at once, so a chapter opening on the right
    // leaf is reached at the left leaf's number — which is the number the knob
    // shows when the reader gets there.
    let page = layout.page_of_row(row) / step * step;
    (page.min(last) as f32 / last as f32).clamp(0.0, 1.0)
}

/// Left edge of every character of `text`, plus the end of the last one.
///
/// One lock on the font atlas for the whole row, not one per glyph. This runs
/// for every row on screen every frame, and it used to call [`measure`] — which
/// takes the lock and is handed a `&str` — once per character, building a
/// fresh `String` for each one. A two page spread is around sixteen hundred
/// characters, so a still window was taking that many locks and allocations
/// sixty times a second to draw text that had not moved.
fn char_positions(ctx: &Context, font: &FontId, text: &str, fill: layout::Stretch) -> Vec<f32> {
    let mut xs = Vec::with_capacity(text.chars().count() + 1);
    ctx.fonts_mut(|f| {
        let mut x = 0.0f32;
        for c in text.chars() {
            xs.push(x);
            // Zero for a combining mark, the same rule `measure` follows.
            if !crate::text::is_combining(c) {
                x += f.glyph_width(font, c);
            }
            match fill {
                layout::Stretch::WordGaps(extra) if c == ' ' => x += extra,
                layout::Stretch::Letters(extra) => x += extra,
                _ => {}
            }
        }
        xs.push(x);
    });
    xs
}

/// The marks that fall on the character range `start..end`.
///
/// A plain scan, deliberately. The search results can be binary searched
/// because [`crate::text::Document::search`] guarantees they are ordered and
/// do not overlap; highlights carry no such promise once `realign_highlights`
/// has moved them about, and a binary search over a list that is not sorted
/// does not run slowly — it silently stops painting some of the reader's marks.
fn highlights_touching(marks: &[Highlight], start: usize, end: usize) -> impl Iterator<Item = &Highlight> {
    marks.iter().filter(move |h| h.start < end && start < h.end)
}

/// Width of `text` in `font`, measured the same way the layout measured it.
///
/// A combining mark is drawn onto the letter before it and moves nothing along,
/// so it contributes no width. Normalising the document composes most of them
/// away; the scripts that have no composed form — Devanagari, Thai, Hebrew
/// points — still arrive here, and counting their advance would put every
/// character after them further right than the painter drew it.
fn measure(ctx: &Context, font: &FontId, text: &str) -> f32 {
    ctx.fonts_mut(|f| {
        text.chars()
            .map(|c| {
                if crate::text::is_combining(c) {
                    0.0
                } else {
                    f.glyph_width(font, c)
                }
            })
            .sum()
    })
}

impl eframe::App for ReaderApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let ctx = &ctx;
        self.poll_loading(ctx);
        self.handle_files(ui);
        self.handle_middle_button(ctx);
        self.run_autoscroll(ctx);
        self.handle_keys(ctx);

        let p = self.palette();
        self.top_bar(ui, &p);
        self.bottom_bar(ui, &p);
        self.side_drawer(ui, &p);
        self.central(ui, &p);
        self.settings_window(ctx);
        self.note_window(ctx);
        self.error_window(ctx);

        // Keep setting the book while the reader looks at the first pages.
        if !self.ts.done {
            self.pump(ctx, std::time::Duration::from_millis(6));
            self.settle_anchor();
        }

        // Animations keep the window repainting only while they run.
        if self.turn.abs() > 0.001 {
            self.turn *= 0.82;
            if self.turn.abs() <= 0.001 {
                self.turn = 0.0;
            }
            ctx.request_repaint();
        }
        if let Some((_, t)) = &mut self.toast {
            *t -= ctx.input(|i| i.stable_dt).min(0.1) as f64;
            if *t <= 0.0 {
                self.toast = None;
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(80));
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_progress();
        self.settings.save();
    }
}

impl ReaderApp {
    fn handle_files(&mut self, ui: &egui::Ui) {
        let ctx = ui.ctx();
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        if let Some(path) = dropped.into_iter().find_map(|f| f.path) {
            self.save_progress();
            self.open(&path);
        }
        let hovering = ctx.input(|i| !i.raw.hovered_files.is_empty());
        if hovering {
            let p = self.palette();
            let screen = ui.max_rect();
            let painter = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::new("drop"),
            ));
            painter.rect_filled(screen, CornerRadius::ZERO, p.canvas.gamma_multiply(0.8));
            painter.text(
                screen.center(),
                Align2::CENTER_CENTER,
                "Drop a text file to read it",
                sans(20.0),
                p.ink,
            );
        }
    }

    fn handle_keys(&mut self, ctx: &Context) {
        // A box that takes text owns the keyboard, whichever box it is.
        //
        // This used to ask `egui_wants_keyboard_input() && self.show_search`,
        // which named one of the two boxes. The note editor was the other, and
        // it never got the keyboard: `handle_keys` runs at the top of the
        // frame, before the window is built and before anything has consumed
        // the events, so every letter typed into a note was read as a shortcut
        // as well. Writing "note about ch 3" cycled the theme, opened two
        // drawers, jumped to the next search hit and marked a passage in Sky —
        // and every space in the sentence turned a page.
        //
        // `text_edit_focused` asks what the guard always meant to ask.
        // `egui_wants_keyboard_input` is true for *any* focused widget, so
        // tabbing onto a toolbar button would have silenced every shortcut.
        if ctx.text_edit_focused() {
            let (esc, enter, shift) = ctx.input(|i| {
                (
                    i.key_pressed(Key::Escape),
                    i.key_pressed(Key::Enter),
                    i.modifiers.shift,
                )
            });
            // Escape mostly does not arrive here at all. egui reads it in
            // `begin_pass` and drops the focus itself (`Key::Escape if
            // !modifiers.any()` in its memory), which happens before a line of
            // this runs — so by the time the guard is asked, no box holds the
            // keyboard any more and the key falls through to `Action::Escape`,
            // which is where it is actually handled. It is still answered here
            // because egui only does that for a bare Escape: with a modifier
            // held the focus stays, and the key has to mean the same thing.
            if self.note_editor.is_some() {
                // Enter belongs to the note — it is a multi-line box.
                if esc {
                    self.note_editor = None;
                }
                return;
            }
            if self.show_search {
                if esc {
                    self.close_search();
                } else if enter {
                    if self.matches.is_empty() {
                        self.run_search(true);
                    } else {
                        self.step_match(if shift { -1 } else { 1 });
                    }
                }
            }
            return;
        }

        let mut actions: Vec<Action> = Vec::new();
        let mut wheel = 0.0f32;
        let mut zoom = 0.0f32;
        ctx.input(|i| {
            let ctrl = i.modifiers.command;
            for (key, action) in [
                (Key::ArrowRight, Action::Next),
                (Key::Space, Action::Next),
                (Key::PageDown, Action::Next),
                (Key::ArrowLeft, Action::Prev),
                (Key::Backspace, Action::Prev),
                (Key::PageUp, Action::Prev),
                (Key::Home, Action::First),
                (Key::End, Action::Last),
            ] {
                if i.key_pressed(key) && !ctrl {
                    actions.push(action);
                }
            }
            if i.key_pressed(Key::F) && ctrl {
                actions.push(Action::Search);
            }
            if i.key_pressed(Key::O) && ctrl {
                actions.push(Action::Open);
            }
            if i.key_pressed(Key::B) && ctrl {
                actions.push(Action::Bookmark);
            }
            if i.key_pressed(Key::T) && !ctrl {
                actions.push(Action::Theme);
            }
            if i.key_pressed(Key::C) && !ctrl {
                actions.push(Action::Contents);
            }
            if i.key_pressed(Key::Escape) {
                actions.push(Action::Escape);
            }
            if i.key_pressed(Key::N) && !ctrl {
                actions.push(Action::NextMatch);
            }
            if i.key_pressed(Key::C) && ctrl {
                actions.push(Action::Copy);
            }
            if i.key_pressed(Key::A) && ctrl {
                actions.push(Action::SelectPage);
            }
            if i.key_pressed(Key::Z) && ctrl {
                actions.push(Action::Undo);
            }
            for (key, ink) in [
                (Key::Num1, Ink::Yellow),
                (Key::Num2, Ink::Mint),
                (Key::Num3, Ink::Sky),
                (Key::Num4, Ink::Rose),
            ] {
                if i.key_pressed(key) && !ctrl {
                    actions.push(Action::Mark(ink));
                }
            }
            if i.key_pressed(Key::Delete) || (i.key_pressed(Key::Num0) && !ctrl) {
                actions.push(Action::Unmark);
            }
            if i.key_pressed(Key::H) && !ctrl {
                actions.push(Action::HighlightList);
            }
            if i.key_pressed(Key::V) && !ctrl {
                actions.push(Action::CycleMode);
            }
            if i.key_pressed(Key::ArrowDown) {
                actions.push(Action::LineDown);
            }
            if i.key_pressed(Key::ArrowUp) {
                actions.push(Action::LineUp);
            }
            if (i.key_pressed(Key::Plus) || i.key_pressed(Key::Equals)) && ctrl {
                actions.push(Action::Bigger);
            }
            if i.key_pressed(Key::Minus) && ctrl {
                actions.push(Action::Smaller);
            }
            wheel = i.smooth_scroll_delta.y;
            zoom = if i.modifiers.command { wheel } else { 0.0 };
        });

        // Rolling the wheel anywhere stops an auto-scroll — that is the reader
        // taking the page back by hand, wherever the pointer happens to be.
        if wheel.abs() > 0.5 && self.autoscroll.is_some() {
            self.autoscroll = None;
        }
        // But only the page is turned by it.
        if !self.pointer_on_the_page(ctx) {
            wheel = 0.0;
            zoom = 0.0;
        }
        // Ctrl + wheel resizes the type, as everywhere else.
        if zoom.abs() > 0.5 {
            self.set_font(self.settings.font_size + zoom.signum());
        } else if wheel.abs() > 0.5 {
            if self.scrolling() {
                // Scroll mode: the wheel moves the column directly.
                self.scroll_by(-wheel);
                self.save_progress();
            } else {
                // Paged modes: gather the wheel until it adds up to a leaf, so
                // one notch of a mouse turns one page and a trackpad flick
                // does not fly through the book.
                self.wheel_accum += wheel;
                let notch = NOTCH;
                while self.wheel_accum.abs() >= notch {
                    let dir = if self.wheel_accum < 0.0 { 1 } else { -1 };
                    self.wheel_accum -= notch * self.wheel_accum.signum();
                    self.turn_page(dir);
                }
            }
        } else if wheel == 0.0 {
            // Let the accumulator decay so an old flick does not turn a page
            // minutes later.
            self.wheel_accum *= 0.86;
        }

        for a in actions {
            match a {
                Action::CycleMode => {
                    let next = self.settings.mode.next();
                    self.set_mode(next);
                }
                Action::LineDown | Action::LineUp => {
                    if self.scrolling() {
                        let step = self.settings.font_size * self.settings.line_height;
                        self.scroll_by(if matches!(a, Action::LineDown) { step } else { -step });
                        self.save_progress();
                    }
                }
                Action::Next => self.turn_page(1),
                Action::Prev => self.turn_page(-1),
                Action::First => self.goto_fraction(0.0),
                Action::Last => {
                    self.finish_typesetting(ctx);
                    self.goto_fraction(1.0);
                }
                Action::Search => {
                    self.show_search = true;
                    self.focus_search = true;
                }
                Action::Open => self.pick_file(),
                Action::Bookmark => self.toggle_bookmark(),
                Action::Theme => {
                    let next = self.skin.next();
                    self.set_skin(ctx, next);
                    self.toast(format!("Theme: {}", next.name()));
                }
                Action::Contents => {
                    self.drawer = match self.drawer {
                        Some(Drawer::Contents) => None,
                        _ => Some(Drawer::Contents),
                    }
                }
                Action::NextMatch => self.step_match(1),
                Action::Copy => self.copy_selection(ctx),
                Action::SelectPage => self.select_visible_pages(),
                Action::Undo => self.undo(),
                Action::Mark(ink) => self.mark_selection(ink),
                Action::Unmark => self.unmark_selection(),
                Action::HighlightList => {
                    self.drawer = match self.drawer {
                        Some(Drawer::Highlights) => None,
                        _ => Some(Drawer::Highlights),
                    }
                }
                Action::Escape => {
                    // Innermost thing first. The note is on top of everything
                    // when it is open, and it is where a bare Escape lands:
                    // egui has already taken the keyboard off its box by the
                    // time this runs, so the guard above never sees the key.
                    if self.note_editor.is_some() {
                        self.note_editor = None;
                    } else if self.selection.is_some() {
                        self.selection = None;
                    } else if self.show_settings {
                        self.show_settings = false;
                    } else if self.show_search {
                        // ...and the search box is in the same position, which
                        // is why closing it used to leave the query and its
                        // hits behind: the branch that clears them is in the
                        // guard, and Escape does not go through the guard.
                        self.close_search();
                    } else {
                        self.drawer = None;
                    }
                }
                Action::Bigger => self.set_font(self.settings.font_size + 1.0),
                Action::Smaller => self.set_font(self.settings.font_size - 1.0),
            }
        }
    }

    /// Switch presentation without losing the reader's place.
    fn set_mode(&mut self, mode: ViewMode) {
        if self.settings.mode == mode {
            return;
        }
        self.anchor = self.visible_offset();
        self.settings.mode = mode;
        self.selection = None;
        self.key = None;
        self.settings.save();
        self.toast(format!("{} view", mode.name()));
    }

    /// Drag the page by `dy` points, as if a hand were on the paper: a hand
    /// moving down pulls the text down, which shows what came *before*.
    ///
    /// In Scroll mode that is the scroll offset. In the paged modes there is
    /// nowhere to slide to, so the movement is banked until it adds up to a
    /// leaf, and then a leaf turns.
    fn slide(&mut self, dy: f32) {
        if dy.abs() < 0.01 {
            return;
        }
        if self.scrolling() {
            self.scroll_by(-dy);
            self.save_progress();
            return;
        }
        // Banked movement is positive when the hand pushes the page up, which
        // is the direction of reading on.
        self.pan_accum -= dy;
        let notch = self.rows_height.max(120.0) * 0.5;
        while self.pan_accum.abs() >= notch {
            let dir = if self.pan_accum > 0.0 { 1 } else { -1 };
            self.pan_accum -= notch * self.pan_accum.signum();
            self.turn_page(dir);
        }
    }

    /// Handle the wheel *button*: a click starts or stops auto-scrolling, and
    /// holding it drags the page directly.
    fn handle_middle_button(&mut self, ctx: &Context) {
        let (pressed, released, down, pos) = ctx.input(|i| {
            (
                i.pointer.button_pressed(egui::PointerButton::Middle),
                i.pointer.button_released(egui::PointerButton::Middle),
                i.pointer.button_down(egui::PointerButton::Middle),
                i.pointer.latest_pos(),
            )
        });

        // A press that lands on the drawer or on a window is that widget's,
        // not the page's. Once a drag has started it may wander anywhere.
        if pressed && self.pointer_on_the_page(ctx) {
            if let Some(p) = pos {
                self.middle_drag = Some((p, 0.0));
            }
        }
        if down {
            if let (Some((last, moved)), Some(p)) = (self.middle_drag, pos) {
                let dy = p.y - last.y;
                // Dragging the page follows the hand: down means back.
                self.slide(dy);
                self.middle_drag = Some((p, moved + dy.abs()));
                if moved + dy.abs() > CLICK_SLOP {
                    ctx.set_cursor_icon(CursorIcon::Grabbing);
                }
            }
        }
        if released {
            let was_a_click = self
                .middle_drag
                .is_some_and(|(_, moved)| moved <= CLICK_SLOP);
            let anchor = self.middle_drag.map(|(at, _)| at);
            self.middle_drag = None;
            if was_a_click {
                // Toggle: a second click stops it, the way a browser does.
                if self.autoscroll.is_some() {
                    self.autoscroll = None;
                    self.toast("Auto-scroll off");
                } else {
                    self.autoscroll = anchor;
                    self.toast("Auto-scroll — move the pointer, click to stop");
                }
            }
        }
    }

    /// Keep an auto-scroll running: the further the pointer sits from the
    /// anchor, the faster the page moves.
    fn run_autoscroll(&mut self, ctx: &Context) {
        let Some(anchor) = self.autoscroll else { return };
        let (pos, clicked, escaped) = ctx.input(|i| {
            (
                i.pointer.latest_pos(),
                i.pointer.button_pressed(egui::PointerButton::Primary),
                i.key_pressed(Key::Escape),
            )
        });
        if clicked || escaped {
            self.autoscroll = None;
            return;
        }
        let Some(p) = pos else { return };
        let speed = autoscroll_speed(p.y - anchor.y);
        if speed != 0.0 {
            let dt = ctx.input(|i| i.stable_dt).min(0.05);
            self.slide(-speed * dt);
        }
        ctx.set_cursor_icon(if speed > 0.0 {
            CursorIcon::ResizeSouth
        } else if speed < 0.0 {
            CursorIcon::ResizeNorth
        } else {
            CursorIcon::Grab
        });
        ctx.request_repaint();
    }

    /// The marker that shows where an auto-scroll is anchored.
    fn paint_autoscroll_anchor(&self, ui: &egui::Ui, p: &Palette) {
        let Some(anchor) = self.autoscroll else { return };
        let painter = ui.painter();
        painter.circle_filled(anchor, 13.0, p.panel.gamma_multiply(0.92));
        painter.circle_stroke(anchor, 13.0, Stroke::new(1.0, p.hairline));
        painter.circle_filled(anchor, 2.5, p.ink_soft);
        for dir in [-1.0f32, 1.0] {
            let tip = Pos2::new(anchor.x, anchor.y + dir * 9.0);
            painter.add(egui::Shape::convex_polygon(
                vec![
                    tip,
                    Pos2::new(anchor.x - 4.0, anchor.y + dir * 4.5),
                    Pos2::new(anchor.x + 4.0, anchor.y + dir * 4.5),
                ],
                p.ink_soft,
                Stroke::NONE,
            ));
        }
    }

    fn set_font(&mut self, size: f32) {
        self.settings.font_size = size.clamp(MIN_FONT, MAX_FONT);
        self.anchor = self.visible_offset();
        self.key = None;
        self.settings.save();
    }

    fn top_bar(&mut self, ui: &mut egui::Ui, p: &Palette) {
        let ctx = &ui.ctx().clone();
        egui::Panel::top("titlebar")
            .exact_size(52.0)
            .frame(
                egui::Frame::NONE
                    .fill(p.panel)
                    .inner_margin(egui::Margin::symmetric(16, 8)),
            )
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    let title = self
                        .doc
                        .as_ref()
                        .map(|d| d.title.clone())
                        .unwrap_or_else(|| "Anti-library".into());
                    ui.label(
                        egui::RichText::new(title)
                            .font(serif(18.0))
                            .color(p.ink)
                            .strong(),
                    );
                    if let Some(ch) = self.chapter_here() {
                        ui.label(
                            egui::RichText::new(format!("· {ch}"))
                                .font(sans(13.0))
                                .color(p.ink_soft),
                        );
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if tool(ui, IC_SETTINGS, "Reading settings", self.show_settings).clicked() {
                            self.show_settings = !self.show_settings;
                        }
                        if tool(
                            ui,
                            match self.settings.mode {
                                ViewMode::Book => IC_BOOK,
                                ViewMode::Page => IC_PAGE,
                                ViewMode::Scroll => IC_SCROLL,
                            },
                            &format!("{} view (V)", self.settings.mode.name()),
                            false,
                        )
                        .clicked()
                        {
                            let next = self.settings.mode.next();
                            self.set_mode(next);
                        }
                        if tool(ui, IC_THEME, "Theme (T)", false).clicked() {
                            let next = self.skin.next();
                            self.set_skin(ctx, next);
                        }
                        if tool(ui, IC_SEARCH, "Search (Ctrl+F)", self.show_search).clicked() {
                            self.show_search = !self.show_search;
                            self.focus_search = self.show_search;
                        }
                        if tool(
                            ui,
                            IC_BOOKMARK,
                            "Bookmark this page (Ctrl+B)",
                            self.bookmarked_here(),
                        )
                        .clicked()
                        {
                            self.toggle_bookmark();
                        }
                        if tool(
                            ui,
                            IC_CONTENTS,
                            "Contents and bookmarks (C)",
                            self.drawer.is_some(),
                        )
                        .clicked()
                        {
                            self.drawer = match self.drawer {
                                Some(_) => None,
                                None => Some(Drawer::Contents),
                            };
                        }
                        if ui.button("Open\u{2026}").on_hover_text("Ctrl+O").clicked() {
                            self.pick_file();
                        }
                    });
                });
            });
    }

    fn bottom_bar(&mut self, ui: &mut egui::Ui, p: &Palette) {
        egui::Panel::bottom("footer")
            .exact_size(46.0)
            .frame(
                egui::Frame::NONE
                    .fill(p.panel)
                    .inner_margin(egui::Margin::symmetric(16, 8)),
            )
            .show(ui, |ui| {
                if self.doc.is_none() {
                    ui.centered_and_justified(|ui| {
                        ui.label(
                            egui::RichText::new("No book open")
                                .font(sans(12.5))
                                .color(p.ink_faint),
                        );
                    });
                    return;
                }
                let pages = self.ts.layout.pages.len().max(1);
                let step = self.columns();
                let _ = pages;
                let shown = if self.scrolling() {
                    let total = self.ts.layout.rows.len().max(1);
                    let row = self.ts.layout.row_at_height(self.scroll) + 1;
                    format!("line {row} of {total}")
                } else {
                    let total = if self.ts.done {
                        format!("{pages}")
                    } else {
                        format!("{pages}+")
                    };
                    if step == 2 && self.page + 1 < pages {
                        format!("{}\u{2013}{} / {total}", self.page + 1, self.page + 2)
                    } else {
                        format!("{} / {total}", self.page + 1)
                    }
                };

                ui.horizontal_centered(|ui| {
                    ui.label(
                        egui::RichText::new(shown)
                            .font(sans(12.5))
                            .color(p.ink_soft),
                    );
                    if !self.ts.done {
                        if let Some(doc) = &self.doc {
                            ui.label(
                                egui::RichText::new(format!(
                                    "· setting {:.0}%",
                                    self.ts.progress(doc) * 100.0
                                ))
                                .font(sans(12.0))
                                .color(p.ink_faint),
                            );
                        }
                    }

                    // How much reading is left. `finished` has to mean the end
                    // of the book and nothing else: it was shown whenever the
                    // estimate rounded to zero minutes, so a short document sat
                    // at "0% · finished" from the moment it opened.
                    let progress = self.progress();
                    let at_the_end = progress >= 0.999;
                    let remaining = self
                        .doc
                        .as_ref()
                        .map(|d| (d.words as f32 * (1.0 - progress) / 240.0).round() as i64)
                        .unwrap_or(0);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(format!("{:.0}%", self.progress() * 100.0))
                                .font(sans(12.5))
                                .color(p.ink_soft),
                        );
                        ui.label(
                            egui::RichText::new(if at_the_end {
                                "finished".to_string()
                            } else if remaining > 0 {
                                format!("about {remaining} min left")
                            } else {
                                "a minute left".to_string()
                            })
                            .font(sans(12.5))
                            .color(p.ink_faint),
                        );

                        // The progress rule, with a tick for every chapter.
                        let width = (ui.available_width() - 24.0).max(80.0);
                        let (rect, resp) = ui.allocate_exact_size(
                            Vec2::new(width, 22.0),
                            Sense::click_and_drag(),
                        );
                        let line = Rect::from_center_size(
                            rect.center(),
                            Vec2::new(rect.width(), 4.0),
                        );
                        let painter = ui.painter();
                        painter.rect_filled(line, CornerRadius::same(2), p.hairline);
                        let done = Rect::from_min_size(
                            line.min,
                            Vec2::new(line.width() * self.progress(), line.height()),
                        );
                        painter.rect_filled(done, CornerRadius::same(2), p.accent);
                        for &row in &self.ts.layout.chapter_rows {
                            let x = line.left() + line.width() * self.fraction_of_row(row);
                            painter.vline(
                                x,
                                rect.center().y - 6.0..=rect.center().y + 6.0,
                                Stroke::new(1.0, p.ink_faint),
                            );
                        }
                        let knob = Pos2::new(
                            line.left() + line.width() * self.progress(),
                            rect.center().y,
                        );
                        painter.circle_filled(knob, 6.0, p.accent);
                        painter.circle_stroke(knob, 6.0, Stroke::new(1.5, p.page));

                        if resp.dragged() || resp.clicked() {
                            if let Some(pos) = resp.interact_pointer_pos() {
                                let t = (pos.x - line.left()) / line.width();
                                self.goto_fraction(t);
                            }
                        }
                        if resp.hovered() {
                            ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
                        }
                    });
                });
            });
    }

    fn side_drawer(&mut self, ui: &mut egui::Ui, p: &Palette) {
        let Some(drawer) = self.drawer else { return };
        egui::Panel::left("drawer")
            .exact_size(288.0)
            .resizable(false)
            .frame(
                egui::Frame::NONE
                    .fill(p.panel)
                    .inner_margin(egui::Margin::same(14)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for (tab, label) in [
                        (Drawer::Contents, "Contents"),
                        (Drawer::Bookmarks, "Marks"),
                        (Drawer::Highlights, "Highlights"),
                    ] {
                        if ui.selectable_label(drawer == tab, label).clicked() {
                            self.drawer = Some(tab);
                        }
                    }
                });
                ui.add_space(6.0);
                ui.separator();
                ui.add_space(4.0);

                if drawer == Drawer::Highlights {
                    self.highlight_tools(ui, p);
                }
                egui::ScrollArea::vertical().show(ui, |ui| match drawer {
                    Drawer::Contents => self.contents_list(ui, p),
                    Drawer::Bookmarks => self.bookmarks_list(ui, p),
                    Drawer::Highlights => self.highlights_list(ui, p),
                });
            });
    }

    fn contents_list(&mut self, ui: &mut egui::Ui, p: &Palette) {
        let Some(doc) = &self.doc else {
            ui.label(egui::RichText::new("No book open").color(p.ink_faint));
            return;
        };
        if doc.chapters.is_empty() {
            ui.label(
                egui::RichText::new("This text has no chapter headings.")
                    .font(sans(13.0))
                    .color(p.ink_faint),
            );
            return;
        }
        let here = self.chapter_index_here();
        let chapters: Vec<(usize, String, usize)> = doc
            .chapters
            .iter()
            .enumerate()
            .map(|(i, c)| (i, c.title.clone(), c.offset))
            .collect();
        for (i, title, offset) in chapters {
            let current = here == Some(i);
            let text = egui::RichText::new(title)
                .font(serif(14.5))
                .color(if current { p.accent } else { p.ink });
            if ui
                .selectable_label(current, text)
                .on_hover_text("Jump to this chapter")
                .clicked()
            {
                self.goto_offset(offset);
            }
        }
    }

    fn bookmarks_list(&mut self, ui: &mut egui::Ui, p: &Palette) {
        let marks = self.bookmarks();
        if marks.is_empty() {
            ui.label(
                egui::RichText::new("No bookmarks yet.\nPress Ctrl+B while reading.")
                    .font(sans(13.0))
                    .color(p.ink_faint),
            );
            return;
        }
        let mut remove = None;
        let mut jump = None;
        for (i, b) in marks.iter().enumerate() {
            ui.horizontal(|ui| {
                if ui
                    .add(
                        egui::Label::new(
                            egui::RichText::new(&b.label).font(serif(13.5)).color(p.ink),
                        )
                        .truncate()
                        .sense(Sense::click()),
                    )
                    .clicked()
                {
                    jump = Some(b.offset);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("\u{00d7}").on_hover_text("Delete").clicked() {
                        remove = Some(i);
                    }
                });
            });
            ui.add_space(2.0);
        }
        if let Some(offset) = jump {
            self.goto_offset(offset);
        }
        if let Some(i) = remove {
            self.remember();
            if let Some(doc) = &self.doc {
                let key = doc.path.clone();
                let rec = self.library.record(&key);
                if i < rec.bookmarks.len() {
                    rec.bookmarks.remove(i);
                }
                self.save_library();
            }
        }
    }

    /// Colour filter and export buttons above the highlight list.
    fn highlight_tools(&mut self, ui: &mut egui::Ui, p: &Palette) {
        let marks = self.highlights();
        ui.horizontal(|ui| {
            let (r, resp) = ui.allocate_exact_size(Vec2::splat(20.0), Sense::click());
            ui.painter().circle_stroke(
                r.center(),
                8.0,
                Stroke::new(
                    if self.ink_filter.is_none() { 2.0 } else { 1.0 },
                    p.ink_soft,
                ),
            );
            if resp.on_hover_text("All colours").clicked() {
                self.ink_filter = None;
            }
            for ink in Ink::ALL {
                let n = marks.iter().filter(|h| h.ink == ink).count();
                let (r, resp) = ui.allocate_exact_size(Vec2::splat(20.0), Sense::click());
                ui.painter().circle_filled(
                    r.center(),
                    8.0,
                    theme::ink_colour(ink, self.skin.is_dark()),
                );
                if self.ink_filter == Some(ink) {
                    ui.painter()
                        .circle_stroke(r.center(), 10.0, Stroke::new(2.0, p.ink_soft));
                }
                if resp
                    .on_hover_text(format!("{} ({n})", ink.name()))
                    .clicked()
                {
                    self.ink_filter = if self.ink_filter == Some(ink) {
                        None
                    } else {
                        Some(ink)
                    };
                }
            }
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui
                .small_button("Copy all")
                .on_hover_text("Every highlight as Markdown")
                .clicked()
            {
                let md = self.highlights_markdown();
                if md.trim().is_empty() {
                    self.toast("Nothing highlighted yet");
                } else {
                    ui.ctx().copy_text(md);
                    self.toast("Highlights copied");
                }
            }
            if ui.small_button("Export\u{2026}").clicked() {
                self.export_highlights();
            }
        });
        ui.add_space(4.0);
        ui.separator();
    }

    fn export_highlights(&mut self) {
        let md = self.highlights_markdown();
        if md.trim().is_empty() {
            self.toast("Nothing highlighted yet");
            return;
        }
        let name = self
            .doc
            .as_ref()
            .map(|d| format!("{} highlights.md", d.title))
            .unwrap_or_else(|| "highlights.md".into());
        if let Some(path) = rfd::FileDialog::new()
            .set_file_name(name)
            .add_filter("Markdown", &["md"])
            .save_file()
        {
            match std::fs::write(&path, md) {
                Ok(()) => self.toast(format!("Saved to {}", path.display())),
                Err(e) => {
                    self.error = Some(("Could not write that file".into(), format!("{e}")))
                }
            }
        }
    }

    fn highlights_list(&mut self, ui: &mut egui::Ui, p: &Palette) {
        let marks = self.highlights();
        let shown: Vec<(usize, Highlight)> = marks
            .into_iter()
            .enumerate()
            .filter(|(_, h)| self.ink_filter.is_none_or(|f| f == h.ink))
            .collect();
        if shown.is_empty() {
            ui.label(
                egui::RichText::new(
                    "Nothing marked yet.\nDrag across the text, then pick a colour.",
                )
                .font(sans(13.0))
                .color(p.ink_faint),
            );
            return;
        }
        let mut jump = None;
        let mut remove = None;
        let mut edit: Option<(usize, String)> = None;
        for (i, h) in shown {
            let colour = theme::ink_colour(h.ink, self.skin.is_dark());
            let resp = ui
                .scope(|ui| {
                    ui.horizontal_top(|ui| {
                        let (r, _) = ui.allocate_exact_size(Vec2::new(6.0, 34.0), Sense::hover());
                        let bar = if h.stale { colour.gamma_multiply(0.35) } else { colour };
                        ui.painter().rect_filled(r, CornerRadius::same(3), bar);
                        let mut text = egui::RichText::new(h.text.trim())
                            .font(serif(13.5))
                            .color(if h.stale { p.ink_faint } else { p.ink });
                        if h.stale {
                            text = text.italics();
                        }
                        let label = ui.add(egui::Label::new(text).truncate().sense(Sense::click()));
                        if h.stale {
                            label.on_hover_text("These words are not in the file any more")
                        } else {
                            label
                        }
                    })
                    .inner
                })
                .inner;
            if resp.clicked() {
                jump = Some(h.start);
            }
            // The note, set under the passage it belongs to.
            if !h.note.trim().is_empty() {
                ui.horizontal_top(|ui| {
                    ui.add_space(10.0);
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(h.note.trim())
                                .font(sans(12.0))
                                .color(p.ink_soft),
                        )
                        .wrap(),
                    );
                });
            }
            resp.context_menu(|ui| {
                if ui
                    .button(if h.note.is_empty() { "Add note\u{2026}" } else { "Edit note\u{2026}" })
                    .clicked()
                {
                    edit = Some((i, h.note.clone()));
                    ui.close();
                }
                if ui.button("Copy").clicked() {
                    ui.ctx().copy_text(h.text.clone());
                    ui.close();
                }
                if ui.button("Delete").clicked() {
                    remove = Some(i);
                    ui.close();
                }
            });
            ui.add_space(3.0);
        }
        if let Some(e) = edit {
            self.note_editor = Some(e);
        }
        if let Some(offset) = jump {
            self.goto_offset(offset);
        }
        if let Some(i) = remove {
            self.delete_highlight(i);
        }
    }

    fn central(&mut self, ui: &mut egui::Ui, p: &Palette) {
        let ctx = &ui.ctx().clone();
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(p.canvas))
            .show(ui, |ui| {
                // Remembered for the next frame's wheel and wheel button.
                self.page_area = ui.available_rect_before_wrap();
                if let Some((name, _)) = &self.loading {
                    let area = ui.available_rect_before_wrap();
                    let card = Rect::from_center_size(area.center(), Vec2::new(360.0, 120.0));
                    self.paint_sheet(ui, card);
                    ui.painter().text(
                        card.center(),
                        Align2::CENTER_CENTER,
                        format!("Opening {name}\u{2026}"),
                        serif(16.0),
                        p.ink_soft,
                    );
                    return;
                }
                if self.doc.is_none() {
                    self.empty_state(ui, p);
                    return;
                }
                let area = ui.available_rect_before_wrap();
                // Generous margins: a book is mostly white space.
                let side = (area.width() * 0.06).clamp(18.0, 90.0);
                let vert = (area.height() * 0.06).clamp(16.0, 56.0);
                let sheet = area.shrink2(Vec2::new(side, vert));
                self.paint_sheet(ui, sheet);

                let ink_margin_x = (sheet.width() * 0.075).clamp(20.0, 74.0);
                let ink_margin_y = (sheet.height() * 0.09).clamp(24.0, 68.0);
                let block = sheet.shrink2(Vec2::new(ink_margin_x, ink_margin_y));
                let cols = self.columns();
                let gutter = if cols == 2 {
                    (block.width() * 0.07).clamp(24.0, 72.0)
                } else {
                    0.0
                };
                let col_w = (block.width() - gutter) / cols as f32;
                // Keep the measure in the comfortable 45–75 character range.
                let max_w = self.settings.measure * self.settings.font_size * 0.5;
                let col_w = col_w.min(max_w).max(120.0);
                let used = col_w * cols as f32 + gutter;
                let left = block.center().x - used * 0.5;
                let col_h = block.height();

                self.rows_height = col_h;
                self.relayout(ctx, col_w, col_h);

                let mut hits = Vec::new();
                if self.scrolling() {
                    // One continuous column: draw from the first row that the
                    // scroll position reaches, offset by however much of that
                    // row is above the top of the view.
                    let rect =
                        Rect::from_min_size(Pos2::new(left, block.top()), Vec2::new(col_w, col_h));
                    let first = self.ts.layout.row_at_height(self.scroll);
                    let y_start = block.top()
                        - (self.scroll - self.ts.layout.tops.get(first).copied().unwrap_or(0.0));
                    self.paint_rows(
                        ui,
                        rect,
                        first,
                        self.ts.layout.rows.len(),
                        y_start,
                        1.0,
                        &mut hits,
                    );

                    // A slim rule on the right stands in for a scrollbar.
                    let total = self.ts.layout.total_height().max(1.0);
                    let frac = (col_h / total).clamp(0.04, 1.0);
                    let track = Rect::from_min_size(
                        Pos2::new(sheet.right() - 10.0, sheet.top() + 16.0),
                        Vec2::new(3.0, sheet.height() - 32.0),
                    );
                    ui.painter()
                        .rect_filled(track, CornerRadius::same(2), p.page_edge);
                    let thumb_h = (track.height() * frac).max(24.0);
                    let t = if self.max_scroll() > 0.0 {
                        self.scroll / self.max_scroll()
                    } else {
                        0.0
                    };
                    ui.painter().rect_filled(
                        Rect::from_min_size(
                            Pos2::new(track.left(), track.top() + (track.height() - thumb_h) * t),
                            Vec2::new(track.width(), thumb_h),
                        ),
                        CornerRadius::same(2),
                        p.ink_faint,
                    );
                } else {
                    let slide = self.turn * 26.0;
                    let alpha = 1.0 - self.turn.abs() * 0.55;
                    for c in 0..cols {
                        let x = left + c as f32 * (col_w + gutter) - slide;
                        let rect = Rect::from_min_size(
                            Pos2::new(x, block.top()),
                            Vec2::new(col_w, col_h),
                        );
                        self.paint_page(ui, rect, self.page + c, alpha, &mut hits);
                    }
                    if cols == 2 {
                        // The spine: a faint fold between the leaves.
                        let x = left + col_w + gutter * 0.5;
                        ui.painter().vline(
                            x,
                            sheet.top() + 24.0..=sheet.bottom() - 24.0,
                            Stroke::new(1.0, p.page_edge),
                        );
                    }
                }
                self.hits = hits;

                // Drag across the text to select it; click the outer thirds to
                // turn pages, the way a tablet reader does.
                let resp = ui.interact(
                    area,
                    egui::Id::new("page-surface"),
                    Sense::click_and_drag(),
                );
                // The page is painted, not laid out as widgets, so nothing about
                // it reaches the accessibility tree on its own — a screen reader
                // met a window with some buttons and a large blank. Publishing
                // what is on the leaves gives it the one thing that matters.
                resp.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::WidgetType::Label,
                        true,
                        self.visible_text(),
                    )
                });

                let over_text = resp
                    .hover_pos()
                    .is_some_and(|pos| self.hits.iter().any(|h| h.rect.contains(pos)));
                if over_text {
                    ui.ctx().set_cursor_icon(CursorIcon::Text);
                }

                if resp.drag_started() {
                    if let Some(pos) = resp.interact_pointer_pos() {
                        if let Some(off) = self.offset_at(ctx, pos) {
                            self.selection = Some(Selection {
                                anchor: off,
                                cursor: off,
                            });
                            self.selecting = true;
                        }
                    }
                }
                if resp.dragged() && self.selecting {
                    if let Some(pos) = resp.interact_pointer_pos() {
                        // Dragging past the top or the bottom of the text turns
                        // the leaf and keeps going. Without this a passage that
                        // crosses a page break could not be selected at all:
                        // the pointer had nowhere to go but off the paper.
                        let (above, below) = (block.top() + 12.0, block.bottom() - 12.0);
                        if pos.y < above {
                            self.extend_selection_beyond(ctx, -1);
                        } else if pos.y > below {
                            self.extend_selection_beyond(ctx, 1);
                        }
                        if let (Some(off), Some(sel)) =
                            (self.offset_at(ctx, pos), self.selection.as_mut())
                        {
                            sel.cursor = off;
                        }
                    }
                }
                if resp.drag_stopped() {
                    self.selecting = false;
                    if self.selection.is_some_and(|s| s.is_empty()) {
                        self.selection = None;
                    }
                }

                if resp.clicked() {
                    if let Some(pos) = resp.interact_pointer_pos() {
                        if self.selection.is_some() {
                            self.selection = None;
                        } else if !over_text && !self.scrolling() {
                            let t = (pos.x - area.left()) / area.width();
                            if t < 0.33 {
                                self.turn_page(-1);
                            } else if t > 0.67 {
                                self.turn_page(1);
                            }
                        }
                    }
                }
                // Double click marks the word under the pointer.
                if resp.double_clicked() {
                    if let Some(pos) = resp.interact_pointer_pos() {
                        if let Some(off) = self.offset_at(ctx, pos) {
                            if let Some((a, b)) = self.word_at(off) {
                                self.selection = Some(Selection {
                                    anchor: a,
                                    cursor: b,
                                });
                            }
                        }
                    }
                }

                self.paint_autoscroll_anchor(ui, p);
                self.selection_toolbar(ui, p);
                self.search_overlay(ui, p, area);
                self.toast_overlay(ui, p, area);
            });
    }

    /// Carry a drag past the edge of what is on screen.
    ///
    /// The page moves and the selection follows it to the far edge of the new
    /// view, so the reader can keep dragging and the marked range keeps growing.
    /// Paced by the frame, which is slow enough to steer and fast enough to get
    /// somewhere.
    fn extend_selection_beyond(&mut self, ctx: &Context, dir: isize) {
        let before = self.visible_offset();
        if self.scrolling() {
            let step = self.settings.font_size * self.settings.line_height * 1.5;
            self.scroll_by(step * dir as f32);
        } else {
            self.turn_page(dir);
        }
        if self.visible_offset() == before {
            return; // at one end of the book already
        }
        let (first, last) = self.visible_span();
        if let Some(sel) = self.selection.as_mut() {
            sel.cursor = if dir > 0 { last } else { first };
        }
        ctx.request_repaint();
    }

    /// Word boundaries around `offset`, for double-click selection.
    fn word_at(&self, offset: usize) -> Option<(usize, usize)> {
        let row = self
            .ts
            .layout
            .rows
            .iter()
            .find(|r| r.kind != RowKind::Blank && r.clip(offset, offset + 1).is_some())?;
        let chars: Vec<char> = row.text.chars().collect();
        let i = (offset - row.offset).min(chars.len().saturating_sub(1));
        let breaks = |c: char| c.is_whitespace() || "\u{2018}\u{2019}\u{201c}\u{201d}.,;:!?()[]{}\"'".contains(c);
        if chars.is_empty() || breaks(chars[i]) {
            return None;
        }
        let mut a = i;
        while a > 0 && !breaks(chars[a - 1]) {
            a -= 1;
        }
        let mut b = i;
        while b < chars.len() && !breaks(chars[b]) {
            b += 1;
        }
        Some((row.offset + a, row.offset + b))
    }

    /// The little bar that appears over a selection: four inks, copy, erase.
    fn selection_toolbar(&mut self, ui: &mut egui::Ui, p: &Palette) {
        let Some(sel) = self.selection else { return };
        if sel.is_empty() || self.selecting {
            return;
        }
        let (a, b) = sel.range();
        // Anchor the bar above the first row of the selection.
        let Some(hit) = self
            .hits
            .iter()
            .find(|h| {
                self.ts.layout
                    .rows
                    .get(h.row)
                    .is_some_and(|r| r.clip(a, b).is_some())
            })
            .cloned()
        else {
            return;
        };

        let size = Vec2::new(232.0, 40.0);
        let mut pos = Pos2::new(
            hit.rect.center().x - size.x * 0.5,
            hit.rect.top() - size.y - 8.0,
        );
        let screen = ui.max_rect();
        pos.x = pos.x.clamp(screen.left() + 8.0, screen.right() - size.x - 8.0);
        if pos.y < screen.top() + 4.0 {
            pos.y = hit.rect.bottom() + 8.0;
        }
        let rect = Rect::from_min_size(pos, size);

        let painter = ui.painter();
        painter.rect_filled(rect.expand(5.0), CornerRadius::same(12), p.shadow);
        painter.rect_filled(rect, CornerRadius::same(12), p.panel);
        painter.rect_stroke(
            rect,
            CornerRadius::same(12),
            Stroke::new(1.0, p.hairline),
            StrokeKind::Inside,
        );

        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(rect.shrink2(Vec2::new(10.0, 6.0)))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        let mut chosen = None;
        for ink in Ink::ALL {
            let (r, resp) = child.allocate_exact_size(Vec2::splat(22.0), Sense::click());
            let colour = theme::ink_colour(ink, self.skin.is_dark());
            child
                .painter()
                .circle_filled(r.center(), 9.0, colour);
            if resp.hovered() || self.ink == ink {
                child.painter().circle_stroke(
                    r.center(),
                    11.0,
                    Stroke::new(1.5, p.ink_soft),
                );
            }
            if resp.on_hover_text(format!("Highlight in {}", ink.name())).clicked() {
                chosen = Some(ink);
            }
        }
        child.add_space(4.0);
        let copy = child.small_button("Copy").on_hover_text("Ctrl+C").clicked();
        let erase = child
            .small_button("Erase")
            .on_hover_text("Remove highlights under the selection")
            .clicked();
        let undo = !self.undo.is_empty()
            && child.small_button("Undo").on_hover_text("Ctrl+Z").clicked();

        if let Some(ink) = chosen {
            self.mark_selection(ink);
        } else if copy {
            let ctx = ui.ctx().clone();
            self.copy_selection(&ctx);
            self.selection = None;
        } else if erase {
            self.unmark_selection();
        } else if undo {
            self.undo();
        }
    }

    fn empty_state(&mut self, ui: &mut egui::Ui, p: &Palette) {
        let area = ui.available_rect_before_wrap();
        // Measured last frame rather than fixed. The card used to be 360pt tall
        // whatever was on it, so a shelf of six books drew its last four rows
        // on the background *below* the card — and the taller the rows got, the
        // further out they went.
        let card = Rect::from_center_size(
            area.center(),
            Vec2::new(area.width().min(520.0), area.height().min(self.empty_card)),
        );
        self.paint_sheet(ui, card);
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(card.shrink(36.0))
                .layout(egui::Layout::top_down(egui::Align::Center)),
        );
        child.add_space(18.0);
        child.label(
            egui::RichText::new("Anti-library")
                .font(serif(30.0))
                .color(p.ink),
        );
        child.add_space(6.0);
        child.label(
            egui::RichText::new("The unread shelf is the useful half.")
                .font(serif(15.0))
                .italics()
                .color(p.ink_soft),
        );
        child.add_space(22.0);
        if child.button("Open a text file\u{2026}").clicked() {
            self.pick_file();
        }
        child.add_space(8.0);
        child.label(
            egui::RichText::new("or drop one onto this window")
                .font(sans(12.5))
                .color(p.ink_faint),
        );
        child.add_space(6.0);
        // The one place a reader can find the version without a console —
        // which this build does not have, and which a bug report needs.
        child.label(
            egui::RichText::new(format!("version {}", env!("CARGO_PKG_VERSION")))
                .font(sans(11.0))
                .color(p.ink_faint),
        );

        let mut recent: Vec<(String, String)> = self
            .library
            .recent()
            .into_iter()
            .filter(|(_, r)| !r.title.is_empty())
            .map(|(path, r)| (r.title.clone(), path.clone()))
            .take(6)
            .collect();
        // Three files can all be called "sample". Where the title alone does
        // not tell them apart, show the file name instead.
        let titles: Vec<String> = recent.iter().map(|(t, _)| t.clone()).collect();
        for (title, path) in recent.iter_mut() {
            if titles.iter().filter(|t| *t == title).count() > 1 {
                if let Some(name) = Path::new(path).file_name() {
                    *title = name.to_string_lossy().to_string();
                }
            }
        }
        if !recent.is_empty() {
            child.add_space(20.0);
            child.label(
                egui::RichText::new("RECENT")
                    .font(sans(11.0))
                    .color(p.ink_faint),
            );
            child.add_space(4.0);
            // Decided inside the rows, acted on after them: the library cannot
            // be borrowed for a change while the list is still reading it.
            let mut to_open: Option<String> = None;
            let mut to_forget: Option<String> = None;
            for (title, path) in recent {
                // A fixed-width block so the rows line up under one another and
                // sit centred like everything else on this card; the button is
                // laid out from the right edge, the title from the left.
                child.allocate_ui_with_layout(
                    Vec2::new(300.0, 22.0),
                    egui::Layout::right_to_left(egui::Align::Center),
                    |row| {
                        if forget_button(row).clicked() {
                            to_forget = Some(path.clone());
                        }
                        row.with_layout(
                            egui::Layout::left_to_right(egui::Align::Center),
                            |left| {
                                if left
                                    .add(
                                        egui::Label::new(
                                            egui::RichText::new(title)
                                                .font(serif(14.0))
                                                .color(p.accent),
                                        )
                                        .truncate()
                                        .sense(Sense::click()),
                                    )
                                    .on_hover_text(&path)
                                    .clicked()
                                {
                                    to_open = Some(path.clone());
                                }
                            },
                        );
                    },
                );
            }
            if let Some(path) = to_open {
                self.open(&PathBuf::from(path));
            }
            if let Some(key) = to_forget {
                // Every book ever opened stayed on this list for good, and each
                // one is asked about on the filesystem when the library opens —
                // which is why a shelf full of books on an unplugged drive made
                // the reader slow to start with no way to mend it.
                let name = self
                    .library
                    .get(&key)
                    .map(|r| r.title.clone())
                    .filter(|t| !t.is_empty())
                    .unwrap_or_else(|| key.clone());
                if self.library.forget(&key) {
                    self.save_library();
                    self.toast(format!("Took {name} off the shelf"));
                }
            }
        }

        // What the card needed to be, for the next frame to use. The 72 is the
        // margin taken off both ends by `shrink(36.0)` above.
        let needed = child.min_rect().height() + 72.0;
        if (needed - self.empty_card).abs() > 0.5 {
            self.empty_card = needed;
            child.ctx().request_repaint();
        }
    }

    fn search_overlay(&mut self, ui: &mut egui::Ui, p: &Palette, area: Rect) {
        if !self.show_search {
            return;
        }
        let rect = Rect::from_min_size(
            Pos2::new(area.right() - 372.0, area.top() + 16.0),
            Vec2::new(356.0, 46.0),
        );
        ui.painter()
            .rect_filled(rect.expand(6.0), CornerRadius::same(10), p.shadow);
        ui.painter()
            .rect_filled(rect, CornerRadius::same(10), p.panel);
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(10),
            Stroke::new(1.0, p.hairline),
            StrokeKind::Inside,
        );

        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(rect.shrink2(Vec2::new(10.0, 8.0)))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        let edit = child.add(
            egui::TextEdit::singleline(&mut self.search)
                .hint_text("Search this book")
                .desired_width(196.0),
        );
        if self.focus_search {
            edit.request_focus();
            self.focus_search = false;
        }
        if edit.changed() {
            self.run_search(false);
        }
        let count = if self.search.is_empty() {
            String::new()
        } else if self.matches.is_empty() {
            "none".to_string()
        } else {
            format!("{}/{}", self.match_cursor + 1, self.matches.len())
        };
        child.label(
            egui::RichText::new(count)
                .font(sans(12.0))
                .color(p.ink_faint),
        );
        if child.small_button("\u{2039}").clicked() {
            self.step_match(-1);
        }
        if child.small_button("\u{203a}").clicked() {
            if self.matches.is_empty() {
                self.run_search(true);
            } else {
                self.step_match(1);
            }
        }
    }

    fn toast_overlay(&mut self, ui: &mut egui::Ui, p: &Palette, area: Rect) {
        let Some((msg, life)) = self.toast.clone() else {
            return;
        };
        let alpha = (life as f32 / 0.6).clamp(0.0, 1.0);
        let font = sans(13.0);
        let width = measure(ui.ctx(), &font, &msg) + 34.0;
        let rect = Rect::from_center_size(
            Pos2::new(area.center().x, area.bottom() - 46.0),
            Vec2::new(width.max(120.0), 36.0),
        );
        let painter = ui.painter();
        painter.rect_filled(
            rect,
            CornerRadius::same(18),
            theme::blend(p.panel, p.ink, 0.85).gamma_multiply(alpha),
        );
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            msg,
            font,
            p.page.gamma_multiply(alpha),
        );
    }

    fn settings_window(&mut self, ctx: &Context) {
        if !self.show_settings {
            return;
        }
        let mut open = true;
        let mut changed = false;
        // Decided inside the panel, done after it: the library cannot be
        // borrowed for a change while the panel that reads it is still open.
        let (mut backup, mut restore, mut diagnose, mut updates) = (false, false, false, false);
        // Tall enough to run off a short screen, and it does: the panel grew
        // two sections and the version — the thing a bug report needs — went
        // under the bottom edge with no way to reach it. Anchored windows do
        // not move, so the panel has to be able to scroll inside itself.
        let room = (ctx.viewport_rect().height() - 96.0).max(240.0);
        egui::Window::new("Reading")
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .anchor(egui::Align2::RIGHT_TOP, [-16.0, 68.0])
            .show(ctx, |ui| {
            // `max_height` alone cannot make this taller: a scroll area takes
            // `available.at_most(max_size)`, and the window offers its child far
            // less than the screen has. `min_scrolled_height` is the floor that
            // is allowed to exceed it — and it only applies once the content is
            // long enough to need scrolling, so a short panel still shrinks.
            egui::ScrollArea::vertical()
                .max_height(room)
                .min_scrolled_height(room)
                .show(ui, |ui| {
                ui.set_width(268.0);
                ui.label(egui::RichText::new("PAPER").font(sans(11.0)));
                ui.horizontal_wrapped(|ui| {
                    for skin in Skin::ALL {
                        if ui
                            .selectable_label(self.skin == skin, skin.name())
                            .clicked()
                        {
                            self.set_skin(ui.ctx(), skin);
                        }
                    }
                });
                ui.add_space(10.0);

                ui.label(egui::RichText::new("TYPE").font(sans(11.0)));
                changed |= ui
                    .add(
                        egui::Slider::new(&mut self.settings.font_size, MIN_FONT..=MAX_FONT)
                            .text("size")
                            .fixed_decimals(0),
                    )
                    .changed();
                changed |= ui
                    .add(
                        egui::Slider::new(&mut self.settings.line_height, 1.2..=2.2)
                            .text("leading")
                            .fixed_decimals(2),
                    )
                    .changed();
                changed |= ui
                    .add(
                        egui::Slider::new(&mut self.settings.measure, 40.0..=90.0)
                            .text("measure")
                            .fixed_decimals(0),
                    )
                    .on_hover_text("Characters per line — 45 to 75 reads best")
                    .changed();
                ui.add_space(10.0);

                ui.label(egui::RichText::new("VIEW").font(sans(11.0)));
                ui.horizontal(|ui| {
                    for mode in ViewMode::ALL {
                        if ui
                            .selectable_label(self.settings.mode == mode, mode.name())
                            .on_hover_text(mode.hint())
                            .clicked()
                            && self.settings.mode != mode
                        {
                            self.set_mode(mode);
                        }
                    }
                });
                changed |= ui
                    .checkbox(&mut self.settings.focus, "Focus the line being read")
                    .on_hover_text("Dim the rest of the page")
                    .changed();
                ui.add_space(10.0);

                ui.label(egui::RichText::new("PAGE").font(sans(11.0)));
                changed |= ui
                    .checkbox(&mut self.settings.justify, "Justify text")
                    .changed();
                changed |= ui.checkbox(&mut self.settings.drop_caps, "Drop caps").changed();
                changed |= ui
                    .checkbox(
                        &mut self.settings.chapter_breaks,
                        "Start chapters on a new page",
                    )
                    .changed();
                changed |= ui
                    .checkbox(&mut self.settings.hyphenate, "Hyphenate")
                    .on_hover_text("Break words at their syllables so the column fills evenly")
                    .changed();
                ui.checkbox(&mut self.settings.page_animation, "Animate page turns");

                ui.add_space(10.0);
                ui.separator();
                let faint = self.skin.palette().ink_faint;
                if let Some(doc) = &self.doc {
                    ui.label(
                        egui::RichText::new(format!(
                            "{} · {} · {} words · {}",
                            doc.format.name(),
                            doc.encoding,
                            doc.words,
                            doc.path
                        ))
                        .font(sans(11.0))
                        .color(faint),
                    );
                }
                // Which faces the page is actually set in. On a machine with
                // none of the preferred ones the reader falls back silently,
                // and this is the only place that says so.
                let faces = if self.fonts.is_empty() {
                    "built-in font only".to_string()
                } else {
                    self.fonts.join(" · ")
                };
                ui.label(
                    egui::RichText::new(format!("type: {faces}"))
                        .font(sans(11.0))
                        .color(faint),
                )
                .on_hover_text("The faces found on this machine, best first");

                ui.add_space(10.0);
                ui.separator();
                ui.label(egui::RichText::new("LIBRARY").font(sans(10.5)).color(faint));
                ui.horizontal(|ui| {
                    backup |= ui
                        .small_button("Back up\u{2026}")
                        .on_hover_text(
                            "Write every book, bookmark, highlight and note to one file",
                        )
                        .clicked();
                    restore |= ui
                        .small_button("Restore\u{2026}")
                        .on_hover_text("Fold a backup back in. Nothing is removed")
                        .clicked();
                    diagnose |= ui
                        .small_button("Diagnostics\u{2026}")
                        .on_hover_text("What to attach to a bug report. No file names, no paths")
                        .clicked();
                });
                ui.label(
                    egui::RichText::new(
                        "Everything the reader keeps lives in one file on this machine.",
                    )
                    .font(sans(10.5))
                    .color(faint),
                );

                ui.add_space(10.0);
                ui.separator();
                ui.label(egui::RichText::new("VERSION").font(sans(10.5)).color(faint));
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(env!("CARGO_PKG_VERSION"))
                            .font(sans(11.0))
                            .color(self.skin.palette().ink),
                    );
                    if self.settings.updates_url.is_some() {
                        updates |= ui
                            .small_button("Check for updates")
                            .on_hover_text("Opens the release page in your browser")
                            .clicked();
                    }
                });
                if self.settings.updates_url.is_none() {
                    ui.label(
                        egui::RichText::new(
                            "This build does not update itself and has no release page set.",
                        )
                        .font(sans(10.5))
                        .color(faint),
                    );
                }
            });
            });
        if changed {
            self.anchor = self.visible_offset();
            self.key = None;
            self.settings = self.settings.clone().sanitised();
            self.settings.save();
        }
        if backup {
            self.back_up_library();
        } else if restore {
            self.restore_library();
        } else if diagnose {
            self.write_diagnostics();
        } else if updates {
            self.open_release_page();
        }
        self.show_settings = open;
    }

    /// The note attached to a marked passage.
    ///
    /// A colour says *that* a passage mattered. Everything a reader actually
    /// wants back a year later — why it mattered, what it argued against, what
    /// to look up — needs words, and until now there was nowhere to put them.
    fn note_window(&mut self, ctx: &Context) {
        let Some((index, _)) = self.note_editor.clone() else {
            return;
        };
        let marks = self.highlights();
        let Some(mark) = marks.get(index).cloned() else {
            self.note_editor = None;
            return;
        };
        let p = self.palette();
        let mut open = true;
        let mut save = false;
        let mut cancel = false;
        egui::Window::new("Note")
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_width(380.0);
                ui.label(
                    egui::RichText::new(mark.text.trim())
                        .font(serif(13.5))
                        .color(p.ink_soft)
                        .italics(),
                );
                ui.add_space(8.0);
                if let Some((_, text)) = &mut self.note_editor {
                    ui.add(
                        egui::TextEdit::multiline(text)
                            .desired_rows(5)
                            .desired_width(f32::INFINITY)
                            .hint_text("What this passage is for"),
                    )
                    .request_focus();
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    save = ui.button("Save").clicked();
                    cancel = ui.button("Cancel").clicked();
                });
            });
        if save {
            if let Some((i, text)) = self.note_editor.take() {
                self.set_note(i, text);
            }
        } else if cancel || !open {
            self.note_editor = None;
        }
    }

    fn set_note(&mut self, index: usize, note: String) {
        self.remember();
        let Some(doc) = &self.doc else { return };
        let key = doc.path.clone();
        let rec = self.library.record(&key);
        let Some(h) = rec.highlights.get_mut(index) else {
            return;
        };
        let had = !h.note.is_empty();
        h.note = note.trim().to_string();
        let now_has = !h.note.is_empty();
        self.save_library();
        self.toast(match (had, now_has) {
            (_, true) => "Note saved",
            (true, false) => "Note removed",
            (false, false) => "No note written",
        });
    }

    fn error_window(&mut self, ctx: &Context) {
        let Some((title, body)) = self.error.clone() else {
            return;
        };
        let mut open = true;
        egui::Window::new(title)
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(body);
            });
        if !open {
            self.error = None;
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum Action {
    CycleMode,
    LineDown,
    LineUp,
    Copy,
    SelectPage,
    Undo,
    Mark(Ink),
    Unmark,
    HighlightList,
    Next,
    Prev,
    First,
    Last,
    Search,
    Open,
    Bookmark,
    Theme,
    Contents,
    NextMatch,
    Escape,
    Bigger,
    Smaller,
}

/// Toolbar icons: Segoe MDL2 code points, with a word for machines that have
/// no icon face installed.
#[derive(Debug, Clone, Copy)]
struct Icon {
    glyph: &'static str,
    word: &'static str,
}

const IC_CONTENTS: Icon = Icon {
    glyph: "\u{e700}",
    word: "Contents",
};
const IC_BOOKMARK: Icon = Icon {
    glyph: "\u{e718}",
    word: "Mark",
};
const IC_SEARCH: Icon = Icon {
    glyph: "\u{e721}",
    word: "Find",
};
const IC_THEME: Icon = Icon {
    glyph: "\u{e793}",
    word: "Theme",
};
const IC_BOOK: Icon = Icon {
    glyph: "\u{e736}",
    word: "Book",
};
const IC_PAGE: Icon = Icon {
    glyph: "\u{e7c3}",
    word: "Page",
};
const IC_SCROLL: Icon = Icon {
    glyph: "\u{e8a1}",
    word: "Scroll",
};
const IC_SETTINGS: Icon = Icon {
    glyph: "\u{e713}",
    word: "Reading",
};
/// Take a book off the shelf.
///
/// A cross, not a wastebasket and not a minus: the file is not being deleted,
/// only taken out of a list — which is the same thing a browser tab or a recent
/// documents list means by a cross. `E738` was tried first and draws as a bare
/// horizontal bar, which says nothing at all in a column of six.
const IC_FORGET: Icon = Icon {
    glyph: "\u{e711}",
    word: "Remove",
};

/// The button that takes a book off the shelf, on the start screen.
///
/// A framed small button rather than a bare glyph: on the start screen there is
/// nothing around it to say it can be pressed, and an unframed icon beside a
/// title reads as part of the title. The name published to a screen reader is
/// the sentence, not the code point — where an icon face is installed the label
/// is a private-use glyph, which is no name at all.
fn forget_button(ui: &mut egui::Ui) -> egui::Response {
    const HINT: &str = "Take this book off the shelf";
    let text = if theme::icons_available() {
        egui::RichText::new(IC_FORGET.glyph).font(theme::icon(12.0))
    } else {
        egui::RichText::new(IC_FORGET.word).font(sans(11.0))
    };
    let resp = ui
        .add(egui::Button::new(text).small())
        .on_hover_text(HINT);
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, false, HINT));
    resp
}

/// A toolbar button that can show itself as active.
fn tool(ui: &mut egui::Ui, ic: Icon, hint: &str, active: bool) -> egui::Response {
    let text = if theme::icons_available() {
        egui::RichText::new(ic.glyph).font(theme::icon(15.0))
    } else {
        egui::RichText::new(ic.word).font(sans(13.0))
    };
    let resp = ui.selectable_label(active, text).on_hover_text(hint);
    // Where an icon face is installed the button's label is a private-use code
    // point, and that code point is the name the button published to the
    // accessibility tree — which is to say it published nothing. The window
    // goes to some trouble to publish the text on the page (see `central`) and
    // then left every control that acts on it unnamed. The hint is the name:
    // it says what the button does and which key does it without the mouse.
    resp.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, true, active, hint)
    });
    resp
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn autoscroll_holds_still_near_the_anchor() {
        assert_eq!(autoscroll_speed(0.0), 0.0);
        assert_eq!(autoscroll_speed(AUTOSCROLL_DEADZONE), 0.0);
        assert_eq!(autoscroll_speed(-AUTOSCROLL_DEADZONE), 0.0);
    }

    #[test]
    fn autoscroll_runs_the_way_the_pointer_went() {
        // Below the anchor reads forward, above it reads back.
        assert!(autoscroll_speed(AUTOSCROLL_DEADZONE + 10.0) > 0.0);
        assert!(autoscroll_speed(-AUTOSCROLL_DEADZONE - 10.0) < 0.0);
    }

    use crate::gui::layout::{typeset, Metrics, Setup};
    use crate::text::Document;
    use std::path::PathBuf;

    /// Fake font: 10pt per glyph, so the tests do not depend on what is installed.
    fn fake(s: &str) -> f32 {
        s.chars().count() as f32 * 10.0
    }

    fn book_layout_of(height: f32) -> layout::Layout {
        let body: String = (0..60)
            .map(|i| format!("Chapter {i}\n\nparagraph {i} of the book with a few words in it\n\n"))
            .collect();
        let doc = Document::from_string(body, &PathBuf::from("t.txt"), "UTF-8");
        typeset(
            &doc,
            &Setup {
                width: 300.0,
                height,
                metrics: Metrics::default(),
                justify: false,
                drop_caps: false,
                chapter_breaks: false,
                hyphenate: false,
            },
            &fake,
            &fake,
        )
    }

    fn book_layout() -> layout::Layout {
        book_layout_of(400.0)
    }

    /// The column height `relayout` hands the typesetter in Scroll mode.
    const SCROLL_HEIGHT: f32 = 1.0e9;

    #[test]
    fn scroll_mode_sets_the_whole_book_as_one_page() {
        // This is why a search hit cannot be reached by page number here, and
        // why `page` says nothing about where the reader is: there is only ever
        // one page, so `page_of_row` answers 0 for every row in the book.
        let l = book_layout_of(SCROLL_HEIGHT);
        assert_eq!(l.pages.len(), 1, "Scroll mode should hold one endless page");
        let last = l.rows.len() - 1;
        assert_eq!(l.page_of_row(last), l.page_of_row(0));
        assert!(
            l.rows[last].offset > l.rows[0].offset,
            "but the rows themselves are far apart, so the offset can carry the jump"
        );
    }

    #[test]
    fn scrolling_moves_which_rows_are_on_screen() {
        // Scroll mode sets the book as one page, so `page` cannot answer this:
        // reading `page` here left the running head and the contents drawer
        // stuck on whatever leaf the last relayout happened to name.
        let l = book_layout();
        let at = |scroll: f32| {
            visible_rows(
                &l,
                View {
                    scrolling: true,
                    scroll,
                    viewport: 400.0,
                    page: 0,
                    columns: 1,
                },
            )
        };
        let (top, bottom) = at(0.0);
        assert_eq!(top, 0);
        assert!(bottom > top, "nothing was on screen");

        let far = l.total_height() * 0.5;
        let (mid_top, mid_bottom) = at(far);
        assert!(
            mid_top > top,
            "scrolling to {far} left the top row at {mid_top}"
        );
        assert!(mid_bottom > mid_top);
        assert!(
            l.rows.get(mid_top).is_some_and(|r| r.offset > 0),
            "the row under the scroll position is still the first one"
        );
    }

    #[test]
    fn paged_modes_still_answer_with_their_leaves() {
        let l = book_layout();
        assert!(l.pages.len() > 2, "need a few pages: {}", l.pages.len());
        let spread = visible_rows(
            &l,
            View {
                scrolling: false,
                scroll: 0.0,
                viewport: 400.0,
                page: 0,
                columns: 2,
            },
        );
        assert_eq!(spread.0, l.pages[0].start);
        assert_eq!(spread.1, l.pages[1].end, "the right leaf was left out");

        let single = visible_rows(
            &l,
            View {
                scrolling: false,
                scroll: 0.0,
                viewport: 400.0,
                page: 1,
                columns: 1,
            },
        );
        assert_eq!(single, (l.pages[1].start, l.pages[1].end));
    }

    /// The symptom: in Scroll mode every chapter tick on the progress rule was
    /// drawn at the left end, one on top of the other.
    #[test]
    fn chapter_ticks_spread_along_the_rule_when_scrolling() {
        let l = book_layout_of(SCROLL_HEIGHT);
        assert_eq!(l.pages.len(), 1, "Scroll mode holds one endless page");
        assert!(l.chapter_rows.len() >= 10, "need chapters to place");
        let view = View {
            scrolling: true,
            scroll: 0.0,
            viewport: 400.0,
            page: 0,
            columns: 1,
        };
        let ts: Vec<f32> = l
            .chapter_rows
            .iter()
            .map(|&r| fraction_of_row(&l, view, true, 0, r))
            .collect();
        // The old arithmetic — page_of_row(row) / pages.len() — gave 0.0 for
        // every one of these, because there is only ever one page here.
        assert!(
            ts.iter().any(|&t| t > 0.5),
            "every tick still landed at the start of the rule: {ts:?}"
        );
        for w in ts.windows(2) {
            assert!(w[1] >= w[0], "ticks are out of order: {ts:?}");
        }
        // The last screenful of the book shares one place on the rule: the
        // knob stops at `max_scroll` too, so the chapters inside it stop with
        // it. Everything before that has to be told apart.
        let distinct = ts
            .iter()
            .map(|t| (t * 1000.0) as i32)
            .collect::<std::collections::HashSet<_>>()
            .len();
        assert!(
            distinct >= ts.len() - 3,
            "ticks piled up: {distinct} places for {} chapters",
            ts.len()
        );
        assert_eq!(ts[0], 0.0);
        assert_eq!(*ts.last().unwrap(), 1.0);
    }

    /// A tick has to land where the knob lands when the reader reaches it, in
    /// every mode — one rule, one scale.
    #[test]
    fn a_tick_is_where_the_knob_will_be() {
        let l = book_layout();
        for columns in [1usize, 2] {
            let view = View {
                scrolling: false,
                scroll: 0.0,
                viewport: 400.0,
                page: 0,
                columns,
            };
            let last = l.pages.len().saturating_sub(1) / columns * columns;
            for &row in &l.chapter_rows {
                let tick = fraction_of_row(&l, view, true, 0, row);
                // Where the reader stands after jumping to that chapter.
                let page = l.page_of_row(row) / columns * columns;
                let knob = (page.min(last) as f32) / last as f32;
                assert!(
                    (tick - knob).abs() < 1.0e-6,
                    "columns {columns}, row {row}: tick {tick} but knob {knob}"
                );
            }
        }
    }

    /// Until the book is fully set the page count keeps growing, so both the
    /// knob and the ticks measure against the document instead.
    #[test]
    fn ticks_follow_the_document_while_the_book_is_still_being_set() {
        let l = book_layout();
        let view = View {
            scrolling: false,
            scroll: 0.0,
            viewport: 400.0,
            page: 0,
            columns: 2,
        };
        let chars = l.rows.last().map(|r| r.range().1).unwrap_or(1) * 2;
        let ts: Vec<f32> = l
            .chapter_rows
            .iter()
            .map(|&r| fraction_of_row(&l, view, false, chars, r))
            .collect();
        for w in ts.windows(2) {
            assert!(w[1] >= w[0], "out of order while setting: {ts:?}");
        }
        assert!(ts.iter().all(|&t| (0.0..=1.0).contains(&t)));
    }

    fn pressed(key: Key) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    /// The symptom: writing a note worked the shortcuts at the same time.
    /// "note about ch 3" cycled the theme, opened two drawers, jumped to the
    /// next search hit — and every space in it turned a page.
    #[test]
    fn a_note_takes_the_keyboard_away_from_the_shortcuts() {
        let ctx = Context::default();
        let doc = Document::from_string(
            "Chapter 1

some text to mark here
".to_string(),
            &PathBuf::from("note.txt"),
            "UTF-8",
        );
        let mut library = Library::default();
        library.record(&doc.path).highlights.push(Highlight {
            start: 0,
            end: 4,
            ink: Ink::Yellow,
            text: "some".into(),
            note: String::new(),
            stale: false,
        });
        // Defaults save to nowhere, so this writes over nobody's library.
        let mut app = ReaderApp::with_state(&ctx, None, Settings::default(), library);
        app.doc = Some(doc);
        app.note_editor = Some((0, String::new()));

        // One frame to put the note on screen; its box asks for the keyboard.
        let _ = ctx.run_ui(egui::RawInput::default(), |_| app.note_window(&ctx));
        assert!(ctx.text_edit_focused(), "the note's box never took the keyboard");

        let before = (app.skin, app.settings.mode, app.drawer, app.page);
        let letters = egui::RawInput {
            events: vec![
                pressed(Key::T),
                pressed(Key::C),
                pressed(Key::V),
                pressed(Key::H),
                pressed(Key::N),
                pressed(Key::Num3),
                pressed(Key::Space),
                pressed(Key::Backspace),
            ],
            ..Default::default()
        };
        let _ = ctx.run_ui(letters, |_| {
            app.handle_keys(&ctx);
            app.note_window(&ctx);
        });
        assert_eq!(
            before,
            (app.skin, app.settings.mode, app.drawer, app.page),
            "the letters of the note were read as shortcuts as well"
        );
        assert!(app.note_editor.is_some(), "the note closed itself");

        // Escape is still ours: it leaves the passage as it was.
        let esc = egui::RawInput {
            events: vec![pressed(Key::Escape)],
            ..Default::default()
        };
        // egui takes the focus off the box the moment it reads the Escape, so
        // by now nothing holds the keyboard and the key arrives as an ordinary
        // shortcut. It still has to close the note and nothing else.
        app.drawer = Some(Drawer::Highlights);
        let _ = ctx.run_ui(esc, |_| {
            app.handle_keys(&ctx);
            app.note_window(&ctx);
        });
        assert!(app.note_editor.is_none(), "Escape did not close the note");
        assert_eq!(
            app.drawer,
            Some(Drawer::Highlights),
            "Escape shut the drawer as well as the note"
        );
    }

    /// With an icon face installed a toolbar button's visible label is a
    /// private-use code point, and that was the name it published to the
    /// accessibility tree. A screen reader met this window, found the text on
    /// the page — which `central` goes to some trouble to publish — and then
    /// eight controls that act on it with no names at all.
    #[test]
    fn the_toolbar_buttons_tell_a_screen_reader_what_they_are() {
        let ctx = Context::default();
        let _ = theme::install_fonts(&ctx);
        ctx.enable_accesskit();
        let hint = "Contents and bookmarks (C)";
        let mut published = None;
        // Two passes: the tree is built out of what the frame registered.
        for _ in 0..2 {
            let out = ctx.run_ui(egui::RawInput::default(), |ui| {
                tool(ui, IC_CONTENTS, hint, false);
            });
            published = out.platform_output.accesskit_update;
        }
        // Printed rather than walked: the shape of an accesskit node is that
        // crate's business and changes between its versions, but a name that is
        // in the tree is in the text of it.
        let tree = format!("{:?}", published.expect("no accessibility tree"));
        assert!(
            tree.contains(hint),
            "the button published no name a person could read:\n{tree}"
        );
        if theme::icons_available() {
            assert!(
                !tree.contains(IC_CONTENTS.glyph),
                "the button is still named after its glyph:\n{tree}"
            );
        }
    }

    /// Escape closed the search box but left the query and its hits behind,
    /// because the branch that clears them sits behind a guard Escape never
    /// reaches.
    #[test]
    fn escape_puts_the_search_away_and_takes_the_query_with_it() {
        let ctx = Context::default();
        let doc = Document::from_string(
            "the quick brown fox
".to_string(),
            &PathBuf::from("search.txt"),
            "UTF-8",
        );
        let mut app =
            ReaderApp::with_state(&ctx, None, Settings::default(), Library::default());
        app.doc = Some(doc);
        app.show_search = true;
        app.search = "quick".into();
        app.run_search(false);
        assert!(!app.matches.is_empty(), "nothing found, so nothing to clear");

        let esc = egui::RawInput {
            events: vec![pressed(Key::Escape)],
            ..Default::default()
        };
        let _ = ctx.run_ui(esc, |_| app.handle_keys(&ctx));
        assert!(!app.show_search, "the box stayed open");
        assert!(app.search.is_empty(), "the query outlived the box");
        assert!(app.matches.is_empty(), "the hits outlived the box");
    }

    /// The symptom: rolling the wheel over the highlight drawer scrolled that
    /// list *and* turned the leaf behind it.
    #[test]
    fn the_wheel_belongs_to_whatever_it_was_rolled_over() {
        let ctx = Context::default();
        let mut app =
            ReaderApp::with_state(&ctx, None, Settings::default(), Library::default());
        // The page as `central` would have left it: drawer to the left of it,
        // toolbar above.
        app.page_area = Rect::from_min_size(Pos2::new(300.0, 60.0), Vec2::new(600.0, 500.0));

        let ask = |at: Pos2| {
            let input = egui::RawInput {
                events: vec![egui::Event::PointerMoved(at)],
                ..Default::default()
            };
            let mut answer = false;
            let _ = ctx.run_ui(input, |_| answer = app.pointer_on_the_page(&ctx));
            answer
        };
        assert!(ask(Pos2::new(600.0, 300.0)), "the middle of the page was not the page");
        assert!(!ask(Pos2::new(100.0, 300.0)), "the drawer counted as the page");
        assert!(!ask(Pos2::new(600.0, 20.0)), "the toolbar counted as the page");
        assert!(!ask(Pos2::new(600.0, 590.0)), "the footer counted as the page");

        // A window sits above the page and keeps what is rolled onto it. It
        // takes two frames: an area's rect is known from the frame before.
        let over_the_settings = Pos2::new(600.0, 300.0);
        let mut on_the_page = true;
        for pass in 0..2 {
            let input = egui::RawInput {
                events: vec![egui::Event::PointerMoved(over_the_settings)],
                ..Default::default()
            };
            let _ = ctx.run_ui(input, |_| {
                egui::Window::new("Reading")
                    .fixed_pos(Pos2::new(540.0, 240.0))
                    .fixed_size(Vec2::new(160.0, 160.0))
                    .show(&ctx, |ui| {
                        ui.label("size");
                    });
                if pass == 1 {
                    on_the_page = app.pointer_on_the_page(&ctx);
                }
            });
        }
        assert!(!on_the_page, "a window over the page did not keep the wheel");
    }

    /// One lock on the font atlas for a whole row, not one per glyph — and the
    /// same numbers either way. Every position the selection, the highlighter
    /// and the search wash are drawn at comes out of this.
    #[test]
    fn batching_the_font_lock_did_not_move_a_single_glyph() {
        let ctx = Context::default();
        let faces = theme::install_fonts(&ctx);
        let _ = ctx.run_ui(egui::RawInput::default(), |_| {});
        if faces.is_empty() {
            eprintln!("no faces on this machine; skipping");
            return;
        }
        let font = serif(18.0);
        for text in [
            "Call me Ishmael, and mind the gap.",
            "읽지 않은 책이 쌓인 서가",
            "Grüße — café naïve",
            // Decomposed: `e` followed by a combining acute, which is drawn on
            // the letter before it and moves nothing along.
            "cafe\u{301} nai\u{308}ve",
            "",
            " ",
        ] {
            for fill in [
                layout::Stretch::None,
                layout::Stretch::WordGaps(3.5),
                layout::Stretch::Letters(1.25),
            ] {
                let got = char_positions(&ctx, &font, text, fill);
                // The old shape: measure one character at a time.
                let mut want = Vec::with_capacity(text.chars().count() + 1);
                let mut x = 0.0f32;
                for c in text.chars() {
                    want.push(x);
                    x += measure(&ctx, &font, &c.to_string());
                    match fill {
                        layout::Stretch::WordGaps(extra) if c == ' ' => x += extra,
                        layout::Stretch::Letters(extra) => x += extra,
                        _ => {}
                    }
                }
                want.push(x);
                assert_eq!(got, want, "{text:?} under {fill:?}");
            }
        }
    }

    #[test]
    fn autoscroll_speeds_up_with_distance_but_is_capped() {
        let near = autoscroll_speed(AUTOSCROLL_DEADZONE + 20.0);
        let far = autoscroll_speed(AUTOSCROLL_DEADZONE + 200.0);
        assert!(far > near * 2.0, "speed should grow with distance");
        assert_eq!(autoscroll_speed(100_000.0), 2400.0, "a flick must not run away");
        assert_eq!(autoscroll_speed(-100_000.0), -2400.0);
    }

}

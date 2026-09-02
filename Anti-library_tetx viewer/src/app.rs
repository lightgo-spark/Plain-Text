//! Reader state machine. Everything here is pure logic so it can be tested
//! without a terminal attached.

use anti_library::library::{Bookmark, Library};
use anti_library::text::{Document, Line, Match};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Reading,
    Contents,
    Bookmarks,
    Search,
    Help,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Paper,
    Sepia,
    Night,
    Ink,
}

impl Theme {
    pub fn next(self) -> Theme {
        match self {
            Theme::Paper => Theme::Sepia,
            Theme::Sepia => Theme::Night,
            Theme::Night => Theme::Ink,
            Theme::Ink => Theme::Paper,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Theme::Paper => "Paper",
            Theme::Sepia => "Sepia",
            Theme::Night => "Night",
            Theme::Ink => "Ink",
        }
    }
}

/// Narrowest terminal that still gets a two-page spread.
pub const MIN_SPREAD_WIDTH: usize = 84;
/// Cells reserved for the spine between the two pages.
pub const SPINE_WIDTH: usize = 3;

pub struct App {
    pub doc: Document,
    pub lines: Vec<Line>,
    pub top: usize,
    pub rows: usize,
    pub cols: usize,
    pub col_width: usize,
    pub mode: Mode,
    pub theme: Theme,
    pub indent: bool,
    pub two_page: bool,
    pub max_text_width: usize,
    pub status: Option<String>,
    pub query: String,
    /// Where the query occurs in the *document*. Character ranges, not line
    /// numbers: the terminal reader rewraps on every resize, and a hit found
    /// on a line stops being true the moment the window changes width.
    pub matches: Vec<Match>,
    pub match_cursor: usize,
    pub list_cursor: usize,
    pub library: Library,
    pub should_quit: bool,
}

impl App {
    pub fn new(doc: Document, library: Library) -> App {
        let saved = library.get(&doc.path).map(|r| r.offset).unwrap_or(0);
        let mut app = App {
            lines: doc.wrap(64, true),
            doc,
            top: 0,
            rows: 20,
            cols: 1,
            col_width: 64,
            mode: Mode::Reading,
            theme: Theme::Paper,
            indent: true,
            two_page: true,
            max_text_width: 72,
            status: None,
            query: String::new(),
            matches: Vec::new(),
            match_cursor: 0,
            list_cursor: 0,
            library,
            should_quit: false,
        };
        app.seek_offset(saved);
        app
    }

    // ---- layout ---------------------------------------------------------

    /// Recompute the wrapped text for a screen area, choosing the column count
    /// itself. Used by tests; the UI calls [`App::relayout_cols`] with the
    /// widths the layout actually produced.
    #[cfg(test)]
    pub fn relayout(&mut self, area_width: usize, area_height: usize) {
        let cols = if self.two_page && area_width >= MIN_SPREAD_WIDTH {
            2
        } else {
            1
        };
        let usable = area_width.saturating_sub(SPINE_WIDTH * (cols - 1));
        self.relayout_cols(cols, usable / cols, area_height);
    }

    /// Recompute the wrapped text. `col_width` is the width of one rendered
    /// column in cells — never wider, or the last glyph of a line is clipped.
    /// Keeps the reader on the same character offset so resizing never loses
    /// the reader's place.
    pub fn relayout_cols(&mut self, cols: usize, col_width: usize, rows: usize) {
        let anchor = self.current_offset();
        let cols = cols.max(1);
        let col_width = col_width.min(self.max_text_width).max(8);
        if cols != self.cols || col_width != self.col_width {
            self.lines = self.doc.wrap(col_width, self.indent);
            self.cols = cols;
            self.col_width = col_width;
            if !self.query.is_empty() {
                self.recompute_matches();
            }
        }
        self.rows = rows.max(1);
        self.seek_offset(anchor);
    }

    pub fn page_lines(&self) -> usize {
        (self.rows * self.cols).max(1)
    }

    /// Start of the last page. Page starts are always multiples of the page
    /// size, so a relayout can never bump the reader off a page boundary.
    pub fn max_top(&self) -> usize {
        let page = self.page_lines();
        self.lines.len().saturating_sub(1) / page * page
    }

    pub fn current_offset(&self) -> usize {
        self.lines.get(self.top).map(|l| l.offset).unwrap_or(0)
    }

    /// Move to the first page whose start is at or before `offset`.
    pub fn seek_offset(&mut self, offset: usize) {
        let idx = match self.lines.binary_search_by(|l| l.offset.cmp(&offset)) {
            Ok(i) => i,
            Err(0) => 0,
            Err(i) => i - 1,
        };
        // Snap to a page boundary so pages stay stable while turning.
        let page = self.page_lines();
        self.top = (idx / page) * page;
        self.clamp();
    }

    fn clamp(&mut self) {
        self.top = self.top.min(self.max_top());
    }

    // ---- navigation -----------------------------------------------------

    pub fn next_page(&mut self) {
        let page = self.page_lines();
        if self.top + page <= self.max_top() {
            self.top += page;
        } else if self.top < self.max_top() {
            self.top = self.max_top();
        } else {
            self.status = Some("End of book".into());
        }
    }

    pub fn prev_page(&mut self) {
        let page = self.page_lines();
        if self.top == 0 {
            self.status = Some("Beginning of book".into());
        }
        self.top = self.top.saturating_sub(page);
    }

    pub fn scroll(&mut self, delta: isize) {
        let t = self.top as isize + delta;
        self.top = t.max(0) as usize;
        self.clamp();
    }

    pub fn go_start(&mut self) {
        self.top = 0;
    }

    pub fn go_end(&mut self) {
        self.top = self.max_top();
    }

    /// Jump to a percentage (0..=100) of the book.
    pub fn go_percent(&mut self, pct: usize) {
        let page = self.page_lines();
        let target = self.lines.len() * pct.min(100) / 100;
        self.top = (target / page) * page;
        self.clamp();
    }

    pub fn progress(&self) -> f64 {
        let last = self.max_top();
        if last == 0 {
            return 1.0;
        }
        (self.top as f64 / last as f64).clamp(0.0, 1.0)
    }

    pub fn page_number(&self) -> usize {
        self.top / self.page_lines() + 1
    }

    pub fn total_pages(&self) -> usize {
        self.lines.len().div_ceil(self.page_lines())
    }

    // ---- search ---------------------------------------------------------

    /// Find the query in the document.
    ///
    /// This used to search the wrapped lines, which meant the reader was told
    /// there was no match whenever the wrap happened to fall inside the phrase
    /// — and in Korean, which wraps between glyphs, inside a single word. The
    /// answer now depends on the book alone, not on how wide the terminal is.
    pub fn recompute_matches(&mut self) {
        self.matches = self.doc.search(&self.query);
        self.match_cursor = 0;
    }

    /// The line the match at `index` sits on.
    fn line_of_match(&self, index: usize) -> usize {
        let Some(m) = self.matches.get(index) else {
            return 0;
        };
        match self.lines.binary_search_by(|l| l.offset.cmp(&m.start)) {
            Ok(i) => i,
            Err(0) => 0,
            Err(i) => i - 1,
        }
    }

    pub fn submit_search(&mut self) {
        self.recompute_matches();
        if self.matches.is_empty() {
            self.status = Some(format!("No match for \"{}\"", self.query));
        } else {
            // Start from the match nearest to the current page.
            let here = self.current_offset();
            self.match_cursor = self
                .matches
                .iter()
                .position(|m| m.start >= here)
                .unwrap_or(0);
            self.jump_to_match();
        }
    }

    pub fn next_match(&mut self) {
        if self.matches.is_empty() {
            self.status = Some("Nothing to search for".into());
            return;
        }
        self.match_cursor = (self.match_cursor + 1) % self.matches.len();
        self.jump_to_match();
    }

    pub fn prev_match(&mut self) {
        if self.matches.is_empty() {
            self.status = Some("Nothing to search for".into());
            return;
        }
        self.match_cursor = (self.match_cursor + self.matches.len() - 1) % self.matches.len();
        self.jump_to_match();
    }

    fn jump_to_match(&mut self) {
        let line = self.line_of_match(self.match_cursor);
        let page = self.page_lines();
        self.top = (line / page) * page;
        self.clamp();
        self.status = Some(format!(
            "Match {}/{} for \"{}\"",
            self.match_cursor + 1,
            self.matches.len(),
            self.query
        ));
    }

    /// The character range the current page covers.
    fn page_span(&self) -> (usize, usize) {
        let end_line = (self.top + self.page_lines()).min(self.lines.len());
        let start = self.lines.get(self.top).map(|l| l.offset).unwrap_or(0);
        let end = self
            .lines
            .get(end_line)
            .map(|l| l.offset)
            .unwrap_or(self.doc.chars);
        (start, end.max(start))
    }

    /// The matches that fall on the current page, as document ranges.
    pub fn visible_matches(&self) -> &[Match] {
        let (start, end) = self.page_span();
        anti_library::text::matches_in(&self.matches, start, end)
    }

    // ---- bookmarks ------------------------------------------------------

    pub fn toggle_bookmark(&mut self) {
        let offset = self.current_offset();
        let label = self
            .lines
            .iter()
            .skip(self.top)
            .find(|l| !l.blank)
            .map(|l| l.text.trim().chars().take(48).collect::<String>())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| format!("offset {offset}"));
        let key = self.doc.path.clone();
        let rec = self.library.record(&key);
        if let Some(pos) = rec.bookmarks.iter().position(|b| b.offset == offset) {
            rec.bookmarks.remove(pos);
            self.status = Some("Bookmark removed".into());
        } else {
            rec.bookmarks.push(Bookmark { offset, label });
            rec.bookmarks.sort_by_key(|b| b.offset);
            self.status = Some("Bookmark added".into());
        }
        self.persist();
    }

    pub fn bookmarks(&self) -> Vec<Bookmark> {
        self.library
            .get(&self.doc.path)
            .map(|r| r.bookmarks.clone())
            .unwrap_or_default()
    }

    pub fn delete_bookmark(&mut self, index: usize) {
        let key = self.doc.path.clone();
        let rec = self.library.record(&key);
        if index < rec.bookmarks.len() {
            rec.bookmarks.remove(index);
            self.status = Some("Bookmark deleted".into());
            self.persist();
        }
        let len = self.bookmarks().len();
        self.list_cursor = self.list_cursor.min(len.saturating_sub(1));
    }

    pub fn persist(&mut self) {
        let offset = self.current_offset();
        let key = self.doc.path.clone();
        let title = self.doc.title.clone();
        let rec = self.library.record(&key);
        rec.offset = offset;
        rec.title = title;
        // Both readers share the library, so both keep the "last opened" stamp
        // the desktop start screen sorts by.
        rec.last_opened = anti_library::library::now();
        if let Err(e) = self.library.save() {
            self.status = Some(format!("Could not save progress: {e}"));
        }
    }

    // ---- overlays -------------------------------------------------------

    pub fn open(&mut self, mode: Mode) {
        self.mode = mode;
        self.list_cursor = match mode {
            Mode::Contents => self
                .doc
                .chapters
                .iter()
                .rposition(|c| c.offset <= self.current_offset())
                .unwrap_or(0),
            _ => 0,
        };
    }

    pub fn list_len(&self) -> usize {
        match self.mode {
            Mode::Contents => self.doc.chapters.len(),
            Mode::Bookmarks => self.bookmarks().len(),
            _ => 0,
        }
    }

    pub fn move_cursor(&mut self, delta: isize) {
        let len = self.list_len();
        if len == 0 {
            return;
        }
        let c = (self.list_cursor as isize + delta).clamp(0, len as isize - 1);
        self.list_cursor = c as usize;
    }

    /// Activate the selected entry of the open overlay.
    pub fn activate(&mut self) {
        match self.mode {
            Mode::Contents => {
                if let Some(ch) = self.doc.chapters.get(self.list_cursor) {
                    let offset = ch.offset;
                    let title = ch.title.clone();
                    self.seek_offset(offset);
                    self.status = Some(format!("Jumped to \"{title}\""));
                }
            }
            Mode::Bookmarks => {
                if let Some(b) = self.bookmarks().get(self.list_cursor) {
                    let (offset, label) = (b.offset, b.label.clone());
                    self.seek_offset(offset);
                    self.status = Some(format!("Jumped to \"{label}\""));
                }
            }
            _ => {}
        }
        self.mode = Mode::Reading;
    }

    pub fn toggle_two_page(&mut self) {
        self.two_page = !self.two_page;
        self.status = Some(if self.two_page {
            "Two-page spread".into()
        } else {
            "Single page".into()
        });
        // Force a rebuild on the next relayout.
        self.col_width = 0;
    }

    pub fn toggle_indent(&mut self) {
        // Rewrapping renumbers every line, so the reader's place has to be
        // taken as a character offset before the old line list is thrown away
        // — and the search hits, which are line numbers, have to be found again.
        let anchor = self.current_offset();
        self.indent = !self.indent;
        self.lines = self.doc.wrap(self.col_width.max(8), self.indent);
        if !self.query.is_empty() {
            self.recompute_matches();
        }
        self.seek_offset(anchor);
        self.status = Some(if self.indent {
            "Paragraph indent on".into()
        } else {
            "Paragraph indent off".into()
        });
    }

    pub fn widen(&mut self, delta: isize) {
        let w = (self.max_text_width as isize + delta).clamp(30, 160) as usize;
        self.max_text_width = w;
        self.col_width = 0; // force rebuild
        self.status = Some(format!("Text width {w}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn app_with(text: &str) -> App {
        let doc = Document::from_string(
            text.to_string(),
            &PathBuf::from("mem.txt"),
            "UTF-8",
        );
        let mut app = App::new(doc, Library::default());
        app.relayout(60, 10);
        app
    }

    fn long_book() -> App {
        let body: String = (0..200)
            .map(|i| format!("line number {i} with some words\n"))
            .collect();
        app_with(&body)
    }

    #[test]
    fn every_page_start_is_page_aligned() {
        let mut app = long_book();
        for _ in 0..100 {
            app.next_page();
            assert_eq!(app.top % app.page_lines(), 0);
        }
        app.go_end();
        assert_eq!(app.top % app.page_lines(), 0);
    }

    #[test]
    fn relayout_at_the_end_of_the_book_is_stable() {
        let mut app = long_book();
        app.go_end();
        let at = app.top;
        for _ in 0..3 {
            app.relayout(60, 10);
            assert_eq!(app.top, at, "relayout moved the last page");
        }
    }

    #[test]
    fn paging_stops_at_the_end() {
        let mut app = long_book();
        for _ in 0..1000 {
            app.next_page();
        }
        assert_eq!(app.top, app.max_top());
        assert!(app.progress() >= 1.0);
    }

    #[test]
    fn paging_stops_at_the_start() {
        let mut app = long_book();
        app.next_page();
        for _ in 0..50 {
            app.prev_page();
        }
        assert_eq!(app.top, 0);
    }

    #[test]
    fn pages_are_aligned_so_turning_is_reversible() {
        let mut app = long_book();
        app.next_page();
        app.next_page();
        let at = app.top;
        app.prev_page();
        app.next_page();
        assert_eq!(app.top, at);
    }

    #[test]
    fn resize_keeps_the_reader_in_place() {
        let mut app = long_book();
        app.next_page();
        app.next_page();
        let offset = app.current_offset();
        app.relayout(100, 25);
        // The line the reader was on is still somewhere on the visible page.
        let idx = app
            .lines
            .iter()
            .rposition(|l| l.offset <= offset)
            .unwrap();
        assert!(
            idx >= app.top && idx < app.top + app.page_lines(),
            "line {idx} left the page {}..{}",
            app.top,
            app.top + app.page_lines()
        );
    }

    #[test]
    fn search_finds_and_cycles_matches() {
        let mut app = long_book();
        app.query = "number 42".into();
        app.submit_search();
        assert_eq!(app.matches.len(), 1);
        assert_eq!(app.visible_matches().len(), 1, "the hit is not on the page");
        let m = app.matches[0];
        assert_eq!(app.doc.slice(m.start, m.end), "number 42");
        app.next_match();
        assert_eq!(app.match_cursor, 0); // wraps around a single match
    }

    #[test]
    fn search_is_case_insensitive() {
        let mut app = app_with("The Raven\n\nQuoth the raven");
        app.query = "RAVEN".into();
        app.submit_search();
        assert_eq!(app.matches.len(), 2);
    }

    #[test]
    fn failed_search_reports_and_does_not_move() {
        let mut app = long_book();
        app.next_page();
        let at = app.top;
        app.query = "zzzz-not-here".into();
        app.submit_search();
        assert_eq!(app.top, at);
        assert!(app.status.as_deref().unwrap().contains("No match"));
    }

    #[test]
    fn bookmark_toggles_off_at_the_same_place() {
        let mut app = long_book();
        app.next_page();
        app.toggle_bookmark();
        assert_eq!(app.bookmarks().len(), 1);
        app.toggle_bookmark();
        assert_eq!(app.bookmarks().len(), 0);
    }

    #[test]
    fn bookmark_jump_returns_to_the_same_page() {
        let mut app = long_book();
        app.next_page();
        app.next_page();
        let at = app.top;
        app.toggle_bookmark();
        app.go_start();
        app.open(Mode::Bookmarks);
        app.activate();
        assert_eq!(app.top, at);
    }

    #[test]
    fn contents_jump_selects_the_current_chapter() {
        let mut app = app_with("Chapter 1\n\naaa\n\nChapter 2\n\nbbb\n\nChapter 3\n\nccc");
        app.go_end();
        app.open(Mode::Contents);
        assert_eq!(app.doc.chapters.len(), 3);
        app.activate();
        assert_eq!(app.mode, Mode::Reading);
    }

    #[test]
    fn percent_jump_is_bounded() {
        let mut app = long_book();
        app.go_percent(150);
        assert!(app.top <= app.max_top());
        app.go_percent(0);
        assert_eq!(app.top, 0);
    }

    #[test]
    fn empty_document_does_not_panic() {
        let mut app = app_with("");
        app.next_page();
        app.prev_page();
        app.go_end();
        app.toggle_bookmark();
        assert_eq!(app.total_pages(), 1);
    }

    #[test]
    fn narrow_terminal_falls_back_to_one_column() {
        let mut app = long_book();
        app.relayout(50, 10);
        assert_eq!(app.cols, 1);
        app.relayout(160, 10);
        assert_eq!(app.cols, 2);
    }

    #[test]
    fn cursor_stays_in_range_when_list_is_empty() {
        let mut app = long_book();
        app.open(Mode::Bookmarks);
        app.move_cursor(5);
        assert_eq!(app.list_cursor, 0);
        app.activate(); // must not panic
    }

    #[test]
    fn saved_progress_is_restored() {
        let doc = Document::from_string(
            (0..200)
                .map(|i| format!("line {i}\n"))
                .collect::<String>(),
            &PathBuf::from("mem.txt"),
            "UTF-8",
        );
        let mut lib = Library::default();
        lib.record("mem.txt").offset = 500;
        let mut app = App::new(doc, lib);
        app.relayout(60, 10);
        assert!(app.top > 0);
        assert!(app.current_offset() <= 500);
    }

    #[test]
    fn toggling_the_indent_keeps_the_reader_in_place() {
        // Paragraphs exactly the width of the measure: with the indent on they
        // need two rows, without it one, so the line numbering really moves.
        let line = "aa bb cc dd ee ff gg hh ii jj kk ll mm nn oo pp qq rr ss";
        let body: String = (0..60).map(|i| format!("p{i:02} {line}\n\n")).collect();
        let mut app = app_with(&body);
        app.query = "p40".into();
        app.submit_search();
        let offset = app.current_offset();
        let matched = app.matches[0];

        app.toggle_indent();
        app.relayout(60, 10);
        assert!(
            app.current_offset().abs_diff(offset) < 200,
            "the indent toggle moved the reader from {offset} to {}",
            app.current_offset()
        );
        assert_eq!(app.matches.len(), 1);
        // The hit is a place in the book, so rewrapping cannot move it.
        assert_eq!(app.matches[0], matched, "the search hit moved with the wrap");
        let m = app.matches[0];
        assert_eq!(app.doc.slice(m.start, m.end), "p40");
    }

    #[test]
    fn a_phrase_broken_across_two_lines_is_still_found() {
        // The defect: the terminal reader searched its wrapped lines, so the
        // answer changed with the width of the window.
        let mut app = app_with("the quick brown fox jumps over the lazy dog\n");
        app.query = "quick brown".into();
        for width in [14usize, 20, 30, 44, 80] {
            app.relayout(width, 10);
            app.submit_search();
            assert_eq!(app.matches.len(), 1, "not found at width {width}");
            let m = app.matches[0];
            assert_eq!(app.doc.slice(m.start, m.end), "quick brown");
        }
    }

    #[test]
    fn a_korean_word_split_by_the_wrap_is_still_found() {
        let mut app = app_with("읽지 않은 책이 쌓인 서가를 안티라이브러리라 부른다.\n");
        app.query = "안티라이브러리".into();
        for width in [12usize, 16, 24, 40] {
            app.relayout(width, 10);
            app.submit_search();
            assert_eq!(app.matches.len(), 1, "not found at width {width}");
            let m = app.matches[0];
            assert_eq!(app.doc.slice(m.start, m.end), "안티라이브러리");
        }
    }

    #[test]
    fn width_setting_is_clamped() {
        let mut app = long_book();
        app.widen(-1000);
        assert_eq!(app.max_text_width, 30);
        app.widen(1000);
        assert_eq!(app.max_text_width, 160);
    }
}

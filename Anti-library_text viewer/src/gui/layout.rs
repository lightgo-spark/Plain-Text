//! Pixel-accurate typesetting: paragraphs become rows, rows become pages.
//!
//! The terminal reader wraps to character cells; a proportional font needs real
//! measurements, so everything here takes a `measure` closure. That also makes
//! the whole module testable without a window: the tests use a fake font where
//! every latin glyph is 10px and every wide glyph 20px.

use crate::text::Document;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// How a row should be painted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
    Body,
    Heading,
    Blank,
}

#[derive(Debug, Clone)]
pub struct Row {
    pub text: String,
    /// Character offset of this row inside the document.
    pub offset: usize,
    /// Left inset in points (paragraph indent, or the well of a drop cap).
    pub indent: f32,
    /// Height of the row in points.
    pub height: f32,
    pub kind: RowKind,
    /// True when the row may be stretched to the full column width.
    pub justify: bool,
    /// Set on the first row of a paragraph that opens with a drop cap.
    pub drop_cap: Option<String>,
    /// Index of the chapter this row belongs to, if any.
    pub chapter: Option<usize>,
    /// True when this row starts a chapter.
    pub chapter_start: bool,
    /// The row ends inside a word and takes a hyphen.
    ///
    /// The hyphen is drawn, never stored: `text` stays exactly what the
    /// document holds at `offset`, so a selection or a highlight made on this
    /// row still lands on the right characters.
    pub hyphen: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct Metrics {
    pub body_height: f32,
    pub heading_height: f32,
    pub blank_height: f32,
    /// Width of the first-line paragraph indent.
    pub indent: f32,
    /// Height of a drop cap, in body rows (0 disables it).
    pub drop_cap_rows: usize,
    /// Type size of the drop cap, relative to the body size.
    pub drop_cap_scale: f32,
    /// Air between the cap and the text wrapped around it.
    pub drop_cap_gap: f32,
}

/// How much of the measure a drop cap's well may take before the paragraph is
/// set plainly instead.
///
/// Half is already generous — an ordinary cap at 18pt in a 620pt column takes
/// about 8% — and the point of the ceiling is the other end: large type in a
/// narrow column, where the well can be wider than the column and there is
/// nothing left to set beside it.
const MAX_DROP_CAP_SHARE: f32 = 0.5;

impl Default for Metrics {
    fn default() -> Self {
        Metrics {
            body_height: 26.0,
            heading_height: 44.0,
            blank_height: 13.0,
            indent: 24.0,
            drop_cap_rows: 3,
            drop_cap_scale: 2.9,
            drop_cap_gap: 6.0,
        }
    }
}

/// A page is a slice of the row list plus the height it actually occupies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Page {
    pub start: usize,
    pub end: usize,
}

impl Page {
    pub fn len(&self) -> usize {
        self.end - self.start
    }

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }
}

/// The typeset document: rows, and the pages they were broken into.
#[derive(Debug, Clone, Default)]
pub struct Layout {
    pub rows: Vec<Row>,
    pub pages: Vec<Page>,
    /// Row index at which each chapter starts.
    pub chapter_rows: Vec<usize>,
    /// Distance from the top of the text to the top of each row, plus the
    /// total height as a final entry. Continuous scrolling reads this.
    pub tops: Vec<f32>,
}

impl Layout {
    /// Page containing a row.
    pub fn page_of_row(&self, row: usize) -> usize {
        match self.pages.binary_search_by(|p| {
            if p.end <= row {
                std::cmp::Ordering::Less
            } else if p.start > row {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        }) {
            Ok(i) => i,
            Err(i) => i.min(self.pages.len().saturating_sub(1)),
        }
    }

    /// First row at or before `offset`.
    pub fn row_of_offset(&self, offset: usize) -> usize {
        match self.rows.binary_search_by(|r| r.offset.cmp(&offset)) {
            Ok(i) => i,
            Err(0) => 0,
            Err(i) => i - 1,
        }
    }

    pub fn page_of_offset(&self, offset: usize) -> usize {
        self.page_of_row(self.row_of_offset(offset))
    }

    /// Height of the whole text when set as one continuous column.
    pub fn total_height(&self) -> f32 {
        self.tops.last().copied().unwrap_or(0.0)
    }

    /// First row visible at scroll position `y`.
    pub fn row_at_height(&self, y: f32) -> usize {
        match self
            .tops
            .binary_search_by(|t| t.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal))
        {
            Ok(i) => i,
            Err(0) => 0,
            Err(i) => (i - 1).min(self.rows.len().saturating_sub(1)),
        }
    }

    pub fn offset_of_page(&self, page: usize) -> usize {
        self.pages
            .get(page)
            .and_then(|p| self.rows.get(p.start))
            .map(|r| r.offset)
            .unwrap_or(0)
    }
}

impl Row {
    /// Number of characters the row holds.
    pub fn chars(&self) -> usize {
        self.text.chars().count()
    }

    /// Character range of the row inside the document, end exclusive.
    pub fn range(&self) -> (usize, usize) {
        (self.offset, self.offset + self.chars())
    }

    /// The part of this row that falls inside `start..end`, as a character
    /// range within the row.
    pub fn clip(&self, start: usize, end: usize) -> Option<(usize, usize)> {
        let (a, b) = self.range();
        let from = start.max(a);
        let to = end.min(b);
        if from >= to {
            None
        } else {
            Some((from - a, to - a))
        }
    }
}

/// Rebuild the document text for a character range.
///
/// Rows carry their document offset, so the gap between one row and the next
/// says what the wrap swallowed: nothing for Korean (which breaks between
/// glyphs), one space for latin prose, and a blank row for a new paragraph.
pub fn extract(rows: &[Row], start: usize, end: usize) -> String {
    let mut out = String::new();
    let mut prev_end: Option<usize> = None;
    let mut blank_between = false;
    for row in rows {
        if row.kind == RowKind::Blank {
            if prev_end.is_some() && row.offset >= start && row.offset < end {
                blank_between = true;
            }
            continue;
        }
        let Some((from, to)) = row.clip(start, end) else {
            continue;
        };
        let piece: String = row.text.chars().skip(from).take(to - from).collect();
        if let Some(pe) = prev_end {
            if blank_between {
                out.push_str("\n\n");
            } else if row.offset + from > pe {
                out.push(' ');
            }
        }
        out.push_str(&piece);
        prev_end = Some(row.offset + to);
        blank_between = false;
    }
    out
}

/// Everything the typesetter needs besides the text itself.
#[derive(Debug, Clone, Copy)]
pub struct Setup {
    /// Column width in points.
    pub width: f32,
    /// Column height in points.
    pub height: f32,
    pub metrics: Metrics,
    pub justify: bool,
    pub drop_caps: bool,
    /// Start every chapter on a fresh page.
    pub chapter_breaks: bool,
    /// Break words at their syllables when it makes the column read better.
    pub hyphenate: bool,
}

/// Sets a document a few paragraphs at a time.
///
/// Setting a 20 MB book in one go costs the better part of a second, which the
/// reader would spend staring at an empty window. The typesetter instead runs
/// on a time budget: each frame it sets as much as fits in a few milliseconds,
/// so the first page appears immediately and the rest of the book catches up
/// while the reader is still on page one.
pub struct Typesetter {
    setup: Setup,
    next_para: usize,
    chapter_idx: Option<usize>,
    next_chapter: usize,
    /// Row index from which pages still have to be recomputed.
    paged_from: usize,
    pub layout: Layout,
    pub done: bool,
}

impl Typesetter {
    pub fn new(setup: Setup) -> Typesetter {
        Typesetter {
            setup,
            next_para: 0,
            chapter_idx: None,
            next_chapter: 0,
            paged_from: 0,
            layout: Layout::default(),
            done: false,
        }
    }

    /// Set at most `paragraphs` more paragraphs. Returns true when finished.
    pub fn step(
        &mut self,
        doc: &Document,
        paragraphs: usize,
        measure: &dyn Fn(&str) -> f32,
        measure_heading: &dyn Fn(&str) -> f32,
    ) -> bool {
        if self.done {
            return true;
        }
        let until = (self.next_para + paragraphs).min(doc.paragraphs.len());
        while self.next_para < until {
            self.set_paragraph(doc, self.next_para, measure, measure_heading);
            self.next_para += 1;
        }
        if self.next_para >= doc.paragraphs.len() {
            if self.layout.rows.is_empty() {
                self.layout.rows.push(blank_row(&self.setup.metrics, 0));
            }
            self.done = true;
        }
        self.repaginate();
        self.done
    }

    /// Index of the next paragraph to be set.
    pub fn next_paragraph(&self) -> usize {
        self.next_para
    }

    /// Fraction of the document already set, 0..=1.
    pub fn progress(&self, doc: &Document) -> f32 {
        if doc.paragraphs.is_empty() {
            return 1.0;
        }
        (self.next_para as f32 / doc.paragraphs.len() as f32).clamp(0.0, 1.0)
    }

    /// Character offset of the last paragraph set so far.
    pub fn set_upto(&self, doc: &Document) -> usize {
        doc.paragraphs
            .get(self.next_para)
            .map(|p| p.offset)
            .unwrap_or(doc.chars)
    }

    fn set_paragraph(
        &mut self,
        doc: &Document,
        index: usize,
        measure: &dyn Fn(&str) -> f32,
        measure_heading: &dyn Fn(&str) -> f32,
    ) {
        let p = &doc.paragraphs[index];
        let m = &self.setup.metrics;
        let width = self.setup.width.max(40.0);
        let rows = &mut self.layout.rows;

        let starts_chapter = doc
            .chapters
            .get(self.next_chapter)
            .is_some_and(|c| c.offset == p.offset && p.is_heading);
        if starts_chapter {
            self.chapter_idx = Some(self.next_chapter);
            self.next_chapter += 1;
            self.layout.chapter_rows.push(rows.len());
        }

        if p.is_blank {
            rows.push(Row {
                chapter: self.chapter_idx,
                ..blank_row(m, p.offset)
            });
            return;
        }

        if p.is_heading {
            // The markers are not part of the title, but they *are* part of the
            // line, and the rows have to point past them. Without this a
            // heading written `# Third` put every selection, highlight and
            // search wash on it two characters to the left.
            let raw = p.text.as_str();
            let body = raw.trim_start().trim_start_matches('#').trim_start();
            let dropped = raw[..raw.len() - body.len()].chars().count();
            let title = body.trim_end();
            // The same breaker the body goes through — headings were left on
            // the old line-at-a-time one and quietly set to a different
            // standard. Not hyphenated: a chapter title broken by a dash is
            // not a chapter title anybody wants.
            let lines = super::linebreak::break_lines(
                title,
                &|_| width.max(1.0),
                measure_heading,
                false,
            );
            for (i, line) in lines.into_iter().enumerate() {
                rows.push(Row {
                    offset: p.offset + line.offset + dropped,
                    text: line.text,
                    indent: 0.0,
                    height: m.heading_height,
                    kind: RowKind::Heading,
                    justify: false,
                    drop_cap: None,
                    chapter: self.chapter_idx,
                    chapter_start: starts_chapter && i == 0,
                    hyphen: false,
                });
            }
            return;
        }

        // A drop cap opens the first paragraph after a chapter title.
        let opens_chapter = self.setup.drop_caps
            && rows
                .iter()
                .rev()
                .find(|r| r.kind != RowKind::Blank)
                .is_some_and(|r| r.kind == RowKind::Heading);
        let mut body = p.text.trim_start().to_string();
        let mut cap = None;
        let mut insets: Vec<f32> = Vec::new();
        if opens_chapter && m.drop_cap_rows > 0 {
            if let Some(first) = body.graphemes(true).next().map(str::to_string) {
                // The well has to match the cap that will be painted into it,
                // and a Hangul cap is far wider than a latin one.
                let well = measure(&first) * m.drop_cap_scale + m.drop_cap_gap;
                // ...and it has to leave a column to set beside it. At large
                // type in a narrow measure the well came out wider than the
                // whole column; the breaker was then given `(width - well).max(1.0)`,
                // force-split a glyph into that one point, and the row painted
                // 196.8pt into a 140pt column. A cap with no room beside it is
                // not a cap, so the paragraph opens plainly instead.
                if well <= width * MAX_DROP_CAP_SHARE {
                    body = body[first.len()..].trim_start().to_string();
                    insets = vec![well; m.drop_cap_rows];
                    cap = Some(first);
                }
            }
        }
        if insets.is_empty() {
            insets = vec![m.indent];
        }

        let inset_at = |i: usize| insets.get(i).copied().unwrap_or(0.0);
        let wrapped = super::linebreak::break_lines(
            &body,
            &|i| (width - inset_at(i)).max(1.0),
            measure,
            self.setup.hyphenate,
        );
        let last = wrapped.len().saturating_sub(1);
        // `body` had the cap (and any leading space) removed, so every row of
        // this paragraph is that much further into the document. Getting this
        // wrong shifts every selection made in a chapter's first paragraph.
        let cap_offset = p.text.chars().count() - body.chars().count();
        for (i, line) in wrapped.into_iter().enumerate() {
            rows.push(Row {
                offset: p.offset + line.offset + cap_offset,
                text: line.text,
                indent: inset_at(i),
                height: m.body_height,
                kind: RowKind::Body,
                justify: self.setup.justify && i != last,
                drop_cap: if i == 0 { cap.take() } else { None },
                chapter: self.chapter_idx,
                chapter_start: false,
                hyphen: line.hyphen,
            });
        }
    }

    /// Rebuild the pages that new rows could have changed, and the row heights
    /// they stack to. Pages already closed are left alone.
    fn repaginate(&mut self) {
        let from = self.paged_from;
        // Every page that starts at or after `from` is provisional.
        while self
            .layout
            .pages
            .last()
            .is_some_and(|pg| pg.start >= from || pg.end > from)
        {
            self.layout.pages.pop();
        }
        let start_row = self.layout.pages.last().map(|pg| pg.end).unwrap_or(0);
        let fresh = paginate_from(
            &self.layout.rows,
            start_row,
            self.setup.height,
            self.setup.chapter_breaks,
            self.done,
        );
        // The final page stays open until the book is fully set, so more rows
        // can still flow into it.
        self.paged_from = fresh.last().map(|pg| pg.start).unwrap_or(start_row);
        self.layout.pages.extend(fresh);
        if self.layout.pages.is_empty() {
            self.layout.pages.push(Page { start: 0, end: 0 });
        }

        // Row tops are append-only.
        if self.layout.tops.is_empty() {
            self.layout.tops.push(0.0);
        }
        for i in self.layout.tops.len() - 1..self.layout.rows.len() {
            let top = self.layout.tops[i];
            self.layout.tops.push(top + self.layout.rows[i].height);
        }
    }
}

fn blank_row(m: &Metrics, offset: usize) -> Row {
    Row {
        text: String::new(),
        offset,
        indent: 0.0,
        height: m.blank_height,
        kind: RowKind::Blank,
        justify: false,
        drop_cap: None,
        chapter: None,
        chapter_start: false,
        hyphen: false,
    }
}

/// Break `doc` into rows of `setup.width`, then into pages of `setup.height`,
/// running to completion. The reader uses [`Typesetter`] instead so it can
/// spread the work over several frames.
pub fn typeset(
    doc: &Document,
    setup: &Setup,
    measure: &dyn Fn(&str) -> f32,
    measure_heading: &dyn Fn(&str) -> f32,
) -> Layout {
    let mut t = Typesetter::new(*setup);
    t.step(doc, usize::MAX, measure, measure_heading);
    t.layout
}

/// Break rows into pages that fit `height`, optionally starting every chapter
/// on a fresh page. Blank rows never open a page — a book does not start a
/// page with white space.
fn paginate_from(
    rows: &[Row],
    from: usize,
    height: f32,
    chapter_breaks: bool,
    closed: bool,
) -> Vec<Page> {
    let height = height.max(1.0);
    let mut pages = Vec::new();
    // A page never opens on white space — including the page this run starts
    // with. Only the break below used to swallow blanks, which was enough when
    // the whole book was set in one go and wrong when it was not: the
    // typesetter resumes at the row after the last closed page, and if a blank
    // sits there it became the first line of a leaf. The book then paginated
    // differently depending on how fast the machine had set it.
    let mut start = from;
    while start < rows.len() && rows[start].kind == RowKind::Blank {
        start += 1;
    }
    let mut used = 0.0f32;
    let mut i = start;

    while i < rows.len() {
        let row = &rows[i];
        let breaks_here = chapter_breaks && row.chapter_start && i > start;
        if breaks_here || (used + row.height > height && i > start) {
            // A run of blank rows is not a page. Text that opens with an empty
            // line (or a chapter break right at the top) would otherwise get a
            // blank leaf before the first word.
            if rows[start..i].iter().any(|r| r.kind != RowKind::Blank) {
                pages.push(Page { start, end: i });
            }
            start = i;
            used = 0.0;
            // Swallow blank rows at the top of a page.
            while start < rows.len() && rows[start].kind == RowKind::Blank {
                start += 1;
            }
            i = start;
            if i >= rows.len() {
                break;
            }
            continue;
        }
        used += row.height;
        i += 1;
    }
    if start < rows.len()
        && (closed || !pages.is_empty() || start >= from)
        && rows[start..].iter().any(|r| r.kind != RowKind::Blank)
    {
        pages.push(Page {
            start,
            end: rows.len(),
        });
    }
    pages
}

/// How a justified row should be stretched to reach the margin.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Stretch {
    /// Widen the word spaces by this much each.
    WordGaps(f32),
    /// Add this much between every pair of glyphs. Korean and Chinese lines
    /// hold too few spaces to absorb the slack, so they are letter-spaced —
    /// which is how those scripts are justified in print.
    Letters(f32),
    /// Leave the line ragged.
    None,
}

/// Decide how to fill the space left over on a justified row.
///
/// `max_gap` bounds a word space, `max_letter` the gap between two glyphs;
/// past those the line is left ragged rather than opening rivers of white.
pub fn stretch(text: &str, row_width: f32, target: f32, max_gap: f32, max_letter: f32) -> Stretch {
    let slack = target - row_width;
    if slack <= 0.0 {
        return Stretch::None;
    }
    if cjk_heavy(text) {
        let slots = text.graphemes(true).count().saturating_sub(1);
        if slots == 0 {
            return Stretch::None;
        }
        let extra = slack / slots as f32;
        return if extra > max_letter {
            Stretch::None
        } else {
            Stretch::Letters(extra)
        };
    }
    let gaps = text.matches(' ').count();
    if gaps == 0 {
        return Stretch::None;
    }
    let extra = slack / gaps as f32;
    if extra > max_gap {
        Stretch::None
    } else {
        Stretch::WordGaps(extra)
    }
}

/// True when wide glyphs carry most of the line, i.e. it is CJK text.
pub fn cjk_heavy(text: &str) -> bool {
    let mut wide = 0usize;
    let mut narrow = 0usize;
    for g in text.graphemes(true) {
        if g.chars().all(char::is_whitespace) {
            continue;
        }
        if UnicodeWidthStr::width(g) > 1 {
            wide += 1;
        } else {
            narrow += 1;
        }
    }
    wide * 2 > narrow
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Fake font: 10pt per narrow glyph, 20pt per wide one.
    fn fake(s: &str) -> f32 {
        s.graphemes(true)
            .map(|g| {
                if UnicodeWidthStr::width(g) > 1 {
                    20.0
                } else {
                    10.0
                }
            })
            .sum()
    }

    fn doc(s: &str) -> Document {
        Document::from_string(s.to_string(), &PathBuf::from("t.txt"), "UTF-8")
    }

    fn setup(width: f32, height: f32) -> Setup {
        Setup {
            width,
            height,
            metrics: Metrics::default(),
            justify: true,
            drop_caps: true,
            chapter_breaks: true,
            hyphenate: true,
        }
    }

    fn typeset_default(d: &Document, width: f32, height: f32) -> Layout {
        typeset(d, &setup(width, height), &fake, &fake)
    }

    #[test]
    fn pages_fit_the_height() {
        let d = doc(&(0..60).map(|i| format!("line {i}\n")).collect::<String>());
        let l = typeset_default(&d, 400.0, 200.0);
        for p in &l.pages {
            let h: f32 = l.rows[p.start..p.end].iter().map(|r| r.height).sum();
            assert!(h <= 200.0 + 0.01, "page overflows: {h}");
        }
    }

    #[test]
    fn pages_cover_every_non_blank_row_once() {
        let d = doc("Chapter 1\n\naaa bbb\n\nccc\n\nChapter 2\n\nddd\n");
        let l = typeset_default(&d, 300.0, 120.0);
        let mut covered = vec![0usize; l.rows.len()];
        for p in &l.pages {
            for c in covered.iter_mut().take(p.end).skip(p.start) {
                *c += 1;
            }
        }
        for (i, c) in covered.iter().enumerate() {
            if l.rows[i].kind != RowKind::Blank {
                assert_eq!(*c, 1, "row {i} covered {c} times: {:?}", l.rows[i].text);
            }
        }
    }

    #[test]
    fn every_chapter_opens_a_page() {
        let d = doc("Chapter 1\n\naaa\n\nChapter 2\n\nbbb\n\nChapter 3\n\nccc\n");
        let l = typeset_default(&d, 300.0, 400.0);
        assert_eq!(l.chapter_rows.len(), 3);
        for &row in &l.chapter_rows {
            let page = l.page_of_row(row);
            assert_eq!(l.pages[page].start, row, "chapter did not open its page");
        }
    }

    #[test]
    fn chapter_breaks_can_be_switched_off() {
        let d = doc("Chapter 1\n\naaa\n\nChapter 2\n\nbbb\n");
        let packed = typeset(
            &d,
            &Setup {
                justify: false,
                drop_caps: false,
                chapter_breaks: false,
                ..setup(300.0, 2000.0)
            },
            &fake,
            &fake,
        );
        assert_eq!(packed.pages.len(), 1);
    }

    #[test]
    fn a_document_that_opens_with_blank_lines_still_starts_on_its_text() {
        // The corpus generator writes a newline before the first chapter, and
        // that used to produce an empty first leaf.
        let d = doc("

Chapter 1

the first words of the book

more text
");
        let l = typeset_default(&d, 300.0, 200.0);
        let first = l.pages[0];
        assert!(
            l.rows[first.start..first.end]
                .iter()
                .any(|r| r.kind != RowKind::Blank),
            "the first page is blank"
        );
        assert_eq!(l.rows[first.start].text, "Chapter 1");
    }

    #[test]
    fn every_page_holds_something_to_read() {
        let d = doc("


Chapter 1



aaa




Chapter 2


bbb


");
        let l = typeset_default(&d, 300.0, 120.0);
        for (i, p) in l.pages.iter().enumerate() {
            assert!(
                l.rows[p.start..p.end].iter().any(|r| r.kind != RowKind::Blank),
                "page {i} has nothing on it"
            );
        }
    }

    #[test]
    fn a_page_never_starts_with_blank_space() {
        let d = doc(&(0..40)
            .map(|i| format!("paragraph {i}\n\n"))
            .collect::<String>());
        let l = typeset_default(&d, 300.0, 130.0);
        for p in &l.pages {
            assert_ne!(l.rows[p.start].kind, RowKind::Blank, "page opens on blank");
        }
    }

    /// A cap is skipped when it would leave no column beside it.
    ///
    /// The regression: at 52 and 64pt in a 140pt column the well was wider than
    /// the column, the breaker was handed one point to work with, and the row
    /// painted past the margin. `tests/column_fits.rs` catches it across the
    /// whole matrix; this pins the rule itself.
    #[test]
    fn a_cap_too_wide_for_the_column_is_not_set() {
        let text = "Chapter 1\n\nA private library is not an ornament at all.\n";
        // The test font makes a latin glyph 10pt, so the well is 10 × 2.9 + 6 =
        // 35pt and the ceiling bites below a 70pt measure. In the real reader
        // the same ratio is reached at 52 and 64pt type in a 140pt column,
        // which is where `tests/column_fits.rs` found it with the real font.
        let narrow = typeset_default(&doc(text), 60.0, 1000.0);
        for row in &narrow.rows {
            assert!(
                row.drop_cap.is_none(),
                "a drop cap was set into a column with no room for it: {:?}",
                row.text
            );
        }
        // And the ordinary case still gets its cap.
        let wide = typeset_default(&doc(text), 620.0, 1000.0);
        assert!(
            wide.rows.iter().any(|r| r.drop_cap.is_some()),
            "the drop cap went missing at an ordinary measure"
        );
    }

    #[test]
    fn drop_cap_is_set_once_after_a_heading() {
        let d = doc("Chapter 1\n\nCall me Ishmael and so on.\n\nSecond paragraph.\n");
        let l = typeset_default(&d, 400.0, 1000.0);
        let caps: Vec<_> = l.rows.iter().filter_map(|r| r.drop_cap.clone()).collect();
        assert_eq!(caps, vec!["C".to_string()]);
    }

    #[test]
    fn a_wide_drop_cap_gets_a_wide_well() {
        // The Hangul cap is twice as wide as the latin one in the fake font,
        // so the text must be wrapped around a wider well.
        let latin = typeset_default(&doc("Chapter 1\n\nCall me Ishmael.\n"), 400.0, 1000.0);
        let hangul = typeset_default(&doc("Chapter 1\n\n읽지 않은 책.\n"), 400.0, 1000.0);
        let well = |l: &Layout| {
            l.rows
                .iter()
                .find(|r| r.drop_cap.is_some())
                .map(|r| r.indent)
                .unwrap()
        };
        assert!(
            well(&hangul) > well(&latin),
            "wide cap well {} should exceed narrow cap well {}",
            well(&hangul),
            well(&latin)
        );
    }

    #[test]
    fn the_last_row_of_a_paragraph_is_never_justified() {
        let d = doc("aaa bbb ccc ddd eee fff ggg hhh iii jjj\n");
        let l = typeset_default(&d, 120.0, 1000.0);
        let body: Vec<_> = l.rows.iter().filter(|r| r.kind == RowKind::Body).collect();
        assert!(body.len() > 1);
        assert!(!body.last().unwrap().justify);
        assert!(body[0].justify);
    }

    #[test]
    fn offsets_survive_the_round_trip() {
        let d = doc(&(0..80).map(|i| format!("line {i} here\n")).collect::<String>());
        let l = typeset_default(&d, 300.0, 150.0);
        for (i, _) in l.pages.iter().enumerate() {
            let offset = l.offset_of_page(i);
            assert_eq!(l.page_of_offset(offset), i, "page {i} did not round trip");
        }
    }

    #[test]
    fn row_offsets_point_at_the_text_that_is_on_the_row() {
        let src = "Chapter 1\n\nCall me Ishmael and never mind how long precisely.\n\n읽지 않은 책이 쌓인 서가를 안티라이브러리라 부른다.\n";
        let d = doc(src);
        let l = typeset_default(&d, 200.0, 4000.0);
        let all: Vec<char> = src.chars().collect();
        for row in l.rows.iter().filter(|r| r.kind != RowKind::Blank) {
            let (a, b) = row.range();
            let from_source: String = all[a..b].iter().collect();
            assert_eq!(
                from_source, row.text,
                "row at {a}..{b} does not match the document"
            );
        }
    }

    #[test]
    fn extract_rebuilds_latin_prose_with_its_spaces() {
        let d = doc("aaa bbb ccc ddd eee fff\n");
        let l = typeset_default(&d, 80.0, 1000.0);
        assert!(l.rows.iter().filter(|r| r.kind == RowKind::Body).count() > 1);
        let (start, end) = (0, d.chars);
        assert_eq!(extract(&l.rows, start, end), "aaa bbb ccc ddd eee fff");
    }

    #[test]
    fn extract_rebuilds_korean_without_inventing_spaces() {
        let text = "읽지 않은 책이 쌓인 서가를 안티라이브러리라 부른다.";
        let d = doc(&format!("{text}\n"));
        let l = typeset_default(&d, 120.0, 1000.0);
        assert_eq!(extract(&l.rows, 0, d.chars), text);
    }

    #[test]
    fn extract_separates_paragraphs_with_a_blank_line() {
        let d = doc("first para\n\nsecond para\n");
        let l = typeset_default(&d, 300.0, 1000.0);
        assert_eq!(extract(&l.rows, 0, d.chars), "first para\n\nsecond para");
    }

    #[test]
    fn extract_takes_only_the_selected_span() {
        let d = doc("abcdefghij\n");
        let l = typeset_default(&d, 300.0, 1000.0);
        assert_eq!(extract(&l.rows, 2, 5), "cde");
        assert_eq!(extract(&l.rows, 0, 0), "");
        assert_eq!(extract(&l.rows, 100, 200), "");
    }

    #[test]
    fn clip_bounds_a_row_to_the_selection() {
        let row = Row {
            text: "hello".into(),
            offset: 10,
            indent: 0.0,
            height: 10.0,
            kind: RowKind::Body,
            justify: false,
            drop_cap: None,
            chapter: None,
            chapter_start: false,
            hyphen: false,
        };
        assert_eq!(row.clip(0, 100), Some((0, 5)));
        assert_eq!(row.clip(12, 14), Some((2, 4)));
        assert_eq!(row.clip(0, 10), None);
        assert_eq!(row.clip(15, 20), None);
    }

    #[test]
    fn row_tops_stack_up_to_the_total_height() {
        let d = doc(&(0..30).map(|i| format!("line {i}\n")).collect::<String>());
        let l = typeset_default(&d, 300.0, 200.0);
        assert_eq!(l.tops.len(), l.rows.len() + 1);
        let sum: f32 = l.rows.iter().map(|r| r.height).sum();
        assert!((l.total_height() - sum).abs() < 0.01);
        for (i, r) in l.rows.iter().enumerate() {
            assert!((l.tops[i + 1] - l.tops[i] - r.height).abs() < 0.01);
        }
    }

    #[test]
    fn row_at_height_finds_the_row_under_the_scroll_position() {
        let d = doc(&(0..30).map(|i| format!("line {i}\n")).collect::<String>());
        let l = typeset_default(&d, 300.0, 200.0);
        assert_eq!(l.row_at_height(-5.0), 0);
        assert_eq!(l.row_at_height(0.0), 0);
        let i = 7;
        let inside = l.tops[i] + l.rows[i].height * 0.5;
        assert_eq!(l.row_at_height(inside), i);
        assert!(l.row_at_height(l.total_height() * 2.0) < l.rows.len());
    }

    #[test]
    fn setting_a_book_in_steps_matches_setting_it_at_once() {
        let d = doc(&(0..120)
            .map(|i| {
                if i % 17 == 0 {
                    format!("Chapter {i}\n\n")
                } else {
                    format!("paragraph {i} with a few words in it\n\n")
                }
            })
            .collect::<String>());
        let whole = typeset_default(&d, 300.0, 220.0);

        let mut t = Typesetter::new(setup(300.0, 220.0));
        let mut steps = 0;
        while !t.step(&d, 7, &fake, &fake) {
            steps += 1;
            assert!(steps < 1000, "typesetter never finished");
        }
        assert!(steps > 3, "the test should take several steps");
        assert_eq!(t.layout.rows.len(), whole.rows.len());
        assert_eq!(t.layout.pages, whole.pages, "page breaks differ");
        assert_eq!(t.layout.chapter_rows, whole.chapter_rows);
        assert_eq!(t.layout.tops.len(), whole.tops.len());
    }

    #[test]
    fn a_partly_set_book_is_already_readable() {
        let d = doc(&(0..400)
            .map(|i| format!("paragraph {i} with several words\n\n"))
            .collect::<String>());
        let mut t = Typesetter::new(setup(300.0, 220.0));
        t.step(&d, 12, &fake, &fake);
        assert!(!t.done);
        assert!(!t.layout.pages.is_empty(), "no page to show yet");
        assert!(!t.layout.rows.is_empty());
        assert!(t.progress(&d) > 0.0 && t.progress(&d) < 1.0);
        // Whatever is already set must be internally consistent.
        assert_eq!(t.layout.tops.len(), t.layout.rows.len() + 1);
        for pg in &t.layout.pages {
            assert!(pg.end <= t.layout.rows.len());
        }
    }

    #[test]
    fn pages_settle_and_do_not_shuffle_once_closed() {
        let d = doc(&(0..200)
            .map(|i| format!("paragraph {i} words words\n\n"))
            .collect::<String>());
        let mut t = Typesetter::new(setup(300.0, 200.0));
        t.step(&d, 40, &fake, &fake);
        let early: Vec<_> = t.layout.pages.iter().take(2).copied().collect();
        while !t.step(&d, 40, &fake, &fake) {}
        let late: Vec<_> = t.layout.pages.iter().take(2).copied().collect();
        assert_eq!(early, late, "closed pages moved under the reader");
    }

    #[test]
    fn empty_document_yields_one_page() {
        let l = typeset_default(&doc(""), 300.0, 200.0);
        assert_eq!(l.pages.len(), 1);
    }

    #[test]
    fn latin_rows_are_justified_on_the_word_spaces() {
        let row = "aaa bbb ccc ddd";
        assert_eq!(
            stretch(row, 100.0, 112.0, 6.0, 2.0),
            Stretch::WordGaps(4.0)
        );
        assert_eq!(
            stretch(row, 100.0, 200.0, 6.0, 2.0),
            Stretch::None,
            "too much slack for four gaps"
        );
        assert_eq!(stretch("word", 100.0, 120.0, 6.0, 2.0), Stretch::None);
        assert_eq!(stretch(row, 120.0, 100.0, 6.0, 2.0), Stretch::None);
    }

    #[test]
    fn korean_rows_are_letter_spaced_instead() {
        // Five glyphs, four gaps between them: 8pt of slack is 2pt per gap.
        let row = "읽지않은책";
        assert_eq!(stretch(row, 100.0, 108.0, 6.0, 3.0), Stretch::Letters(2.0));
        assert_eq!(
            stretch(row, 100.0, 200.0, 6.0, 3.0),
            Stretch::None,
            "a huge gap between letters is worse than a ragged edge"
        );
    }

    #[test]
    fn a_mixed_line_follows_its_majority_script() {
        assert!(cjk_heavy("읽지 않은 책이 rust"));
        assert!(!cjk_heavy("the quick brown 책"));
        assert!(!cjk_heavy(""));
    }

    /// Every row must say where its own text is. This is the invariant every
    /// selection, highlight and search wash is built on, so it is checked over
    /// a matrix rather than on one example — the two ways of breaking it below
    /// each survived a passing test suite.
    fn rows_agree_with_the_document(src: &str, width: f32) -> Result<(), String> {
        let d = doc(src);
        let l = typeset_default(&d, width, 4000.0);
        for r in &l.rows {
            if r.kind == RowKind::Blank {
                continue;
            }
            let (a, b) = r.range();
            let says = d.slice(a, b);
            if says != r.text {
                return Err(format!(
                    "at width {width}: row {:?} sits at {a}..{b}, where the document says {says:?}",
                    r.text
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn a_word_split_across_rows_keeps_each_row_at_its_own_offset() {
        // Every row of a hard-split word was handed the offset of the word's
        // first character, so selecting anything on the second row of a long
        // word marked the wrong text.
        for src in [
            "Istanbul ISTANBUL\n",
            "supercalifragilisticexpialidocious\n",
            "https://example.com/a/very/long/path/that/will/not/fit\n",
            &"a".repeat(200),
        ] {
            for width in [30.0f32, 40.0, 55.0, 80.0, 130.0] {
                rows_agree_with_the_document(src, width).unwrap();
            }
        }
    }

    #[test]
    fn a_heading_row_points_past_the_markers_it_dropped() {
        // `# Third` is set as `Third`, but the `# ` is still on the line, so
        // the row has to start two characters further in.
        for src in [
            "# Third\n\nBody.\n",
            "### Deep heading\n\nBody.\n",
            "   Chapter 9\n\nBody.\n",
            "#Chapter\n\nBody.\n",
            "Chapter 1\n\nBody.\n",
        ] {
            for width in [60.0f32, 120.0, 200.0, 400.0] {
                rows_agree_with_the_document(src, width).unwrap();
            }
        }
    }

    #[test]
    fn a_book_paginates_the_same_however_fast_it_was_set() {
        // The typesetter resumes at the row after the last closed page. When a
        // blank row sat there it became the first line of a leaf, so the page
        // breaks of a finished book depended on how many paragraphs each step
        // had happened to set — that is, on the speed of the machine.
        let sources = [
            "The quick brown fox jumps over the lazy dog. It jumps again, and \
then it stops.\n\nA second paragraph follows here.\n",
            "Chapter 1\n\nFirst body.\n\nMore of it.\n\nChapter 2\n\nSecond body.\n",
            &"aaa bbb ccc\n\n".repeat(20),
            "\n\n\nOnly after some blank lines.\n\nAnd then more.\n",
        ];
        for src in sources {
            let d = doc(src);
            for height in [80.0f32, 140.0, 260.0] {
                for width in [90.0f32, 200.0, 380.0] {
                    let s = setup(width, height);
                    let want = typeset(&d, &s, &fake, &fake);
                    for chunk in [1usize, 2, 3, 5, 8, 64] {
                        let mut ts = Typesetter::new(s);
                        while !ts.step(&d, chunk, &fake, &fake) {}
                        assert_eq!(
                            ts.layout.pages, want.pages,
                            "{width}x{height} in steps of {chunk}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn no_page_ever_opens_on_a_blank_row_however_the_book_was_set() {
        let d = doc("Head\n\n\n\nBody one.\n\n\n\nBody two.\n\n\n\nBody three.\n");
        for chunk in [1usize, 2, 3, 7, 1000] {
            let mut ts = Typesetter::new(setup(200.0, 90.0));
            while !ts.step(&d, chunk, &fake, &fake) {}
            for p in &ts.layout.pages {
                assert_ne!(
                    ts.layout.rows[p.start].kind,
                    RowKind::Blank,
                    "in steps of {chunk}, page {p:?} opens on white space"
                );
            }
        }
    }

    #[test]
    fn narrow_column_does_not_hang() {
        let d = doc("supercalifragilistic 한글도 섞여 있다\n");
        let l = typeset_default(&d, 41.0, 100.0);
        assert!(!l.rows.is_empty());
    }
}

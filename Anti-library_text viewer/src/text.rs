//! Text loading, encoding detection and word wrapping.

use anyhow::{Context, Result};
use std::path::Path;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// A logical paragraph (source line) of the document.
#[derive(Debug, Clone)]
pub struct Paragraph {
    pub text: String,
    /// Character offset of this paragraph inside the whole document.
    pub offset: usize,
    /// Length of the source line in characters, *before* its trailing spaces
    /// were trimmed. Offsets are counted on the untrimmed line, so this is what
    /// [`Document::slice`] needs to give those spaces back.
    pub len: usize,
    pub is_blank: bool,
    pub is_heading: bool,
}

/// A single visual line produced by wrapping a paragraph to a given width.
#[derive(Debug, Clone)]
pub struct Line {
    pub text: String,
    pub offset: usize,
    pub is_heading: bool,
    pub blank: bool,
}

#[derive(Debug, Clone)]
pub struct Chapter {
    pub title: String,
    pub offset: usize,
}

/// One occurrence of a search, as a character range in the document.
///
/// Ranges are half open and never overlap, and the list they come in is
/// ordered — both `start` and `end` increase — which is what lets a painter
/// binary search for the matches that fall on one row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Match {
    pub start: usize,
    pub end: usize,
}

/// Characters that carry no text: a break hint we do not use, a zero width
/// space, and a byte order mark that wandered into the middle of a file.
///
/// They are dropped when the document is read rather than at the point of use.
/// Left in, a soft hyphen sits invisibly inside a word and quietly breaks
/// everything that matches on text: `co\u{ad}operate` is not found by a search
/// for `cooperate`, and copying the word pastes a character the reader never saw.
fn is_invisible(c: char) -> bool {
    matches!(c, '\u{00ad}' | '\u{200b}' | '\u{feff}')
}

#[derive(Debug, Clone)]
pub struct Document {
    pub title: String,
    pub path: String,
    pub encoding: &'static str,
    pub paragraphs: Vec<Paragraph>,
    pub chapters: Vec<Chapter>,
    pub chars: usize,
    pub words: usize,
    /// The format it was read from.
    pub format: crate::import::Format,
    /// The document is largely written right to left.
    ///
    /// The reader cannot set those scripts. The renderer it draws through lays
    /// glyphs out strictly left to right — its own source says so, and says
    /// bidirectional support has not been added — so a Hebrew or Arabic
    /// paragraph would come out reversed, word by word, and look like text.
    /// Saying nothing would be the worst of the three options; this is how the
    /// reader gets told.
    pub rtl: bool,
}

/// The encoding a byte order mark declares, if the bytes open with one.
///
/// A BOM is the file saying what it is, so it outranks every guess below —
/// including the "is this binary?" guess, which a UTF-16 file would otherwise
/// fail on its own padding NULs.
pub fn bom_encoding(bytes: &[u8]) -> Option<&'static encoding_rs::Encoding> {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return Some(encoding_rs::UTF_8);
    }
    // UTF-32LE opens with the same two bytes as UTF-16LE, so it is checked first.
    if bytes.starts_with(&[0xFF, 0xFE, 0x00, 0x00]) || bytes.starts_with(&[0x00, 0x00, 0xFE, 0xFF])
    {
        return None; // UTF-32 is not supported; fall through to the guesses
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {
        return Some(encoding_rs::UTF_16LE);
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        return Some(encoding_rs::UTF_16BE);
    }
    None
}

/// Decode bytes as UTF-8, falling back to legacy Korean/Japanese codepages.
///
/// A byte order mark is believed first: Windows Notepad still writes UTF-16 for
/// "Unicode", and those files are ordinary text however many NUL bytes they hold.
pub fn decode(bytes: &[u8]) -> (String, &'static str) {
    if let Some(enc) = bom_encoding(bytes) {
        let (cow, _, _) = enc.decode(bytes);
        return (cow.into_owned(), enc.name());
    }
    if let Ok(s) = std::str::from_utf8(bytes) {
        return (s.to_string(), "UTF-8");
    }
    for enc in [
        encoding_rs::EUC_KR,
        encoding_rs::SHIFT_JIS,
        encoding_rs::WINDOWS_1252,
    ] {
        let (cow, _, had_errors) = enc.decode(bytes);
        if !had_errors {
            return (cow.into_owned(), enc.name());
        }
    }
    let (cow, _, _) = encoding_rs::UTF_8.decode(bytes);
    (cow.into_owned(), "UTF-8 (lossy)")
}

/// Put the text into composed form, so one letter is one character.
///
/// The same word can be written two ways: `é` as itself, or as `e` followed by
/// a combining accent — and Korean the same, as a syllable or as the jamo it is
/// built from, which is what macOS writes. Nothing downstream can tell those
/// apart, so all of it went wrong at once: a search for `한글` typed on a
/// Windows keyboard found nothing in a file from a Mac, and the typesetter
/// measured a combining mark as a character of its own width, putting every
/// selection on that line about nine points to the right of the text.
///
/// The quick check first: most files are already composed, and normalising one
/// costs a pass over the whole book.
fn to_nfc(raw: String) -> String {
    use unicode_normalization::{is_nfc_quick, IsNormalized, UnicodeNormalization};
    match is_nfc_quick(raw.chars()) {
        IsNormalized::Yes => raw,
        _ => raw.nfc().collect(),
    }
}

/// Does this character take no width of its own — a mark that sits on the
/// letter before it?
///
/// The layout adds up glyph advances, but a combining mark has no advance: it
/// is drawn onto its neighbour. Counting one would push everything after it
/// along a line the painter never moved.
pub fn is_combining(c: char) -> bool {
    unicode_width::UnicodeWidthChar::width(c) == Some(0) && !c.is_control()
}

/// Is this a character from a script written right to left?
fn is_rtl(c: char) -> bool {
    matches!(c,
        '\u{0590}'..='\u{05ff}'   // Hebrew
        | '\u{0600}'..='\u{06ff}' // Arabic
        | '\u{0700}'..='\u{074f}' // Syriac
        | '\u{0780}'..='\u{07bf}' // Thaana
        | '\u{07c0}'..='\u{08ff}' // NKo, Samaritan, Arabic supplement
        | '\u{fb1d}'..='\u{fdff}' // Hebrew and Arabic presentation forms
        | '\u{fe70}'..='\u{feff}'
    )
}

/// Case fold a string for searching, one character at a time.
fn fold(s: &str) -> Vec<char> {
    s.chars().flat_map(char::to_lowercase).collect()
}

/// The matches that touch the character range `start..end`.
///
/// `matches` must be the ordered, non-overlapping list [`Document::search`]
/// returns; the range is found by binary search, so painting a screenful of
/// rows costs the same whether the book holds ten matches or ten thousand.
pub fn matches_in(matches: &[Match], start: usize, end: usize) -> &[Match] {
    let from = matches.partition_point(|m| m.end <= start);
    let to = from + matches[from..].partition_point(|m| m.start < end);
    &matches[from..to]
}

fn looks_like_heading(line: &str) -> bool {
    let t = line.trim();
    if t.is_empty() || t.chars().count() > 60 {
        return false;
    }
    if t.starts_with('#') {
        return true;
    }
    let upper = t.to_uppercase();
    for key in [
        "CHAPTER ", "PART ", "BOOK ", "PROLOGUE", "EPILOGUE", "PREFACE",
    ] {
        if upper.starts_with(key) {
            return true;
        }
    }
    // Korean chapter headings, in the two shapes books actually use:
    // `제 3 장` (a leading 제, then 장/부/편 for chapter/part/volume) and `3장`
    // (a digit, then the same word). These characters are the feature — a
    // Korean document has no contents drawer without them — so they are product
    // logic and not text awaiting translation.
    if t.starts_with('제') && (t.contains('장') || t.contains('부') || t.contains('편')) {
        return true;
    }
    if (t.ends_with('장') || t.ends_with('부') || t.ends_with('편'))
        && t.chars().next().is_some_and(|c| c.is_ascii_digit())
    {
        return true;
    }
    let mut it = t.chars();
    if it.next().is_some_and(|c| c.is_ascii_digit()) {
        let rest: String = it.collect();
        if rest.starts_with('.') && t.chars().count() <= 40 {
            return true;
        }
    }
    false
}

impl Document {
    /// Read a document from disk. Word, PDF, EPUB, ODT, RTF and HTML files are
    /// converted to text first; anything else is decoded as text.
    pub fn load(path: &Path) -> Result<Document> {
        let (converted, format) = crate::import::extract(path)?;
        if let Some(text) = converted {
            let mut doc = Self::from_string(text, path, "UTF-8");
            doc.format = format;
            return Ok(doc);
        }
        let bytes =
            std::fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
        let (raw, encoding) = decode(&bytes);
        Ok(Self::from_string(raw, path, encoding))
    }

    pub fn from_string(raw: String, path: &Path, encoding: &'static str) -> Document {
        let raw = raw
            .replace("\r\n", "\n")
            .replace('\r', "\n")
            .replace('\t', "    ");
        // Offsets are counted after this, so nothing downstream ever has to
        // know these characters existed.
        let raw: String = if raw.chars().any(is_invisible) {
            raw.chars().filter(|c| !is_invisible(*c)).collect()
        } else {
            raw
        };
        let raw = to_nfc(raw);
        let mut paragraphs = Vec::new();
        let mut chapters = Vec::new();
        let mut offset = 0usize;

        for line in raw.split('\n') {
            let text = line.trim_end().to_string();
            let is_blank = text.trim().is_empty();
            let is_heading = !is_blank && looks_like_heading(&text);
            if is_heading {
                chapters.push(Chapter {
                    title: text.trim().trim_start_matches('#').trim().to_string(),
                    offset,
                });
            }
            let len = line.chars().count();
            paragraphs.push(Paragraph {
                text,
                offset,
                len,
                is_blank,
                is_heading,
            });
            offset += len + 1; // the newline that ended the line
        }

        let title = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Untitled".into());
        let words = raw.split_whitespace().count();
        // A sentence may quote a word of Hebrew without being a Hebrew book.
        // The question is whether the body is written that way — and that is
        // settled by the opening. Asking every character of a 20 MB book cost
        // a fifth of a second on its own, which is a great deal to pay to
        // learn something the first page already says.
        const SAMPLE: usize = 64 * 1024;
        let (mut rtl_chars, mut letters) = (0usize, 0usize);
        for (at, c) in raw.char_indices() {
            if at >= SAMPLE {
                break;
            }
            // ASCII carries most documents and is never right to left, so it is
            // dismissed before any table is consulted.
            if c.is_ascii() {
                if c.is_ascii_alphabetic() {
                    letters += 1;
                }
                continue;
            }
            if c.is_alphabetic() {
                letters += 1;
                if is_rtl(c) {
                    rtl_chars += 1;
                }
            }
        }
        let rtl = letters > 20 && rtl_chars * 5 > letters;

        Document {
            title,
            // This string is the key the library files the book under, so it
            // is settled here, once, rather than at each of the places a path
            // can reach the reader.
            path: crate::library::key_for(path),
            encoding,
            paragraphs,
            chapters,
            chars: raw.chars().count(),
            words,
            format: crate::import::Format::Text,
            rtl,
        }
    }

    /// Paragraph holding `offset`, by binary search.
    fn paragraph_at(&self, offset: usize) -> usize {
        match self.paragraphs.binary_search_by(|p| p.offset.cmp(&offset)) {
            Ok(i) => i,
            Err(0) => 0,
            Err(i) => i - 1,
        }
    }

    /// The document text between two character offsets.
    ///
    /// Trailing spaces were trimmed when the document was read, so they come
    /// back as spaces here: the slice is always exactly `end - start` characters
    /// long (clamped to the document), which is what callers comparing a saved
    /// highlight against the file depend on.
    pub fn slice(&self, start: usize, end: usize) -> String {
        if start >= end || self.paragraphs.is_empty() {
            return String::new();
        }
        let mut out = String::new();
        for p in &self.paragraphs[self.paragraph_at(start)..] {
            if p.offset >= end {
                break;
            }
            let kept = p.text.chars().count();
            let line_start = p.offset;
            let from = start.saturating_sub(line_start).min(p.len);
            let to = (end - line_start).min(p.len);
            let mut kept_chars = p.text.chars().skip(from);
            for i in from..to {
                // Past the trimmed text: a space the reader never sees but the
                // offsets still count.
                let c = if i < kept {
                    kept_chars.next().unwrap_or(' ')
                } else {
                    ' '
                };
                out.push(c);
            }
            // The newline that ended the line, if the range reaches it.
            if end > line_start + p.len && start <= line_start + p.len {
                out.push('\n');
            }
        }
        out
    }

    /// Find `needle`, preferring the occurrence nearest to `near`.
    ///
    /// Used to re-anchor a highlight after the file it marks has been edited:
    /// the saved offsets no longer point at the saved text, but the words are
    /// usually still in the book, a little to one side. The search therefore
    /// starts in a window around the old position and widens only if it finds
    /// nothing — scanning a 20 MB book once per highlight would stall the
    /// window on open.
    pub fn find_near(&self, needle: &str, near: usize) -> Option<usize> {
        if needle.is_empty() || self.paragraphs.is_empty() {
            return None;
        }
        let centre = self.paragraph_at(near);
        let n = self.paragraphs.len();
        let mut window = 64usize;
        loop {
            let from = centre.saturating_sub(window);
            let to = (centre + window + 1).min(n);
            if let Some(found) = self.find_in(needle, near, from, to) {
                return Some(found);
            }
            if from == 0 && to == n {
                return None;
            }
            window = window.saturating_mul(8).max(64);
        }
    }

    /// Nearest occurrence of `needle` to `near`, searching paragraphs
    /// `range` only.
    fn find_in(&self, needle: &str, near: usize, from: usize, to: usize) -> Option<usize> {
        let mut best: Option<(usize, usize)> = None; // (distance, offset)
        for p in &self.paragraphs[from..to] {
            let mut at = 0usize;
            while let Some(rel) = p.text[at..].find(needle) {
                let byte_at = at + rel;
                let offset = p.offset + p.text[..byte_at].chars().count();
                let d = offset.abs_diff(near);
                if best.is_none_or(|(bd, _)| d < bd) {
                    best = Some((d, offset));
                }
                at = byte_at + needle.len().max(1);
                if at >= p.text.len() {
                    break;
                }
            }
        }
        best.map(|(_, o)| o)
    }

    /// Every case-insensitive occurrence of `needle` in the document.
    ///
    /// The search runs on the paragraphs, not on the wrapped rows, and that is
    /// the whole point of it. A row is where the *column* happened to break the
    /// text, so a reader looking for `quick brown` in a book that set those two
    /// words on different lines used to be told there was no match — and a
    /// Korean reader had it worse, because Korean wraps between glyphs, so any
    /// word long enough to straddle a line became unfindable. Neither the
    /// column width nor how much of the book has been set can change the answer
    /// this gives.
    ///
    /// Matches never overlap and come out in order, so a painter can binary
    /// search this list for the few that fall on one row.
    pub fn search(&self, needle: &str) -> Vec<Match> {
        let query: Vec<char> = fold(needle);
        if query.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::new();
        // One pair of buffers for the whole book. The reader searches on every
        // keystroke, and a 20 MB book is four hundred thousand paragraphs — a
        // fresh allocation for each of them is most of the cost of the search.
        let mut hay: Vec<char> = Vec::new();
        let mut src: Vec<usize> = Vec::new();
        // Folding a paragraph costs a pass over it and two buffers filled, and
        // most paragraphs hold no occurrence at all. Dismissing those cheaply
        // is where the time goes: the reader searches on every keystroke, and a
        // 20 MB book is four hundred thousand paragraphs.
        //
        // The dismissal has to be exact, not merely likely. It is only made for
        // a paragraph of plain ASCII, where the whole question is settled by a
        // case-insensitive byte: no ASCII character folds to a non-ASCII one,
        // so an ASCII paragraph cannot hold a match for a query that opens with
        // one. Anything else takes the long road.
        let first = query[0];
        let first_ascii = first.is_ascii().then_some(first as u8);
        for p in &self.paragraphs {
            // Nothing to find in a blank line, or in one shorter than the query.
            if p.is_blank || p.text.len() < query.len() {
                continue;
            }
            if p.text.is_ascii() {
                let Some(b) = first_ascii else { continue };
                if !p.text.as_bytes().iter().any(|c| c.eq_ignore_ascii_case(&b)) {
                    continue;
                }
            }
            // Case folding is not one character in, one character out — `ß`
            // folds to `ss`, `İ` to two characters — so the folded text carries
            // the index of the source character each of its characters came
            // from. Slicing the original by a folded index would cut it apart.
            hay.clear();
            src.clear();
            for (i, c) in p.text.chars().enumerate() {
                // ASCII is one character in, one out, and the general mapping
                // is a table lookup this can skip.
                if c.is_ascii() {
                    hay.push(c.to_ascii_lowercase());
                    src.push(i);
                } else {
                    for lc in c.to_lowercase() {
                        hay.push(lc);
                        src.push(i);
                    }
                }
            }
            let mut at = 0usize;
            while at + query.len() <= hay.len() {
                if hay[at..at + query.len()] == query[..] {
                    let start = src[at];
                    // The character *after* the last one the match covered.
                    let end = src[at + query.len() - 1] + 1;
                    out.push(Match {
                        start: p.offset + start,
                        end: p.offset + end,
                    });
                    at += query.len();
                } else {
                    at += 1;
                }
            }
        }
        out
    }

    /// Wrap the whole document to `width` columns, honouring word boundaries for
    /// space separated scripts and grapheme boundaries for CJK.
    pub fn wrap(&self, width: usize, indent: bool) -> Vec<Line> {
        let width = width.max(8);
        let mut out = Vec::new();
        for p in self.paragraphs.iter() {
            if p.is_blank {
                out.push(Line {
                    text: String::new(),
                    offset: p.offset,
                    is_heading: false,
                    blank: true,
                });
                continue;
            }
            let lead = if indent && !p.is_heading { "  " } else { "" };
            let mut first = true;
            for (text, delta) in wrap_line(&p.text, width, lead.len()) {
                out.push(Line {
                    text: if first { format!("{lead}{text}") } else { text },
                    offset: p.offset + delta,
                    is_heading: p.is_heading,
                    blank: false,
                });
                first = false;
            }
        }
        if out.is_empty() {
            out.push(Line {
                text: String::new(),
                offset: 0,
                is_heading: false,
                blank: true,
            });
        }
        out
    }
}

/// Wrap one source line. Returns `(text, char offset within the line)` pairs.
fn wrap_line(src: &str, width: usize, first_indent: usize) -> Vec<(String, usize)> {
    let mut rows: Vec<(String, usize)> = Vec::new();
    let mut cur = String::new();
    let mut cur_w = 0usize;
    let mut cur_start = 0usize;
    let mut chars_seen = 0usize;
    let mut limit = width.saturating_sub(first_indent).max(4);

    // Split into chunks that must not be broken: a run of non-space narrow
    // characters (a latin word), a single wide grapheme, or a space.
    let mut chunks: Vec<(String, usize)> = Vec::new();
    let mut word = String::new();
    let mut word_start = 0usize;
    for g in src.graphemes(true) {
        let gc = g.chars().count();
        let wide = UnicodeWidthStr::width(g) > 1;
        if g.chars().all(char::is_whitespace) || wide {
            if !word.is_empty() {
                chunks.push((std::mem::take(&mut word), word_start));
            }
            chunks.push((g.to_string(), chars_seen));
        } else {
            if word.is_empty() {
                word_start = chars_seen;
            }
            word.push_str(g);
        }
        chars_seen += gc;
    }
    if !word.is_empty() {
        chunks.push((word, word_start));
    }

    for (chunk, start) in chunks {
        let cw = UnicodeWidthStr::width(chunk.as_str());
        let is_space = chunk.chars().all(char::is_whitespace);
        if cur_w + cw > limit && !cur.is_empty() {
            rows.push((cur.trim_end().to_string(), cur_start));
            cur = String::new();
            cur_w = 0;
            cur_start = start;
            limit = width;
            if is_space {
                continue; // swallow the space that caused the break
            }
        }
        if cur.is_empty() {
            if is_space && !rows.is_empty() {
                continue;
            }
            cur_start = start;
        }
        // A single chunk longer than a whole line: hard split it. Each row
        // starts further into the chunk and must carry that offset, or the
        // reader's place in a long word is a character count out.
        if cw > limit {
            let mut placed = 0usize;
            for g in chunk.graphemes(true) {
                let gw = UnicodeWidthStr::width(g);
                if cur_w + gw > limit && !cur.is_empty() {
                    rows.push((std::mem::take(&mut cur), cur_start));
                    cur_w = 0;
                    cur_start = start + placed;
                    limit = width;
                }
                cur.push_str(g);
                cur_w += gw;
                placed += g.chars().count();
            }
            continue;
        }
        cur.push_str(&chunk);
        cur_w += cw;
    }
    if !cur.trim().is_empty() || rows.is_empty() {
        rows.push((cur.trim_end().to_string(), cur_start));
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn doc(s: &str) -> Document {
        Document::from_string(s.to_string(), &PathBuf::from("t.txt"), "UTF-8")
    }

    #[test]
    fn wraps_latin_on_word_boundaries() {
        let rows = wrap_line("the quick brown fox jumps", 10, 0);
        assert!(rows
            .iter()
            .all(|(t, _)| UnicodeWidthStr::width(t.as_str()) <= 10));
        assert_eq!(rows[0].0, "the quick");
    }

    #[test]
    fn wraps_cjk_without_spaces() {
        let rows = wrap_line("가나다라마바사아자차", 6, 0);
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0].0, "가나다");
    }

    #[test]
    fn hard_splits_overlong_word() {
        let rows = wrap_line("aaaaaaaaaaaaaaaaaaaa", 6, 0);
        assert!(rows.len() > 1);
        assert!(rows.iter().all(|(t, _)| t.chars().count() <= 6));
    }

    #[test]
    fn offsets_are_monotonic_and_within_document() {
        let d = doc("hello world this is a long enough line\n\nsecond paragraph here");
        let lines = d.wrap(12, true);
        let mut prev = 0;
        for l in &lines {
            assert!(l.offset >= prev, "offset went backwards");
            assert!(l.offset <= d.chars);
            prev = l.offset;
        }
    }

    #[test]
    fn detects_headings() {
        let d = doc("Chapter 1\n\ntext\n\n제 2 장\n\nmore");
        assert_eq!(d.chapters.len(), 2);
        assert_eq!(d.chapters[0].title, "Chapter 1");
        assert_eq!(d.chapters[1].title, "제 2 장");
    }

    #[test]
    fn prose_is_not_mistaken_for_a_heading() {
        assert!(!looks_like_heading(
            "It was the best of times, it was the worst of times, and nobody knew."
        ));
        assert!(!looks_like_heading("그는 문을 열고 밖으로 나갔다."));
    }

    #[test]
    fn blank_lines_survive_wrapping() {
        let d = doc("a\n\nb");
        let lines = d.wrap(20, false);
        assert!(lines.iter().any(|l| l.blank));
    }

    #[test]
    fn decodes_euc_kr() {
        let (bytes, _, _) = encoding_rs::EUC_KR.encode("한글 테스트");
        let (s, name) = decode(&bytes);
        assert_eq!(s, "한글 테스트");
        assert_eq!(name, "EUC-KR");
    }

    #[test]
    fn decodes_utf16_by_its_byte_order_mark() {
        for (bom, enc) in [([0xFFu8, 0xFE], encoding_rs::UTF_16LE), ([0xFE, 0xFF], encoding_rs::UTF_16BE)] {
            let mut bytes = bom.to_vec();
            for u in "읽지 않은 책\n둘째 줄".encode_utf16() {
                let b = if bom[0] == 0xFF {
                    u.to_le_bytes()
                } else {
                    u.to_be_bytes()
                };
                bytes.extend_from_slice(&b);
            }
            let (s, name) = decode(&bytes);
            assert_eq!(s, "읽지 않은 책\n둘째 줄", "{}", enc.name());
            assert_eq!(name, enc.name());
        }
    }

    #[test]
    fn a_utf8_byte_order_mark_is_not_read_as_text() {
        let mut bytes = vec![0xEFu8, 0xBB, 0xBF];
        bytes.extend_from_slice("hello".as_bytes());
        let (s, name) = decode(&bytes);
        assert_eq!(s, "hello");
        assert_eq!(name, "UTF-8");
    }

    #[test]
    fn wrapping_never_loses_characters() {
        let src = "읽지 않은 책이 쌓인 서가를 안티라이브러리라 부른다. 이미 읽은 책보다 아직 읽지 않은 책이 더 많은 것을 아는 일, 그것이 서재의 쓸모다.";
        for w in [20usize, 33, 40, 56, 57] {
            let joined: String = wrap_line(src, w, 2)
                .iter()
                .map(|(t, _)| t.replace(' ', ""))
                .collect();
            let expected: String = src.replace(' ', "");
            assert_eq!(joined, expected, "characters lost at width {w}");
        }
    }

    #[test]
    fn slice_returns_the_text_at_those_offsets() {
        let src = "first line here\nsecond line\n\nfourth";
        let d = doc(src);
        let all: Vec<char> = src.chars().collect();
        for (a, b) in [(0, 5), (6, 11), (16, 27), (0, src.chars().count()), (3, 3)] {
            let expected: String = all[a..b].iter().collect();
            assert_eq!(d.slice(a, b), expected, "slice {a}..{b}");
        }
    }

    #[test]
    fn slice_gives_back_the_spaces_that_were_trimmed() {
        // Offsets are counted on the untrimmed line, so a slice that spans a
        // trailing space must be as long as the range it was asked for.
        let src = "abc   \nnext line";
        let d = doc(src);
        let all: Vec<char> = src.chars().collect();
        for (a, b) in [(0, 8), (2, 7), (3, 6), (0, src.chars().count())] {
            let expected: String = all[a..b].iter().collect();
            assert_eq!(d.slice(a, b), expected, "slice {a}..{b}");
            assert_eq!(d.slice(a, b).chars().count(), b - a, "length {a}..{b}");
        }
    }

    #[test]
    fn slice_is_exact_for_every_range_of_every_shape() {
        for src in [
            "abc   \nnext\n",
            "  leading and trailing   \n\n   \nx\n",
            "한글  \n두번째 줄   \n\n끝",
            "no trailing at all\nnone here either",
            "\n\n\n",
            "",
            "tabs\there  \nand more \t\n",
            "mixed\r\nline\r\nendings   \r\n",
        ] {
            // Newlines are normalised and tabs expanded before offsets are
            // counted, so the document's own text is what a slice must match.
            let norm = src
                .replace("\r\n", "\n")
                .replace('\r', "\n")
                .replace('\t', "    ");
            let d = doc(src);
            let all: Vec<char> = norm.chars().collect();
            assert_eq!(d.chars, all.len(), "char count for {src:?}");
            for a in 0..=all.len() {
                for b in a..=all.len() {
                    let expected: String = all[a..b].iter().collect();
                    assert_eq!(d.slice(a, b), expected, "{src:?} range {a}..{b}");
                }
            }
        }
    }

    #[test]
    fn slice_is_clamped_and_never_panics() {
        let d = doc("short\n");
        assert_eq!(d.slice(100, 200), "");
        assert_eq!(d.slice(9, 3), "");
    }

    #[test]
    fn find_near_prefers_the_closest_occurrence() {
        let d = doc("alpha marker beta\n\ngamma marker delta\n\nmarker end");
        let first = d.find_near("marker", 0).unwrap();
        let last = d.find_near("marker", d.chars).unwrap();
        assert!(first < last);
        assert_eq!(d.slice(first, first + 6), "marker");
        assert_eq!(d.slice(last, last + 6), "marker");
        assert_eq!(d.find_near("not here at all", 0), None);
    }

    #[test]
    fn find_near_handles_korean_text() {
        let d = doc("앞 문장\n\n읽지 않은 책이 쌓인 서가\n\n뒤 문장");
        let at = d.find_near("쌓인 서가", 0).unwrap();
        assert_eq!(d.slice(at, at + 5), "쌓인 서가");
    }

    #[test]
    fn find_near_searches_close_to_home_first() {
        // Two identical passages far apart: each anchor must find its own.
        let filler = "filler line that is here only to take up room\n\n";
        let text = format!("{}marker text\n\n{}marker text\n", filler.repeat(400), filler.repeat(400));
        let d = doc(&text);
        let first = d.find_near("marker text", 0).unwrap();
        let second = d.find_near("marker text", d.chars).unwrap();
        assert!(first < second, "the two passages were not told apart");
        assert_eq!(d.slice(first, first + 11), "marker text");
        assert_eq!(d.slice(second, second + 11), "marker text");
    }

    #[test]
    fn find_near_still_reaches_text_far_from_the_anchor() {
        // Nothing nearby: the window has to widen until it finds the words.
        let filler = "nothing to see on this line at all\n\n";
        let text = format!("{}the only marker in the book\n", filler.repeat(5000));
        let d = doc(&text);
        let at = d.find_near("the only marker", 0).expect("must widen and find it");
        assert_eq!(d.slice(at, at + 15), "the only marker");
    }

    #[test]
    fn every_wrapped_line_points_at_the_text_it_holds() {
        // The terminal reader jumps by the offset a line carries, so a line
        // that lies about where it is takes the reader somewhere else. Long
        // words are the case that broke: each row of a hard-split word used to
        // be handed the offset of the word's first character.
        for src in [
            "hello world this is ordinary prose\n\nsecond paragraph\n",
            "supercalifragilisticexpialidocious and more\n",
            "https://example.com/a/very/long/path/that/will/not/fit\n",
            "읽지 않은 책이 쌓인 서가를 안티라이브러리라 부른다.\n",
            &"a".repeat(150),
        ] {
            let d = doc(src);
            for width in [8usize, 12, 20, 33, 64] {
                for line in d.wrap(width, false) {
                    if line.blank {
                        continue;
                    }
                    let end = line.offset + line.text.chars().count();
                    let says = d.slice(line.offset, end);
                    assert_eq!(
                        says, line.text,
                        "at width {width}: line {:?} sits at {}..{end}, where the document says {says:?}",
                        line.text, line.offset
                    );
                }
            }
        }
    }

    #[test]
    fn empty_document_still_wraps() {
        let d = doc("");
        assert!(!d.wrap(40, true).is_empty());
    }

    /// Every occurrence, found the slow and obvious way, for the search to be
    /// held against. Paragraph by paragraph, because that is the unit the
    /// document search works in — a phrase does not run across a blank line.
    fn brute_force(d: &Document, needle: &str) -> Vec<Match> {
        let q: Vec<char> = needle.chars().flat_map(char::to_lowercase).collect();
        let mut out = Vec::new();
        if q.is_empty() {
            return out;
        }
        for p in &d.paragraphs {
            if p.is_blank {
                continue;
            }
            let chars: Vec<char> = p.text.chars().collect();
            let mut i = 0usize;
            while i < chars.len() {
                // The shortest run of characters from here whose folding is
                // exactly the query. Slow on purpose: nothing is shared with
                // the implementation it is checking.
                let hit = (i + 1..=chars.len()).find(|&j| {
                    let folded: Vec<char> =
                        chars[i..j].iter().flat_map(|c| c.to_lowercase()).collect();
                    folded == q
                });
                match hit {
                    Some(j) => {
                        out.push(Match {
                            start: p.offset + i,
                            end: p.offset + j,
                        });
                        i = j;
                    }
                    None => i += 1,
                }
            }
        }
        out
    }

    #[test]
    fn search_finds_a_phrase_the_column_broke_in_two() {
        // The defect this replaces: the reader searched the wrapped rows, so a
        // phrase split across a line break was reported as "no match".
        let d = doc("the quick brown fox jumps over the lazy dog\n");
        for q in ["quick brown", "lazy dog", "jumps over the", "the quick brown fox"] {
            let hits = d.search(q);
            assert_eq!(hits.len(), 1, "{q:?} was not found");
            assert_eq!(d.slice(hits[0].start, hits[0].end), q);
        }
    }

    #[test]
    fn search_finds_a_korean_word_however_the_column_falls() {
        // Korean wraps between glyphs, so a long word straddles a line at most
        // widths. Searching rows made those words unfindable.
        let d = doc("읽지 않은 책이 쌓인 서가를 안티라이브러리라 부른다.\n");
        for q in ["안티라이브러리", "쌓인 서가", "부른다"] {
            let hits = d.search(q);
            assert_eq!(hits.len(), 1, "{q:?} was not found");
            assert_eq!(d.slice(hits[0].start, hits[0].end), q);
        }
    }

    #[test]
    fn search_matches_are_the_text_they_claim_to_be() {
        for src in [
            "First LINE here\nsecond line\n\nthird Line and line again\n",
            "한글 문장 한글 문장\n\n다른 문장\n",
            "trailing spaces here   \nnext\n",
            "Grüße aus Köln, grüße\n",
            "",
        ] {
            let d = doc(src);
            for q in ["line", "한글", "문장", "grüße", "here", "zzz", ""] {
                let hits = d.search(q);
                assert_eq!(hits, brute_force(&d, q), "{src:?} / {q:?}");
                for h in &hits {
                    let got = d.slice(h.start, h.end);
                    assert_eq!(
                        got.to_lowercase(),
                        q.to_lowercase(),
                        "{src:?} / {q:?}: matched {got:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn search_results_are_ordered_and_never_overlap() {
        let d = doc(&"aaa bbb aaa ccc aaa\n\n".repeat(40));
        let hits = d.search("aaa");
        assert_eq!(hits.len(), 120);
        for w in hits.windows(2) {
            assert!(w[0].end <= w[1].start, "{:?} then {:?}", w[0], w[1]);
            assert!(w[0].start < w[1].start);
        }
    }

    #[test]
    fn search_does_not_run_across_a_blank_line() {
        let d = doc("ends with quick\n\nbrown starts here\n");
        assert!(d.search("quick brown").is_empty());
    }

    #[test]
    fn matches_in_returns_exactly_the_ones_that_touch_the_range() {
        let d = doc(&"aaa bbb\n\n".repeat(30));
        let hits = d.search("aaa");
        for start in 0..d.chars {
            for len in [1usize, 3, 9, 40] {
                let end = (start + len).min(d.chars);
                let got = matches_in(&hits, start, end);
                let want: Vec<Match> = hits
                    .iter()
                    .copied()
                    .filter(|m| m.start < end && start < m.end)
                    .collect();
                assert_eq!(got, &want[..], "range {start}..{end}");
            }
        }
    }

    #[test]
    fn a_soft_hyphen_does_not_hide_a_word_from_the_search() {
        // `&shy;` arrives from HTML as U+00AD. Left in the text it sits inside
        // the word invisibly, and the word can no longer be found or copied.
        let d = doc("co\u{ad}operate with the te\u{200b}am\n");
        assert_eq!(d.search("cooperate").len(), 1);
        assert_eq!(d.search("team").len(), 1);
        assert!(
            !d.paragraphs[0].text.chars().any(is_invisible),
            "an invisible character survived into the text"
        );
    }

    #[test]
    fn text_is_composed_so_one_letter_is_one_character() {
        // The same words, written the two ways Unicode allows. A reader typing
        // on a Windows keyboard produces the first; a file from a Mac holds the
        // second. Nothing downstream can tell them apart, so they are made the
        // same here.
        for (decomposed, composed) in [
            ("e\u{301}cole", "école"),
            ("cafe\u{301}", "café"),
            ("\u{1112}\u{1161}\u{11ab}\u{1100}\u{1173}\u{11af}", "한글"),
            ("already composed", "already composed"),
        ] {
            let d = doc(&format!("{decomposed}\n"));
            assert_eq!(d.paragraphs[0].text, composed, "{decomposed:?}");
            assert_eq!(d.chars, composed.chars().count() + 1);
            assert_eq!(d.search(composed).len(), 1, "searching for {composed:?}");
            // And the offsets still describe the text that is there.
            assert_eq!(d.slice(0, composed.chars().count()), composed);
        }
    }

    #[test]
    fn a_combining_mark_takes_no_width_of_its_own() {
        // It is drawn onto the letter before it, so counting its advance would
        // push everything after it along a line the painter never moved.
        assert!(is_combining('\u{0301}'), "a combining acute");
        assert!(is_combining('\u{0e31}'), "a Thai vowel sign");
        assert!(is_combining('\u{05b8}'), "a Hebrew point");
        assert!(!is_combining('a'));
        assert!(!is_combining('한'));
        assert!(!is_combining(' '));
        assert!(!is_combining('\n'), "a control character is not a mark");
    }

    #[test]
    fn dropping_invisibles_leaves_the_offsets_consistent() {
        let d = doc("a\u{ad}b\u{200b}c\nsecond\n");
        assert_eq!(d.paragraphs[0].text, "abc");
        assert_eq!(d.chars, "abc\nsecond\n".chars().count());
        assert_eq!(d.slice(0, 3), "abc");
        assert_eq!(d.slice(4, 10), "second");
    }
}

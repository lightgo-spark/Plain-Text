//! The quality gate: a few thousand checks over the parts of the reader that
//! have no window in them.
//!
//! The unit tests each pin one behaviour with one example. This does the other
//! thing: it takes every rule the reader is supposed to obey and runs it across
//! a matrix of documents, widths, ranges and queries, checking each answer
//! against a slower one derived a different way. A rule that only holds for the
//! example someone thought of is not a rule, and this is where that shows.
//!
//! Every check is counted, so "the gate passed" is a number and not a feeling.

use crate::gui::layout::{self, Metrics, RowKind, Setup};
use crate::import;
use crate::library::{self, Bookmark, BookRecord, Highlight, Ink, Library};
use crate::text::{matches_in, Document, Match};
use std::path::PathBuf;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// A run of the gate: how many checks were made and which of them failed.
#[derive(Default)]
pub struct Gate {
    pub checks: usize,
    pub failures: Vec<String>,
    /// Checks made, per area, for the summary table.
    pub areas: Vec<(String, usize, usize)>,
    area: Option<(String, usize, usize)>,
}

impl Gate {
    pub fn new() -> Gate {
        Gate::default()
    }

    fn area(&mut self, name: &str) {
        self.close_area();
        self.area = Some((name.to_string(), 0, 0));
    }

    fn close_area(&mut self) {
        if let Some(a) = self.area.take() {
            self.areas.push(a);
        }
    }

    /// Record one check.
    fn check(&mut self, ok: bool, what: impl FnOnce() -> String) {
        self.checks += 1;
        if let Some(a) = &mut self.area {
            a.1 += 1;
        }
        if !ok {
            let msg = what();
            if let Some(a) = &mut self.area {
                a.2 += 1;
            }
            // A gate that prints ten thousand lines is not read. Keep the
            // first failures of each kind; the count is exact regardless.
            if self.failures.len() < 60 {
                self.failures.push(msg);
            }
        }
    }

    fn eq<T: PartialEq + std::fmt::Debug>(&mut self, got: T, want: T, what: &str) {
        let ok = got == want;
        self.check(ok, || format!("{what}: got {got:?}, wanted {want:?}"));
    }

    fn ok(&mut self, cond: bool, what: &str) {
        self.check(cond, || what.to_string());
    }

    pub fn passed(&self) -> bool {
        self.failures.is_empty()
    }
}

/// Fake font, so the gate measures the typesetter and not whatever faces this
/// machine happens to have: 10pt a narrow glyph, 20pt a wide one.
fn fake(s: &str) -> f32 {
    s.graphemes(true)
        .map(|g| if UnicodeWidthStr::width(g) > 1 { 20.0 } else { 10.0 })
        .sum()
}

fn doc(s: &str) -> Document {
    Document::from_string(s.to_string(), &PathBuf::from("qa.txt"), "UTF-8")
}

/// The documents the gate runs everything against. Between them they hold the
/// shapes that have broken the reader before: trailing spaces, CRLF, tabs,
/// blank-only lines, Korean, mixed scripts, headings, and nothing at all.
fn corpus() -> Vec<(&'static str, String)> {
    vec![
        ("empty", String::new()),
        ("blank lines only", "\n\n\n".into()),
        ("one word", "word".into()),
        ("latin prose", "The quick brown fox jumps over the lazy dog. \
It jumps again, and then it stops.\n\nA second paragraph follows here.\n".into()),
        ("trailing spaces", "abc   \nnext line  \n\n   \nlast\n".into()),
        ("crlf", "mixed\r\nline\r\nendings   \r\n".into()),
        ("tabs", "tabs\there  \nand\tmore \t\n".into()),
        // The Hangul entries below are fixtures, not prose. They are the only
        // way to exercise what the reader does differently for Korean: EUC-KR
        // and UTF-16 decoding, composing jamo to NFC, justifying by stretching
        // between glyphs rather than between words, breaking a line where there
        // is no space to break at, and reading `제 N 장` as a heading. Replacing
        // them with English would leave every one of those rules unchecked.
        ("korean", "읽지 않은 책이 쌓인 서가를 안티라이브러리라 부른다. \
이미 읽은 책보다 아직 읽지 않은 책이 더 많다.\n\n서재는 자랑이 아니라 도구다.\n".into()),
        ("mixed scripts", "한글 and latin mixed 함께 in one line 이렇게\n\n두 번째 문단\n".into()),
        ("headings", "Chapter 1\n\nBody of the first chapter here.\n\n제 2 장\n\n\
두 번째 장의 본문이다.\n\n# Third\n\nAnd its body.\n".into()),
        ("case pairs", "Istanbul ISTANBUL istanbul Grüße GRÜSSE grüße Straße STRASSE\n".into()),
        ("repeats", "aaa bbb aaa ccc aaa\n\n".repeat(12)),
        ("long line", "word ".repeat(200)),
        ("no spaces", "가나다라마바사아자차카타파하".repeat(8)),
        ("punctuation", "\"Quoted,\" he said — then (parenthetically) stopped; didn't he?\n".into()),
        ("numbers", "1. First item\n2. Second item\n10. Tenth item\n".into()),
        ("one long word", "a".repeat(300)),
        ("single newline", "\n".into()),
        ("leading blanks", "\n\n\nThe text starts only here.\n".into()),
        ("invisibles", "co\u{ad}operate with the te\u{200b}am\u{feff} today\n".into()),
    ]
}

/// Every occurrence of `needle`, found the slow way. Shares no code with the
/// search it is checking.
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
            let hit = (i + 1..=chars.len()).find(|&j| {
                let folded: Vec<char> = chars[i..j].iter().flat_map(|c| c.to_lowercase()).collect();
                folded == q
            });
            match hit {
                Some(j) => {
                    out.push(Match { start: p.offset + i, end: p.offset + j });
                    i = j;
                }
                None => i += 1,
            }
        }
    }
    out
}

/// Run the whole gate.
pub fn run() -> Gate {
    let mut g = Gate::new();
    search(&mut g);
    ranges(&mut g);
    slicing(&mut g);
    wrapping(&mut g);
    linebreaking(&mut g);
    pagination(&mut g);
    selection(&mut g);
    justification(&mut g);
    importing(&mut g);
    persistence(&mut g);
    presentation(&mut g);
    g.close_area();
    g
}

// -------------------------------------------------------------------------

/// The search must answer for the document, not for the column. Every query is
/// checked against a brute force search, and every match against the text it
/// claims to have found.
fn search(g: &mut Gate) {
    g.area("search");
    const QUERIES: &[&str] = &[
        "the", "The", "THE", "quick brown", "lazy dog", "jumps over the",
        "읽지", "안티라이브러리", "쌓인 서가", "책보다", "서재는",
        "istanbul", "ISTANBUL", "grüße", "GRÜSSE", "straße",
        "aaa", "aaa bbb", "word", "가나다", "라마바사", "cooperate", "team",
        "chapter", "제 2 장", "first", "10.", "\"quoted,\"", "didn't",
        "not in any of these documents at all", "a", " ", "",
    ];
    for (name, src) in corpus() {
        let d = doc(&src);
        for q in QUERIES {
            let got = d.search(q);
            let want = brute_force(&d, q);
            g.check(got == want, || {
                format!("search {name} / {q:?}: got {} hits, wanted {}", got.len(), want.len())
            });
            // Whatever it found has to be the words it was asked for.
            for m in &got {
                let text = d.slice(m.start, m.end);
                g.check(text.to_lowercase() == q.to_lowercase(), || {
                    format!("search {name} / {q:?}: matched {text:?}")
                });
                g.check(m.start < m.end, || format!("search {name} / {q:?}: empty range"));
                g.check(m.end <= d.chars, || {
                    format!("search {name} / {q:?}: {m:?} runs past the document")
                });
            }
            // Ordered, and never overlapping.
            for w in got.windows(2) {
                g.check(w[0].end <= w[1].start, || {
                    format!("search {name} / {q:?}: {:?} overlaps {:?}", w[0], w[1])
                });
            }
        }
    }

    // The answer must not depend on the width of the column. This is the defect
    // the document search replaced: the reader searched the wrapped rows, so a
    // phrase split by the wrap was reported as no match at all.
    let cases: &[(&str, &str)] = &[
        ("the quick brown fox jumps over the lazy dog\n", "quick brown"),
        ("the quick brown fox jumps over the lazy dog\n", "jumps over the lazy"),
        ("읽지 않은 책이 쌓인 서가를 안티라이브러리라 부른다.\n", "안티라이브러리"),
        ("읽지 않은 책이 쌓인 서가를 안티라이브러리라 부른다.\n", "쌓인 서가를"),
        ("한글 and latin mixed 함께 in one line 이렇게\n", "latin mixed 함께"),
    ];
    for (src, q) in cases {
        let d = doc(src);
        let expected = d.search(q);
        g.eq(expected.len(), 1, &format!("{q:?} should occur once"));
        for width in [60.0f32, 80.0, 100.0, 140.0, 180.0, 240.0, 300.0, 420.0, 600.0, 900.0] {
            let l = typeset_at(&d, width, 4000.0);
            // Found, and reachable: the rows the hit falls on must exist.
            let hits = matches_in(&expected, 0, d.chars);
            g.eq(hits.len(), 1, &format!("{q:?} at width {width}"));
            let m = expected[0];
            let row = l.row_of_offset(m.start);
            g.check(row < l.rows.len(), || {
                format!("{q:?} at width {width}: no row holds offset {}", m.start)
            });
            // And the text really is on the page there.
            let rebuilt = layout::extract(&l.rows, m.start, m.end);
            g.check(rebuilt.to_lowercase() == q.to_lowercase(), || {
                format!("{q:?} at width {width}: the page shows {rebuilt:?}")
            });
        }
    }
}

/// `matches_in` binary searches an ordered list. It has to return exactly what
/// a filter would.
fn ranges(g: &mut Gate) {
    g.area("match ranges");
    let d = doc(&"aaa bbb ccc\n\n".repeat(24));
    let hits = d.search("aaa");
    g.ok(!hits.is_empty(), "the range corpus holds no matches");
    for start in (0..d.chars).step_by(3) {
        for len in [0usize, 1, 2, 7, 13, 40, 200] {
            let end = (start + len).min(d.chars);
            let got = matches_in(&hits, start, end);
            let want: Vec<Match> = hits
                .iter()
                .copied()
                .filter(|m| m.start < end && start < m.end)
                .collect();
            g.check(got == &want[..], || {
                format!("matches_in {start}..{end}: got {} wanted {}", got.len(), want.len())
            });
        }
    }
    // An empty list, and a range past the end, must both be quiet.
    let none: Vec<Match> = Vec::new();
    for (a, b) in [(0usize, 0usize), (0, 10), (100, 200), (5, 5)] {
        g.eq(matches_in(&none, a, b).len(), 0, "empty match list");
        g.ok(matches_in(&hits, d.chars + a, d.chars + b + 1).is_empty(), "past the end");
    }
}

/// A slice must be exactly the characters it was asked for — the invariant the
/// whole highlighting system rests on.
fn slicing(g: &mut Gate) {
    g.area("slicing");
    for (name, src) in corpus() {
        let d = doc(&src);
        let norm: String = src
            .replace("\r\n", "\n")
            .replace('\r', "\n")
            .replace('\t', "    ")
            .chars()
            .filter(|c| !matches!(c, '\u{ad}' | '\u{200b}' | '\u{feff}'))
            .collect();
        let all: Vec<char> = norm.chars().collect();
        g.eq(d.chars, all.len(), &format!("char count of {name}"));
        // Exhaustive on the small ones, sampled on the large.
        let step = if all.len() <= 48 { 1 } else { all.len() / 24 + 1 };
        let mut a = 0usize;
        while a <= all.len() {
            let mut b = a;
            while b <= all.len() {
                let want: String = all[a..b].iter().collect();
                let got = d.slice(a, b);
                g.check(got == want, || {
                    format!("slice {name} {a}..{b}: got {got:?} wanted {want:?}")
                });
                b += step;
            }
            a += step;
        }
        // Out of range on either side is empty, never a panic.
        g.eq(d.slice(all.len() + 5, all.len() + 9), String::new(), "slice past the end");
        g.eq(d.slice(9, 3), String::new(), "reversed slice");
    }
}

/// Wrapping may move characters between rows; it may never lose one, and the
/// rows may never be wider than the column.
fn wrapping(g: &mut Gate) {
    g.area("wrapping");
    for (name, src) in corpus() {
        for width in [40.0f32, 70.0, 110.0, 160.0, 250.0, 400.0] {
            let d = doc(&src);
            let l = typeset_at(&d, width, 4000.0);
            for r in &l.rows {
                if r.kind == RowKind::Blank {
                    continue;
                }
                let w = fake(&r.text) + r.indent;
                // One glyph of slack: a row holding a single chunk wider than
                // the whole column has nowhere else to put it.
                let single = r.text.graphemes(true).count() <= 1;
                g.check(w <= width + 0.01 || single, || {
                    format!("wrap {name}@{width}: row {:?} is {w}pt wide", r.text)
                });
                // The row must say where its own text is.
                let (a, b) = r.range();
                g.check(b <= d.chars, || {
                    format!("wrap {name}@{width}: row range {a}..{b} past {}", d.chars)
                });
                let from_doc = d.slice(a, b);
                g.check(from_doc == r.text, || {
                    format!("wrap {name}@{width}: row {:?} but document says {from_doc:?}", r.text)
                });
            }
            // Row offsets never go backwards.
            let mut prev = 0usize;
            for r in &l.rows {
                g.check(r.offset >= prev, || {
                    format!("wrap {name}@{width}: offset went {prev} -> {}", r.offset)
                });
                prev = r.offset;
            }
        }
        // The character-cell wrap the terminal uses, held to the same rule.
        let d = doc(&src);
        for cols in [12usize, 20, 33, 56, 80] {
            let lines = d.wrap(cols, true);
            let joined: String = lines
                .iter()
                .map(|l| l.text.replace(' ', ""))
                .collect();
            let expected: String = src
                .replace("\r\n", "\n")
                .replace('\r', "\n")
                .replace('\t', "    ")
                .chars()
                .filter(|c| !matches!(c, '\u{ad}' | '\u{200b}' | '\u{feff}'))
                .filter(|c| !c.is_whitespace())
                .collect();
            g.check(joined == expected, || {
                format!("terminal wrap {name}@{cols}: {} chars, wanted {}", joined.chars().count(), expected.chars().count())
            });
        }
    }
}

fn typeset_at(d: &Document, width: f32, height: f32) -> layout::Layout {
    layout::typeset(
        d,
        &Setup {
            width,
            height,
            metrics: Metrics::default(),
            justify: true,
            drop_caps: true,
            chapter_breaks: true,
            hyphenate: true,
        },
        &fake,
        &fake,
    )
}

/// The line breaker looks at the whole paragraph, so the rules it has to obey
/// are about the paragraph: nothing may be lost, nothing may be invented, and
/// no line may end up wider than the column it was broken for.
fn linebreaking(g: &mut Gate) {
    g.area("line breaking");
    use crate::gui::linebreak::break_lines;

    // Words nothing can hyphenate, including one long enough that the
    // hyphenator used to take the whole reader down with it.
    let awkward = [
        "https://example.com/a/path/that/is/really/quite/long/indeed?query=1&more=2",
        &"a".repeat(300),
        "antidisestablishmentarianism",
        "Rindfleischetikettierungsuberwachungsaufgabenubertragungsgesetz",
        "가나다라마바사아자차카타파하".repeat(6).as_str(),
    ]
    .map(String::from);

    let sources: Vec<String> = corpus()
        .into_iter()
        .map(|(_, s)| s.lines().map(String::from).collect::<Vec<_>>().join(" "))
        .chain(awkward)
        .collect();

    for (n, src) in sources.iter().enumerate() {
        for width in [40.0f32, 70.0, 120.0, 200.0, 380.0, 700.0] {
            for hyphens in [false, true] {
                let lines = break_lines(src, &|_| width, &fake, hyphens);
                g.ok(!lines.is_empty(), &format!("doc {n}@{width}: no lines at all"));

                // Every character comes back, and none is invented.
                let got: String = lines
                    .iter()
                    .flat_map(|l| l.text.chars())
                    .filter(|c| !c.is_whitespace())
                    .collect();
                let want: String = src.chars().filter(|c| !c.is_whitespace()).collect();
                g.check(got == want, || {
                    format!(
                        "doc {n}@{width} (hyphens {hyphens}): {} characters back, wanted {}",
                        got.chars().count(),
                        want.chars().count()
                    )
                });

                let chars: Vec<char> = src.chars().collect();
                for l in &lines {
                    // No line may overhang, unless it holds a single thing that
                    // is itself wider than the column.
                    let w = fake(&l.text) + if l.hyphen { fake("-") } else { 0.0 };
                    let single = l.text.graphemes(true).count() <= 1;
                    g.check(w <= width + 0.01 || single, || {
                        format!("doc {n}@{width}: {:?} is {w}pt", l.text)
                    });
                    // The line is the paragraph at its own offset — the
                    // invariant selection and highlighting stand on. A hyphen
                    // is drawn, never stored, so it must not appear here.
                    let end = l.offset + l.text.chars().count();
                    let says: String = chars[l.offset.min(chars.len())..end.min(chars.len())]
                        .iter()
                        .collect();
                    g.check(says == l.text, || {
                        format!("doc {n}@{width}: line {:?} sits where the text says {says:?}", l.text)
                    });
                    g.check(!l.text.ends_with('\u{ad}'), || {
                        format!("doc {n}@{width}: a soft hyphen reached a line")
                    });
                }
                // Lines run forwards through the paragraph.
                for w in lines.windows(2) {
                    g.check(w[1].offset >= w[0].offset, || {
                        format!("doc {n}@{width}: offsets went backwards")
                    });
                }
            }

            // Hyphenation changes where the lines break, never what they say.
            let plain: String = break_lines(src, &|_| width, &fake, false)
                .iter()
                .flat_map(|l| l.text.chars())
                .filter(|c| !c.is_whitespace())
                .collect();
            let hyphenated: String = break_lines(src, &|_| width, &fake, true)
                .iter()
                .flat_map(|l| l.text.chars())
                .filter(|c| !c.is_whitespace())
                .collect();
            g.check(plain == hyphenated, || {
                format!("doc {n}@{width}: hyphenation changed the text")
            });
        }
        // A column whose width changes from line to line — the well of a drop
        // cap — is the case a line-at-a-time breaker never had to think about.
        let lines = break_lines(src, &|i| if i < 3 { 90.0 } else { 260.0 }, &fake, true);
        for (i, l) in lines.iter().enumerate() {
            let limit = if i < 3 { 90.0 } else { 260.0 };
            let w = fake(&l.text) + if l.hyphen { fake("-") } else { 0.0 };
            let single = l.text.graphemes(true).count() <= 1;
            g.check(w <= limit + 0.01 || single, || {
                format!("doc {n} drop cap: line {i} is {w}pt in a {limit}pt column")
            });
        }
    }

    // Looking at the paragraph should not leave a line nearly empty while its
    // neighbours are full — the whole reason for doing it this way.
    let prose = "The quick brown fox jumps over the lazy dog near the riverbank \
and then it turns around and walks slowly back again to where it started";
    for width in [150.0f32, 220.0, 310.0] {
        let lines = break_lines(prose, &|_| width, &fake, true);
        if lines.len() < 2 {
            continue;
        }
        let shortest = lines[..lines.len() - 1]
            .iter()
            .map(|l| fake(&l.text))
            .fold(f32::INFINITY, f32::min);
        g.check(shortest > width * 0.5, || {
            format!("at {width}pt a line was left {shortest}pt long while others were full")
        });
    }
}

/// Pages must hold every row exactly once, fit the height they were given, and
/// never open on white space.
fn pagination(g: &mut Gate) {
    g.area("pagination");
    for (name, src) in corpus() {
        for height in [80.0f32, 140.0, 260.0, 500.0] {
            for width in [90.0f32, 200.0, 380.0] {
                let d = doc(&src);
                let l = typeset_at(&d, width, height);
                g.ok(!l.pages.is_empty(), &format!("{name}@{width}x{height}: no pages"));

                let readable = l.rows.iter().any(|r| r.kind != RowKind::Blank);
                let mut covered = vec![0usize; l.rows.len()];
                for p in &l.pages {
                    if p.is_empty() {
                        // A document with no text at all still gets a leaf to
                        // show. Nothing else may be empty.
                        g.check(!readable, || {
                            format!("{name}@{width}x{height}: an empty page in a book with text")
                        });
                        continue;
                    }
                    let h: f32 = l.rows[p.start..p.end].iter().map(|r| r.height).sum();
                    // A page holding a single row taller than the column has
                    // nowhere else to put it.
                    g.check(h <= height + 0.01 || p.len() <= 1, || {
                        format!("{name}@{width}x{height}: page holds {h}pt")
                    });
                    g.check(
                        l.rows.get(p.start).is_none_or(|r| r.kind != RowKind::Blank),
                        || format!("{name}@{width}x{height}: page opens on a blank row"),
                    );
                    for c in covered.iter_mut().take(p.end).skip(p.start) {
                        *c += 1;
                    }
                }
                for (i, c) in covered.iter().enumerate() {
                    if l.rows[i].kind != RowKind::Blank {
                        g.check(*c == 1, || {
                            format!("{name}@{width}x{height}: row {i} covered {c} times")
                        });
                    }
                }
                // Every page has something to read on it.
                for (i, p) in l.pages.iter().enumerate() {
                    g.check(
                        !readable || l.rows[p.start..p.end].iter().any(|r| r.kind != RowKind::Blank),
                        || format!("{name}@{width}x{height}: page {i} is empty"),
                    );
                }
                // Setting the book a piece at a time must land where setting it
                // in one go does.
                let mut ts = layout::Typesetter::new(Setup {
                    width,
                    height,
                    metrics: Metrics::default(),
                    justify: true,
                    drop_caps: true,
                    chapter_breaks: true,
                    hyphenate: true,
                });
                while !ts.step(&d, 3, &fake, &fake) {}
                g.eq(ts.layout.rows.len(), l.rows.len(), &format!("{name}: row count in steps"));
                g.eq(ts.layout.pages.len(), l.pages.len(), &format!("{name}: page count in steps"));
                for (a, b) in ts.layout.pages.iter().zip(l.pages.iter()) {
                    g.eq(a, b, &format!("{name}: page break in steps"));
                }
                // Row tops stack to the total height.
                let sum: f32 = l.rows.iter().map(|r| r.height).sum();
                g.check((l.total_height() - sum).abs() < 0.01, || {
                    format!("{name}@{width}x{height}: tops sum to {} not {sum}", l.total_height())
                });
                // A page can be found from any offset inside it.
                for p in l.pages.iter().take(6) {
                    let off = l.rows[p.start].offset;
                    g.eq(l.page_of_offset(off), l.page_of_row(p.start), &format!("{name}: page of offset"));
                }
            }
        }
    }
}

/// Copying a selection must give back the words that were on the page.
fn selection(g: &mut Gate) {
    g.area("selection");
    for (name, src) in corpus() {
        let d = doc(&src);
        for width in [80.0f32, 160.0, 320.0] {
            let l = typeset_at(&d, width, 3000.0);
            for r in l.rows.iter().take(24) {
                if r.kind == RowKind::Blank || r.text.is_empty() {
                    continue;
                }
                let (a, b) = r.range();
                let got = layout::extract(&l.rows, a, b);
                g.check(got == r.text, || {
                    format!("extract {name}@{width}: {got:?} for row {:?}", r.text)
                });
                // Any part of a row comes back as that part of it.
                let chars: Vec<char> = r.text.chars().collect();
                for cut in [1usize, chars.len() / 2, chars.len().saturating_sub(1)] {
                    if cut == 0 || cut > chars.len() {
                        continue;
                    }
                    let want: String = chars[..cut].iter().collect();
                    let got = layout::extract(&l.rows, a, a + cut);
                    g.check(got == want, || {
                        format!("extract part {name}@{width}: {got:?} wanted {want:?}")
                    });
                }
                // Clipping a row to a range that misses it gives nothing.
                g.ok(r.clip(b, b + 5).is_none(), "clip past the row");
                g.ok(r.clip(a.saturating_sub(5), a).is_none(), "clip before the row");
            }
        }
    }
}

/// A justified row may be stretched, but never past the point where the gaps
/// become holes.
fn justification(g: &mut Gate) {
    g.area("justification");
    let rows = [
        "the quick brown fox jumps",
        "읽지 않은 책이 쌓인 서가를",
        "한글 and latin mixed 함께",
        "oneverylongwordwithnospaces",
        "가나다라마바사아자차",
        "a b c d e f g h i j k l",
    ];
    for text in rows {
        let w = fake(text);
        for target in [w, w + 4.0, w + 20.0, w + 90.0, w + 400.0] {
            for (max_gap, max_letter) in [(4.0f32, 1.5f32), (8.0, 3.0), (20.0, 9.0)] {
                let s = layout::stretch(text, w, target, max_gap, max_letter);
                match s {
                    layout::Stretch::WordGaps(extra) => {
                        g.check(extra <= max_gap + 0.001 && extra > 0.0, || {
                            format!("stretch {text:?}: word gap {extra} over {max_gap}")
                        });
                        g.ok(!layout::cjk_heavy(text), "word gaps used on a CJK row");
                    }
                    layout::Stretch::Letters(extra) => {
                        g.check(extra <= max_letter + 0.001 && extra > 0.0, || {
                            format!("stretch {text:?}: letter gap {extra} over {max_letter}")
                        });
                        g.ok(layout::cjk_heavy(text), "letter spacing used on a latin row");
                    }
                    layout::Stretch::None => {
                        g.ok(true, "ragged is always allowed");
                    }
                }
                // Never stretch a row that already fills the measure.
                if target <= w {
                    g.eq(
                        format!("{s:?}"),
                        format!("{:?}", layout::Stretch::None),
                        &format!("stretch {text:?} with no slack"),
                    );
                }
            }
        }
    }
}

/// What comes out of a document format has to be the words that went in.
fn importing(g: &mut Gate) {
    g.area("importing");

    // An archive says nothing trustworthy about how much it holds. Before this
    // ceiling a 2 MB EPUB reached 3.6 GB of working set in eight seconds and
    // was still climbing; the reader has to turn such a document away, say that
    // its size is the reason, and go on opening ordinary ones.
    {
        use std::io::Write;
        let room = std::env::temp_dir().join("antilib-qa-size");
        let _ = std::fs::remove_dir_all(&room);
        std::fs::create_dir_all(&room).unwrap();

        let build = |path: &std::path::Path, bytes: usize| {
            let file = std::fs::File::create(path).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let opts: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            zip.start_file("word/document.xml", opts).unwrap();
            zip.write_all(b"<w:document><w:body><w:p><w:t>Prose.</w:t></w:p></w:body>").unwrap();
            let block = vec![b' '; 1024 * 1024];
            let mut written = 0usize;
            while written < bytes {
                let n = block.len().min(bytes - written);
                zip.write_all(&block[..n]).unwrap();
                written += n;
            }
            zip.write_all(b"</w:document>").unwrap();
            zip.finish().unwrap();
        };

        // Just over the ceiling, and comfortably under it.
        let over = room.join("over.docx");
        build(&over, (import::MAX_ENTRY_BYTES + 4 * 1024 * 1024) as usize);
        let small = std::fs::metadata(&over).map(|m| m.len()).unwrap_or(u64::MAX);
        g.ok(small < 8 * 1024 * 1024, "the bomb was not small on disk, so it proves nothing");
        match Document::load(&over) {
            Ok(_) => g.ok(false, "an archive over the ceiling was read as a document"),
            Err(e) => g.ok(
                format!("{e:#}").contains("larger than this reader will open"),
                &format!("refused, but not for its size — the reason matters: {e:#}"),
            ),
        }

        let under = room.join("under.docx");
        build(&under, 1024 * 1024);
        match Document::load(&under) {
            Ok(doc) => g.ok(doc.chars > 0, "an ordinary archive came back empty"),
            Err(e) => g.ok(false, &format!("the ceiling turned away an ordinary document: {e:#}")),
        }
        let _ = std::fs::remove_dir_all(&room);
    }
    // Every named entity the reader knows must come back as its character, and
    // one it does not know must come back whole rather than vanish.
    let entities: &[(&str, &str)] = &[
        ("&amp;", "&"), ("&lt;", "<"), ("&gt;", ">"), ("&quot;", "\""),
        ("&apos;", "'"), ("&nbsp;", "\u{a0}"), ("&ndash;", "\u{2013}"),
        ("&mdash;", "\u{2014}"), ("&lsquo;", "\u{2018}"), ("&rsquo;", "\u{2019}"),
        ("&ldquo;", "\u{201c}"), ("&rdquo;", "\u{201d}"), ("&hellip;", "\u{2026}"),
        ("&bull;", "\u{2022}"), ("&copy;", "\u{a9}"), ("&reg;", "\u{ae}"),
        ("&trade;", "\u{2122}"), ("&deg;", "\u{b0}"), ("&plusmn;", "\u{b1}"),
        ("&times;", "\u{d7}"), ("&divide;", "\u{f7}"), ("&euro;", "\u{20ac}"),
        ("&pound;", "\u{a3}"), ("&yen;", "\u{a5}"), ("&cent;", "\u{a2}"),
        ("&larr;", "\u{2190}"), ("&rarr;", "\u{2192}"), ("&ne;", "\u{2260}"),
        ("&le;", "\u{2264}"), ("&ge;", "\u{2265}"), ("&frac12;", "\u{bd}"),
        ("&#65;", "A"), ("&#x41;", "A"), ("&#X41;", "A"), ("&#8217;", "\u{2019}"),
        ("&#233;", "\u{e9}"), ("&#x2014;", "\u{2014}"), ("&#44032;", "가"),
    ];
    for (src, want) in entities {
        let out = import::strip_html(&format!("<p>[{src}]</p>"));
        g.check(out.trim() == format!("[{want}]"), || {
            format!("entity {src}: got {:?} wanted [{want}]", out.trim())
        });
    }
    // An entity the table does not hold must survive, not disappear.
    for unknown in ["&notarealentity;", "&zzz;", "&#xZZ;"] {
        let out = import::strip_html(&format!("<p>a{unknown}b</p>"));
        g.check(out.contains('a') && out.contains('b'), || {
            format!("unknown entity {unknown} took its neighbours: {out:?}")
        });
    }
    // Scripts and styles are not text; block elements break paragraphs.
    let page = "<html><head><title>t</title></head><body><script>var x=1;</script>\
<style>p{color:red}</style><h1>Heading</h1><p>First para.</p><p>Second para.</p>\
<div>Third</div><ul><li>one</li><li>two</li></ul></body></html>";
    let out = import::strip_html(page);
    for gone in ["var x=1", "color:red", "<script", "<p>"] {
        g.check(!out.contains(gone), || format!("strip_html kept {gone:?}: {out:?}"));
    }
    for kept in ["Heading", "First para.", "Second para.", "Third", "one", "two"] {
        g.check(out.contains(kept), || format!("strip_html lost {kept:?}: {out:?}"));
    }
    g.check(
        out.lines().filter(|l| l.contains("First para.")).count() == 1,
        || format!("a paragraph was doubled: {out:?}"),
    );

    // RTF: the markup is not the book.
    let bs = '\\';
    let cases: Vec<(String, &str)> = vec![
        (format!("{{{bs}rtf1{bs}ansi Hello world.{bs}par}}"), "Hello world."),
        (
            format!("{{{bs}rtf1{bs}ansi{{{bs}fonttbl{{{bs}f0 Arial;}}}}Body text.{bs}par}}"),
            "Body text.",
        ),
        (
            format!("{{{bs}rtf1{bs}ansi{{{bs}*{bs}generator Word;}}Real text.{bs}par}}"),
            "Real text.",
        ),
        (
            format!("{{{bs}rtf1{bs}ansi{bs}ansicpg1252 caf{bs}'e9 na{bs}'efve{bs}par}}"),
            "caf\u{e9} na\u{ef}ve",
        ),
        (
            format!("{{{bs}rtf1{bs}ansi{bs}ansicpg949 {bs}'c7{bs}'d1{bs}'b1{bs}'db{bs}par}}"),
            "한글",
        ),
        (
            format!("{{{bs}rtf1{bs}ansi{bs}uc2 {bs}u51069?_{bs}u51648?_ {bs}u52293?_.{bs}par}}"),
            "읽지 책.",
        ),
        (
            format!("{{{bs}rtf1{bs}ansi{bs}u51069 ?{bs}u51648 ?{bs}par}}"),
            "읽지",
        ),
    ];
    for (src, want) in &cases {
        let got = import::rtf_to_text(src);
        g.check(got.trim() == *want, || format!("rtf {src:?}: got {:?} wanted {want:?}", got.trim()));
    }
    // Whatever it is handed, the RTF reader must not hang or panic.
    for junk in [
        "{", "}", "{\\rtf", "{{{{{{", "}}}}}", "{\\rtf1\\u", "{\\rtf1\\'",
        "{\\rtf1\\ansi\\uc99999 \\u65 x}", "\\\\\\\\", "{\\*}", "",
    ] {
        let out = import::rtf_to_text(junk);
        g.check(out.len() < 10_000, || format!("rtf ran away on {junk:?}"));
    }

    // Format detection believes the bytes over the extension.
    let by_magic: &[(&str, &[u8], import::Format)] = &[
        ("mislabelled.txt", b"%PDF-1.7", import::Format::Pdf),
        ("notes.txt", b"{\\rtf1\\ansi", import::Format::Rtf),
        ("plain.txt", b"just text", import::Format::Text),
        ("page.html", b"<html>", import::Format::Html),
        ("page.htm", b"<html>", import::Format::Html),
        ("page.xhtml", b"<html>", import::Format::Html),
        ("book.pdf", b"anything", import::Format::Pdf),
        ("book.rtf", b"anything", import::Format::Rtf),
        ("readme.md", b"# title", import::Format::Text),
    ];
    for (name, head, want) in by_magic {
        g.eq(import::detect(&PathBuf::from(name), head), *want, &format!("detect {name}"));
    }
    // Binary data is refused; text — including UTF-16, which is full of NULs —
    // is not.
    g.ok(import::looks_binary(&[0u8, 1, 2, 3]), "a NUL run is binary");
    g.ok(!import::looks_binary(b"ordinary text"), "text is not binary");
    g.ok(!import::looks_binary(&[]), "nothing is not binary");
    let mut utf16 = vec![0xFFu8, 0xFE];
    for u in "안녕".encode_utf16() {
        utf16.extend_from_slice(&u.to_le_bytes());
    }
    g.ok(!import::looks_binary(&utf16), "a UTF-16 file is text");

    // Reflow joins the lines a page broke, without inventing spaces in Korean.
    let reflowed = import::reflow("This line was\nbroken by the page.\n\nNew paragraph.\n");
    g.check(reflowed.contains("This line was broken by the page."), || {
        format!("reflow lost the join: {reflowed:?}")
    });
    let korean = import::reflow("읽지 않은 책이\n쌓인 서가를 안티\n라이브러리라 한다.\n");
    g.check(!korean.contains("않은 책이쌓인"), || format!("reflow ran words together: {korean:?}"));
}

/// Everything the reader saves has to come back the way it went in.
fn persistence(g: &mut Gate) {
    g.area("persistence");
    let dir = std::env::temp_dir().join("antilib-qa");
    let _ = std::fs::create_dir_all(&dir);

    // Round trip a library holding every shape of record.
    let store = dir.join("qa-library.json");
    let _ = std::fs::remove_file(&store);
    let mut lib = Library::load_from(store.clone());
    for (i, ink) in Ink::ALL.iter().enumerate() {
        let key = format!("book-{i}.txt");
        let rec = lib.record(&key);
        rec.offset = i * 1000;
        rec.title = format!("Book {i}");
        rec.last_opened = 100 + i as u64;
        for b in 0..4 {
            rec.bookmarks.push(Bookmark { offset: b * 50, label: format!("mark {b}") });
        }
        for h in 0..4 {
            rec.highlights.push(Highlight {
                start: h * 100,
                end: h * 100 + 20,
                ink: *ink,
                text: format!("passage {h}"),
                note: if h % 2 == 0 { String::new() } else { format!("note {h}") },
                stale: h == 3,
            });
        }
    }
    lib.save().unwrap();
    let back = Library::load_from(store.clone());
    g.eq(back.books.len(), lib.books.len(), "book count survives a save");
    for (key, rec) in &lib.books {
        let other = back.get(key);
        g.ok(other.is_some(), &format!("{key} survived the save"));
        if let Some(o) = other {
            g.eq(o.offset, rec.offset, &format!("{key} offset"));
            g.eq(o.title.clone(), rec.title.clone(), &format!("{key} title"));
            g.eq(o.last_opened, rec.last_opened, &format!("{key} timestamp"));
            g.eq(o.bookmarks.len(), rec.bookmarks.len(), &format!("{key} bookmark count"));
            g.eq(o.highlights.len(), rec.highlights.len(), &format!("{key} highlight count"));
            for (a, b) in o.highlights.iter().zip(rec.highlights.iter()) {
                g.eq(a.start, b.start, "highlight start");
                g.eq(a.end, b.end, "highlight end");
                g.eq(a.ink, b.ink, "highlight colour");
                g.eq(a.text.clone(), b.text.clone(), "highlight text");
                g.eq(a.note.clone(), b.note.clone(), "highlight note");
                g.eq(a.stale, b.stale, "highlight staleness");
            }
        }
    }
    g.ok(!store.with_extension("json.tmp").exists(), "a temporary file was left behind");
    let _ = std::fs::remove_file(&store);

    // A corrupt or half-written store must never stop the reader reading.
    for junk in ["", "{", "not json at all", r#"{"books":"#, r#"{"books":{"a":5}}"#] {
        let p = dir.join("qa-corrupt.json");
        std::fs::write(&p, junk).unwrap();
        let lib = Library::load_from(p.clone());
        g.ok(lib.books.is_empty(), &format!("corrupt store {junk:?} yielded books"));
    }

    // ...and reading one must not be the end of it. The empty library keeps
    // the path it failed to read, so the next page turn wrote that emptiness
    // back over the file — the one unrecoverable move in the program. The
    // store has to still be there afterwards, byte for byte.
    for (n, junk) in ["", "{", "not json at all", r#"{"books":"#].iter().enumerate() {
        let room = dir.join(format!("qa-damaged-{n}"));
        let _ = std::fs::remove_dir_all(&room);
        std::fs::create_dir_all(&room).unwrap();
        let p = room.join("library.json");
        std::fs::write(&p, junk).unwrap();

        let mut lib = Library::load_from(p.clone());
        g.ok(lib.books.is_empty(), "a damaged store yielded books");
        g.ok(lib.damaged_store().is_some(), "a damaged store was not set aside");

        lib.record("book.txt").offset = 1;
        lib.save().unwrap();

        let left: Vec<String> = std::fs::read_dir(&room)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        let kept = left.iter().find(|f| f.contains("damaged"));
        g.ok(kept.is_some(), &format!("the damaged store was thrown away: {left:?}"));
        if let Some(name) = kept {
            let content = std::fs::read_to_string(room.join(name)).unwrap_or_default();
            g.eq(content, (*junk).to_string(), "the damaged store was not kept as it was");
        }
        g.ok(p.exists(), "no new library was started after the damaged one");
        let _ = std::fs::remove_dir_all(&room);
    }

    // Two readers, one file. The merge is three-way, and this is the whole of
    // what that means, over every arrangement of a single mark:
    //
    //   base   was it in the file both readers started from?
    //   mine   is it in this reader's copy now?
    //   disk   is it in the file now, after the other reader saved?
    //
    // Kept when this reader still has it; dropped when this reader had it and
    // does not now (an erase, which a union would undo); taken when the other
    // reader added it (which a plain overwrite would throw away).
    for base in [false, true] {
        for mine in [false, true] {
            for disk in [false, true] {
                let room = dir.join("qa-merge");
                let _ = std::fs::remove_dir_all(&room);
                std::fs::create_dir_all(&room).unwrap();
                let p = room.join("library.json");

                let mark = |present: bool| -> String {
                    if present {
                        r#"[{"start":10,"end":20,"ink":"Sky","text":"passage","note":"n"}]"#.into()
                    } else {
                        "[]".into()
                    }
                };
                let flag = |present: bool| -> String {
                    if present {
                        r#"[{"offset":7,"label":"m"}]"#.into()
                    } else {
                        "[]".into()
                    }
                };
                let store_of = |present: bool| {
                    format!(
                        r#"{{"books":{{"book.txt":{{"offset":1,"title":"B","last_opened":5,"bookmarks":{},"highlights":{}}}}}}}"#,
                        flag(present),
                        mark(present)
                    )
                };

                // What both readers started from.
                std::fs::write(&p, store_of(base)).unwrap();
                let mut reader = Library::load_from(p.clone());

                // What this reader did to its own copy.
                {
                    let rec = reader.record("book.txt");
                    if mine {
                        if rec.highlights.is_empty() {
                            rec.highlights.push(Highlight {
                                start: 10, end: 20, ink: Ink::Sky,
                                text: "passage".into(), note: "n".into(), stale: false,
                            });
                        }
                        if rec.bookmarks.is_empty() {
                            rec.bookmarks.push(Bookmark { offset: 7, label: "m".into() });
                        }
                    } else {
                        rec.highlights.clear();
                        rec.bookmarks.clear();
                    }
                }

                // What the other reader left in the file meanwhile.
                std::fs::write(&p, store_of(disk)).unwrap();
                reader.save().unwrap();

                let back = Library::load_from(p.clone());
                let rec = back.get("book.txt").cloned().unwrap_or_default();
                let want = if mine { true } else if base { false } else { disk };
                let case = format!("base={base} mine={mine} disk={disk}");
                g.eq(
                    !rec.highlights.is_empty(),
                    want,
                    &format!("three-way merge of a highlight ({case})"),
                );
                g.eq(
                    !rec.bookmarks.is_empty(),
                    want,
                    &format!("three-way merge of a bookmark ({case})"),
                );
                // Nothing is ever doubled by a merge.
                g.ok(rec.highlights.len() <= 1, &format!("the merge doubled a highlight ({case})"));
                g.ok(rec.bookmarks.len() <= 1, &format!("the merge doubled a bookmark ({case})"));
                let _ = std::fs::remove_dir_all(&room);
            }
        }
    }

    // The temporary file a save writes through must carry this process's name,
    // or two readers saving at once write through the same one.
    {
        let room = dir.join("qa-tmp");
        let _ = std::fs::remove_dir_all(&room);
        std::fs::create_dir_all(&room).unwrap();
        let p = room.join("library.json");
        let mut lib = Library::load_from(p.clone());
        lib.record("a.txt").offset = 1;
        lib.save().unwrap();
        let left: Vec<String> = std::fs::read_dir(&room)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        g.eq(left.len(), 1, &format!("a save left more than the library behind: {left:?}"));
        let _ = std::fs::remove_dir_all(&room);
    }

    // Merging two records for one book keeps everyone's work.
    for (a_time, b_time, wanted_offset) in [(100u64, 200u64, 22usize), (200, 100, 11), (0, 0, 11)] {
        let mut first = BookRecord {
            offset: 11,
            last_opened: a_time,
            title: "First".into(),
            bookmarks: vec![Bookmark { offset: 1, label: "a".into() }],
            highlights: vec![Highlight {
                start: 0, end: 5, ink: Ink::Yellow,
                text: "x".into(), note: String::new(), stale: false,
            }],
        };
        let second = BookRecord {
            offset: 22,
            last_opened: b_time,
            title: "Second".into(),
            bookmarks: vec![Bookmark { offset: 2, label: "b".into() }],
            highlights: vec![Highlight {
                start: 9, end: 12, ink: Ink::Mint,
                text: "y".into(), note: "kept".into(), stale: false,
            }],
        };
        first.absorb(second);
        g.eq(first.bookmarks.len(), 2, "absorb kept both bookmarks");
        g.eq(first.highlights.len(), 2, "absorb kept both highlights");
        g.eq(first.offset, wanted_offset, "absorb took the later reading position");
        g.ok(
            first.highlights.windows(2).all(|w| w[0].start <= w[1].start),
            "absorb left the highlights out of order",
        );
    }
    // Absorbing a copy of itself must not double anything.
    let mut one = BookRecord {
        offset: 1, last_opened: 5, title: "T".into(),
        bookmarks: vec![Bookmark { offset: 3, label: "m".into() }],
        highlights: vec![Highlight {
            start: 0, end: 2, ink: Ink::Sky, text: "t".into(),
            note: String::new(), stale: false,
        }],
    };
    let copy = one.clone();
    one.absorb(copy);
    g.eq(one.bookmarks.len(), 1, "absorbing a copy doubled the bookmarks");
    g.eq(one.highlights.len(), 1, "absorbing a copy doubled the highlights");

    // One file, one key, however the reader was pointed at it.
    let file = dir.join("qa-book.txt");
    std::fs::write(&file, "text").unwrap();
    let settled = library::key_for(&file);
    let variants = [
        file.clone(),
        std::fs::canonicalize(&file).unwrap(),
        dir.join(".").join("qa-book.txt"),
        dir.join("..").join("antilib-qa").join("qa-book.txt"),
    ];
    for v in &variants {
        g.eq(library::key_for(v), settled.clone(), &format!("key for {}", v.display()));
    }
    g.ok(!settled.starts_with(r"\\?\"), "the verbatim prefix reached the key");
    // And a document reads back under that same key.
    let d = Document::load(&file).unwrap();
    g.eq(d.path.clone(), settled.clone(), "the document filed itself elsewhere");
    let _ = std::fs::remove_file(&file);

    // Settings clamp anything a hand-edited file could hold.
    use crate::gui::settings::{Settings, MAX_FONT, MIN_FONT};
    // `Settings` keeps the file it came from private, so the struct update
    // syntax this lint asks for is not available outside its module.
    #[allow(clippy::field_reassign_with_default)]
    for size in [-100.0f32, 0.0, 1.0, 11.9, 12.0, 30.0, 64.0, 65.0, 1e9] {
        let mut s = Settings::default();
        s.font_size = size;
        let s = s.sanitised();
        g.check(s.font_size >= MIN_FONT && s.font_size <= MAX_FONT, || {
            format!("font size {size} clamped to {}", s.font_size)
        });
    }
    for lead in [-5.0f32, 0.0, 1.0, 1.65, 3.0, 100.0] {
        let mut s = Settings::default();
        s.line_height = lead;
        let s = s.sanitised();
        g.check(s.line_height >= 1.1 && s.line_height <= 2.4, || {
            format!("leading {lead} clamped to {}", s.line_height)
        });
    }
    for measure in [0.0f32, 10.0, 38.0, 62.0, 96.0, 500.0] {
        let mut s = Settings::default();
        s.measure = measure;
        let s = s.sanitised();
        g.check(s.measure >= 38.0 && s.measure <= 96.0, || {
            format!("measure {measure} clamped to {}", s.measure)
        });
    }
    // An old settings file, and a partial one, must still open the reader.
    for json in [
        r#"{}"#,
        r#"{"font_size":21.0}"#,
        r#"{"font_size":21.0,"spread":false}"#,
        r#"{"mode":"Scroll","skin":"Night"}"#,
        r#"{"unknown_field":1}"#,
    ] {
        let parsed: Result<Settings, _> = serde_json::from_str(json);
        g.ok(parsed.is_ok(), &format!("settings {json} would not load"));
    }
}

/// The look of the thing, held to the numbers rather than to taste.
fn presentation(g: &mut Gate) {
    g.area("presentation");
    use crate::gui::theme::{blend, ink_colour, Skin};
    use crate::library::Ink;
    fn lum(c: egui::Color32) -> f32 {
        let f = |v: u8| {
            let s = v as f32 / 255.0;
            if s <= 0.03928 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * f(c.r()) + 0.7152 * f(c.g()) + 0.0722 * f(c.b())
    }
    for skin in Skin::ALL {
        let p = skin.palette();
        let ratio = |a, b| {
            let (x, y) = (lum(a), lum(b));
            (x.max(y) + 0.05) / (x.min(y) + 0.05)
        };
        // Body type against the stock it is printed on: AAA, because this is a
        // reader and the text is the entire product.
        g.check(ratio(p.ink, p.page) >= 7.0, || {
            format!("{}: ink on page is {:.1}:1", skin.name(), ratio(p.ink, p.page))
        });
        // The quieter inks still have to be readable, if only at AA.
        g.check(ratio(p.ink_soft, p.page) >= 4.5, || {
            format!("{}: soft ink is {:.1}:1", skin.name(), ratio(p.ink_soft, p.page))
        });
        g.check(ratio(p.ink, p.panel) >= 7.0, || {
            format!("{}: ink on panel is {:.1}:1", skin.name(), ratio(p.ink, p.panel))
        });
        g.check(ratio(p.accent, p.page) >= 3.0, || {
            format!("{}: accent on page is {:.1}:1", skin.name(), ratio(p.accent, p.page))
        });
        // The sheet has to read as a sheet against the desk.
        let apart = |a: egui::Color32, b: egui::Color32| {
            (a.r() as i32 - b.r() as i32).abs()
                + (a.g() as i32 - b.g() as i32).abs()
                + (a.b() as i32 - b.b() as i32).abs()
        };
        g.check(apart(p.page, p.canvas) >= 12 || apart(p.page_edge, p.canvas) >= 30, || {
            format!("{}: the sheet vanishes into the desk", skin.name())
        });
        // A highlighter must not swallow the words under it.
        g.check(ratio(p.ink, p.highlight) >= 3.0, || {
            format!("{}: text on a search hit is {:.1}:1", skin.name(), ratio(p.ink, p.highlight))
        });
        // The four the reader actually marks with. This gate used to stop at
        // the search wash above and never reach them, and they were the ones
        // that had drifted: at night Yellow put the body text on 4.65:1, well
        // under the AAA the same text is held to one check earlier. A mark is
        // painted behind the words and read *through*, so it answers to the
        // body standard, not to the 3:1 an ornament would.
        for ink in Ink::ALL {
            let c = ink_colour(ink, skin.is_dark());
            g.check(ratio(p.ink, c) >= 7.0, || {
                format!(
                    "{}: text on a {} highlight is {:.2}:1",
                    skin.name(),
                    ink.name(),
                    ratio(p.ink, c)
                )
            });
            // And the mark has to be visible as a mark, or the colour says
            // nothing about which passage was worth keeping.
            g.check(apart(c, p.page) >= 40, || {
                format!(
                    "{}: a {} highlight vanishes into the page",
                    skin.name(),
                    ink.name()
                )
            });
            for other in Ink::ALL {
                if other == ink {
                    continue;
                }
                // Four colours that cannot be told apart are one colour.
                let d = ink_colour(other, skin.is_dark());
                g.check(apart(c, d) >= 40, || {
                    format!(
                        "{}: {} and {} are the same highlight",
                        skin.name(),
                        ink.name(),
                        other.name()
                    )
                });
            }
        }
        // Blending stays between the two colours it was given.
        for t in [0.0f32, 0.12, 0.5, 0.88, 1.0] {
            let m = blend(p.page, p.ink, t);
            let lo = lum(p.page).min(lum(p.ink)) - 0.01;
            let hi = lum(p.page).max(lum(p.ink)) + 0.01;
            g.check(lum(m) >= lo && lum(m) <= hi, || {
                format!("{}: blend at {t} left the range", skin.name())
            });
        }
        g.eq(blend(p.page, p.ink, 0.0), p.page, "blend at 0");
        g.eq(blend(p.page, p.ink, 1.0), p.ink, "blend at 1");
        g.eq(blend(p.page, p.ink, 9.0), p.ink, "blend is clamped");
    }
    // The skins cycle, and each is its own.
    let mut s = Skin::Paper;
    let mut seen = Vec::new();
    for _ in 0..Skin::ALL.len() {
        seen.push(s.name());
        s = s.next();
    }
    g.eq(s, Skin::Paper, "the skins do not come back round");
    g.eq(seen.len(), Skin::ALL.len(), "a skin was skipped");
    g.eq(
        seen.iter().collect::<std::collections::HashSet<_>>().len(),
        Skin::ALL.len(),
        "two skins share a name",
    );
    // The view modes cycle too, and know their own shape.
    use crate::gui::settings::ViewMode;
    let mut m = ViewMode::Book;
    for _ in 0..ViewMode::ALL.len() {
        g.ok(!m.name().is_empty(), "a view mode has no name");
        g.ok(!m.hint().is_empty(), "a view mode has no hint");
        g.ok(m.columns() >= 1, "a view mode with no columns");
        m = m.next();
    }
    g.eq(m, ViewMode::Book, "the view modes do not come back round");
    for ink in Ink::ALL {
        g.ok(!ink.name().is_empty(), "an ink has no name");
    }
}

/// Print the gate's report.
pub fn report(g: &Gate) {
    println!("Anti-library quality gate");
    println!("{:-<58}", "");
    for (name, checks, failed) in &g.areas {
        println!(
            "  {name:<18} {checks:>6} checks   {}",
            if *failed == 0 { "ok".to_string() } else { format!("{failed} FAILED") }
        );
    }
    println!("{:-<58}", "");
    println!("  {:<18} {:>6} checks", "total", g.checks);
    if g.failures.is_empty() {
        println!("\n  all clear");
    } else {
        println!("\n  {} failing check(s):", g.failures.len());
        for f in &g.failures {
            println!("   - {f}");
        }
    }
}

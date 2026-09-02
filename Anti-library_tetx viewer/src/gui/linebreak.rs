//! Breaking a paragraph into lines, the way a typesetter does it.
//!
//! The reader used to decide each line on its own: fill it until the next word
//! will not fit, then start again. That is what a word processor does, and it
//! is why word processors produce a ragged right edge with holes in it — the
//! decision that makes line three look good is the one that leaves line four
//! with two words on it, and by then it is too late to go back.
//!
//! This looks at the paragraph. Every place a line *could* break is scored, and
//! the set of breaks with the lowest total cost wins, so a slightly tight line
//! here buys an evenly set one there. It is Knuth and Plass's algorithm, the one
//! TeX has used since 1981, and the reason a book page looks like a book page.
//!
//! Words are also allowed to break at their syllables, which is the other half
//! of the same job: a column this narrow without hyphenation has nowhere to put
//! the slack, and the justified lines open into rivers of white.

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// One line, ready to be set.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    /// The text of the line, exactly as it reads in the document.
    pub text: String,
    /// Character offset of the line within the paragraph.
    pub offset: usize,
    /// The line ends in the middle of a word and takes a hyphen.
    ///
    /// The hyphen is *not* part of `text`, and deliberately so: `text` has to
    /// stay the characters the document holds at `offset`, or every selection,
    /// highlight and search wash made on this line lands one character out.
    /// The painter draws the hyphen; the offsets never hear about it.
    pub hyphen: bool,
}

/// What a paragraph looks like to the line breaker.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Item {
    /// Something that must be set as a unit: a word, or one wide glyph.
    Box { width: f32, from: usize, to: usize },
    /// Space that can give and take. A line may break here, and the space
    /// itself then disappears.
    Glue { width: f32, stretch: f32, shrink: f32, from: usize, to: usize },
    /// A place a line may break at a price — the syllable joints of a word.
    /// `width` is what appears there *if* the break is taken (the hyphen).
    Penalty { width: f32, cost: f32, at: usize },
}

impl Item {
    fn width(&self) -> f32 {
        match self {
            Item::Box { width, .. } | Item::Glue { width, .. } => *width,
            Item::Penalty { .. } => 0.0,
        }
    }
    fn stretch(&self) -> f32 {
        match self {
            Item::Glue { stretch, .. } => *stretch,
            _ => 0.0,
        }
    }
    fn shrink(&self) -> f32 {
        match self {
            Item::Glue { shrink, .. } => *shrink,
            _ => 0.0,
        }
    }
}

/// How much a space may give, as a fraction of its own width.
///
/// These are the painter's limits written as ratios, and they have to be: a
/// break the breaker calls comfortable but the painter refuses to justify comes
/// out ragged in the middle of a justified page.
const GLUE_STRETCH: f32 = 0.85;
/// Nothing shrinks.
///
/// TeX lets a line pull its spaces in a little to save a break, and this
/// breaker could offer the same — but the painter cannot take it up. It knows
/// how to open a word space and how to letter-space a CJK line, and it has no
/// way to set a line tighter than the words are wide. A breaker that assumed
/// otherwise chose lines a few points over the measure, and they were simply
/// drawn past the margin: the gate caught nine of them in one run.
const GLUE_SHRINK: f32 = 0.0;
/// What one gap between two CJK glyphs may open to, as a fraction of a glyph.
const CJK_STRETCH: f32 = 0.08;
/// The price of ending a line inside a word.
const HYPHEN_COST: f32 = 50.0;
/// Extra demerits for a second hyphenated line straight after the first.
const DOUBLE_HYPHEN_DEMERITS: f32 = 3000.0;
/// How loose a line may be and still count as settable. Past this the line is
/// not offered at all — which is what keeps rivers off the page.
const MAX_RATIO: f32 = 3.0;
/// Never hyphenate a word shorter than this, nor leave fewer than
/// [`MIN_AFFIX`] characters on either side of the break.
const MIN_HYPHEN_WORD: usize = 5;
const MIN_AFFIX: usize = 2;

fn is_wide(g: &str) -> bool {
    UnicodeWidthStr::width(g) > 1
}

/// Is this a word the hyphenator can do anything with — letters only?
///
/// The length ceiling is not a nicety. `hypher` keeps the word it is working on
/// in a fixed buffer and *panics* past [`hypher::MAX_INLINE_SIZE`], so a
/// document holding one long URL — or a run of base64, or a chemical name —
/// would have taken the whole reader down with it. Nothing that long is a word
/// a hyphenation pattern has an opinion about anyway; it goes to the hard split
/// below instead.
fn hyphenatable(word: &str) -> bool {
    let n = word.chars().count();
    n >= MIN_HYPHEN_WORD
        && word.len() <= hypher::MAX_INLINE_SIZE
        && word.chars().all(|c| c.is_ascii_alphabetic())
}

/// The price of splitting a word the hyphenator could not split. High enough
/// that it is only ever taken when nothing else fits.
const HARD_SPLIT_COST: f32 = 1000.0;

/// Break apart any box too wide for the narrowest line it could land on.
///
/// A word the hyphenator has no patterns for — a URL, a chemical name, a run of
/// one letter — is otherwise a single unbreakable box, and a box wider than the
/// column has nowhere to go but off the edge of the page.
fn split_overlong(stream: Vec<Item>, min_width: f32, measure: &dyn Fn(&str) -> f32, src: &str) -> Vec<Item> {
    if !stream.iter().any(|i| matches!(i, Item::Box { width, .. } if *width > min_width)) {
        return stream;
    }
    let chars: Vec<char> = src.chars().collect();
    let mut out = Vec::with_capacity(stream.len());
    for item in stream {
        let Item::Box { width, from, to } = item else {
            out.push(item);
            continue;
        };
        if width <= min_width || to - from <= 1 {
            out.push(item);
            continue;
        }
        let text: String = chars[from.min(chars.len())..to.min(chars.len())].iter().collect();
        let mut at = from;
        let mut first = true;
        for g in text.graphemes(true) {
            let len = g.chars().count();
            if !first {
                out.push(Item::Penalty { width: 0.0, cost: HARD_SPLIT_COST, at });
            }
            out.push(Item::Box { width: measure(g), from: at, to: at + len });
            at += len;
            first = false;
        }
    }
    out
}

/// Turn a paragraph into the stream of boxes, glue and penalties the breaker
/// works on.
fn items(
    src: &str,
    measure: &dyn Fn(&str) -> f32,
    hyphens: bool,
    hyphen_width: f32,
) -> Vec<Item> {
    let mut out: Vec<Item> = Vec::new();
    let mut word = String::new();
    let mut word_start = 0usize;
    let mut seen = 0usize;
    let mut prev_wide = false;

    // Close off the latin word being gathered, splitting it at its syllables.
    macro_rules! flush_word {
        () => {
            if !word.is_empty() {
                push_word(&mut out, &word, word_start, measure, hyphens, hyphen_width);
                word.clear();
            }
        };
    }

    for g in src.graphemes(true) {
        let gc = g.chars().count();
        let space = g.chars().all(char::is_whitespace);
        let wide = is_wide(g);
        if space {
            flush_word!();
            let w = measure(g);
            out.push(Item::Glue {
                width: w,
                stretch: w * GLUE_STRETCH,
                shrink: w * GLUE_SHRINK,
                from: seen,
                to: seen + gc,
            });
            prev_wide = false;
        } else if wide {
            flush_word!();
            let w = measure(g);
            // CJK breaks between glyphs, so the joint between two of them is a
            // place a line may end. It carries no space of its own, but it can
            // open a little — which is how these scripts are justified.
            if prev_wide {
                out.push(Item::Glue {
                    width: 0.0,
                    stretch: w * CJK_STRETCH,
                    shrink: 0.0,
                    from: seen,
                    to: seen,
                });
            }
            out.push(Item::Box { width: w, from: seen, to: seen + gc });
            prev_wide = true;
        } else {
            if word.is_empty() {
                word_start = seen;
            }
            word.push_str(g);
            prev_wide = false;
        }
        seen += gc;
    }
    flush_word!();
    out
}

/// The syllables of a word, remembered.
///
/// Prose repeats itself — `the`, `and`, `library` — and hyphenating a 20 MB
/// book asks the same questions tens of thousands of times. The answers do not
/// change, so they are kept. Per thread, because the typesetter runs on one.
fn syllables_of(word: &str) -> Vec<String> {
    use std::cell::RefCell;
    use std::collections::HashMap;
    thread_local! {
        static SEEN: RefCell<HashMap<String, Vec<String>>> = RefCell::new(HashMap::new());
    }
    SEEN.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(hit) = cache.get(word) {
            return hit.clone();
        }
        let syllables: Vec<String> = hypher::hyphenate(word, hypher::Lang::English)
            .map(str::to_string)
            .collect();
        // A book's vocabulary is finite; a pathological input's is not.
        if cache.len() < 50_000 {
            cache.insert(word.to_string(), syllables.clone());
        }
        syllables
    })
}

/// Add one latin word, broken at the syllables the hyphenator finds.
fn push_word(
    out: &mut Vec<Item>,
    word: &str,
    start: usize,
    measure: &dyn Fn(&str) -> f32,
    hyphens: bool,
    hyphen_width: f32,
) {
    if !hyphens || !hyphenatable(word) {
        out.push(Item::Box {
            width: measure(word),
            from: start,
            to: start + word.chars().count(),
        });
        return;
    }
    let syllables: Vec<String> = syllables_of(word);
    if syllables.len() < 2 {
        out.push(Item::Box {
            width: measure(word),
            from: start,
            to: start + word.chars().count(),
        });
        return;
    }
    let total = word.chars().count();
    let mut at = start;
    for (i, syl) in syllables.iter().enumerate() {
        let len = syl.chars().count();
        if i > 0 {
            let before = at - start;
            let after = total - before;
            // A single letter left hanging on either side of the break reads as
            // a mistake, whatever the pattern file says.
            if before >= MIN_AFFIX && after >= MIN_AFFIX {
                out.push(Item::Penalty {
                    width: hyphen_width,
                    cost: HYPHEN_COST,
                    at,
                });
            }
        }
        out.push(Item::Box {
            width: measure(syl),
            from: at,
            to: at + len,
        });
        at += len;
    }
}

/// A place the paragraph could end a line, and the best way of getting there.
#[derive(Debug, Clone, Copy)]
struct Node {
    item: usize,
    line: usize,
    demerits: f32,
    previous: Option<usize>,
    /// The line reaching this node ends in a hyphen.
    hyphenated: bool,
    /// Totals at this point, for the ratio arithmetic below.
    width: f32,
    stretch: f32,
    shrink: f32,
}

/// Break `src` into lines of `line_width(i)` points.
///
/// Falls back to filling each line in turn only where no set of breaks is
/// settable at all — a column narrower than a single word, say. The result
/// always covers the whole paragraph.
pub fn break_lines(
    src: &str,
    line_width: &dyn Fn(usize) -> f32,
    measure: &dyn Fn(&str) -> f32,
    hyphens: bool,
) -> Vec<Line> {
    let hyphen_width = measure("-");
    let stream = items(src, measure, hyphens, hyphen_width);
    if stream.is_empty() {
        return vec![Line { text: String::new(), offset: 0, hyphen: false }];
    }
    // The narrowest column any line could land in, so nothing is left in a box
    // it cannot fit through. Ten lines is far past any drop cap's well.
    //
    // The hyphen comes off that width. A syllable joint is a place a line may
    // end only *with* a dash, so where the dash does not fit the joint is not a
    // breakpoint at all — and a word whose every joint is unusable is one
    // unbreakable run again. Splitting a little earlier keeps a way through.
    let narrowest = (0..10).map(line_width).fold(f32::INFINITY, f32::min).max(1.0);
    let reserve = if hyphens { hyphen_width } else { 0.0 };
    let min_width = (narrowest - reserve).max(narrowest * 0.5).max(1.0);
    let stream = split_overlong(stream, min_width, measure, src);

    // Loose first, then looser. TeX does the same thing for the same reason:
    // a tight tolerance sets the page beautifully when it can, and refusing to
    // set the page at all is not an improvement on setting it loosely.
    for tolerance in [MAX_RATIO, 10.0, 1000.0] {
        if let Some(breaks) = optimal_breaks(&stream, line_width, tolerance) {
            return assemble(src, &stream, &breaks);
        }
    }
    FELL_BACK.with(|n| n.set(n.get() + 1));
    greedy(src, &stream, line_width)
}

thread_local! {
    static FELL_BACK: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
/// How many paragraphs have gone to the fallback on this thread.
pub fn fallbacks() -> usize {
    FELL_BACK.with(|n| n.get())
}

/// Running totals up to each item, so any span's natural width is one
/// subtraction rather than a walk.
struct Totals {
    width: Vec<f32>,
    stretch: Vec<f32>,
    shrink: Vec<f32>,
}

impl Totals {
    fn of(stream: &[Item]) -> Totals {
        let mut t = Totals {
            width: Vec::with_capacity(stream.len() + 1),
            stretch: Vec::with_capacity(stream.len() + 1),
            shrink: Vec::with_capacity(stream.len() + 1),
        };
        let (mut w, mut st, mut sh) = (0.0f32, 0.0f32, 0.0f32);
        t.width.push(0.0);
        t.stretch.push(0.0);
        t.shrink.push(0.0);
        for it in stream {
            w += it.width();
            st += it.stretch();
            sh += it.shrink();
            t.width.push(w);
            t.stretch.push(st);
            t.shrink.push(sh);
        }
        t
    }
}

/// May a line end just before item `i`?
fn is_breakpoint(stream: &[Item], i: usize) -> bool {
    match stream.get(i) {
        // Glue is a breakpoint only when something precedes it, or the line
        // would open with a space.
        Some(Item::Glue { .. }) => i > 0 && matches!(stream[i - 1], Item::Box { .. }),
        Some(Item::Penalty { .. }) => true,
        _ => false,
    }
}

/// The set of breaks with the lowest total cost, or `None` when the paragraph
/// cannot be set at these widths at all.
fn optimal_breaks(
    stream: &[Item],
    line_width: &dyn Fn(usize) -> f32,
    tolerance: f32,
) -> Option<Vec<usize>> {
    let t = Totals::of(stream);
    let n = stream.len();
    // Every node ever made; `active` indexes into it.
    let mut nodes: Vec<Node> = vec![Node {
        item: 0,
        line: 0,
        demerits: 0.0,
        previous: None,
        hyphenated: false,
        width: 0.0,
        stretch: 0.0,
        shrink: 0.0,
    }];
    let mut active: Vec<usize> = vec![0];
    let mut best_end: Option<usize> = None;

    for i in 0..=n {
        let end_of_paragraph = i == n;
        if !end_of_paragraph && !is_breakpoint(stream, i) {
            continue;
        }
        // The hyphen, if this break is taken at a syllable joint.
        let extra = match stream.get(i) {
            Some(Item::Penalty { width, .. }) => *width,
            _ => 0.0,
        };
        let penalty = match stream.get(i) {
            Some(Item::Penalty { cost, .. }) => *cost,
            _ => 0.0,
        };
        let flagged = matches!(stream.get(i), Some(Item::Penalty { .. }));

        let mut best: Option<(f32, usize, f32)> = None; // (demerits, node, _)
        let mut still_active: Vec<usize> = Vec::with_capacity(active.len());

        for &a in &active {
            let node = nodes[a];
            let natural = t.width[i] - node.width + extra;
            let stretch = t.stretch[i] - node.stretch;
            let shrink = t.shrink[i] - node.shrink;
            let target = line_width(node.line);
            let slack = target - natural;

            let ratio = if slack > 0.0 {
                if stretch > 0.0 { slack / stretch } else { f32::INFINITY }
            } else if slack < 0.0 {
                if shrink > 0.0 { slack / shrink } else { f32::NEG_INFINITY }
            } else {
                0.0
            };

            // Too tight to set at all: this node can never reach any later
            // break either, so it leaves the list.
            if ratio < -1.0 {
                continue;
            }
            still_active.push(a);

            // Too loose to offer — unless this is the last line, which is
            // allowed to be as short as it likes.
            if ratio > tolerance && !end_of_paragraph {
                continue;
            }
            let badness = if end_of_paragraph && ratio > 0.0 {
                0.0 // the last line simply ends; it is not "loose"
            } else {
                100.0 * ratio.abs().powi(3)
            };
            let mut d = (1.0 + badness + penalty).powi(2);
            if flagged && node.hyphenated {
                d += DOUBLE_HYPHEN_DEMERITS;
            }
            let total = node.demerits + d;
            if best.is_none_or(|(bd, _, _)| total < bd) {
                best = Some((total, a, ratio));
            }
        }

        active = still_active;
        if let Some((demerits, from, _)) = best {
            let node = Node {
                item: i,
                line: nodes[from].line + 1,
                demerits,
                previous: Some(from),
                hyphenated: flagged,
                // The hyphen belongs to the line that *ends* here, and it has
                // already been counted into that line's width. Carrying it into
                // the node made it a credit against the next line, which then
                // came out one hyphen too wide — and, being the line after a
                // hyphenated one, it was the hardest place to notice.
                width: t.width[i],
                // The glue a line breaks at is not set, so it does not count
                // toward the next line either.
                stretch: t.stretch[i],
                shrink: t.shrink[i],
            };
            nodes.push(node);
            let idx = nodes.len() - 1;
            if end_of_paragraph {
                best_end = Some(idx);
            } else {
                active.push(idx);
            }
        }
        if active.is_empty() && !end_of_paragraph {
            return None; // nothing can reach the end from here
        }
    }

    let mut at = best_end?;
    let mut breaks = Vec::new();
    while let Some(prev) = nodes[at].previous {
        breaks.push(nodes[at].item);
        at = prev;
    }
    breaks.reverse();
    Some(breaks)
}

/// Turn the chosen break points back into lines of text.
fn assemble(src: &str, stream: &[Item], breaks: &[usize]) -> Vec<Line> {
    let chars: Vec<char> = src.chars().collect();
    let mut lines = Vec::new();
    let mut start_item = 0usize;
    for &b in breaks {
        lines.push(one_line(&chars, stream, start_item, b));
        // The break itself is consumed: a glue disappears, a penalty becomes
        // the hyphen the painter draws.
        start_item = match stream.get(b) {
            Some(Item::Glue { .. }) => b + 1,
            _ => b,
        };
    }
    keep_real_lines(lines)
}

/// Drop the lines that hold nothing.
///
/// A break can fall so that the span between it and the next holds only
/// penalties — no box, no glue, no text. [`one_line`] has nowhere to take an
/// offset from in that case and answers zero, so an empty line at the end of a
/// paragraph reported itself as starting at the beginning of it, and the
/// offsets ran backwards. They are not lines; they are gaps between breaks.
fn keep_real_lines(mut lines: Vec<Line>) -> Vec<Line> {
    lines.retain(|l| !l.text.is_empty());
    if lines.is_empty() {
        lines.push(Line { text: String::new(), offset: 0, hyphen: false });
    }
    lines
}

/// The text of items `from..to`, and where it sits in the paragraph.
fn one_line(chars: &[char], stream: &[Item], from: usize, to: usize) -> Line {
    let mut first = None;
    let mut last = None;
    for it in &stream[from..to.min(stream.len())] {
        let (a, b) = match it {
            Item::Box { from, to, .. } => (*from, *to),
            Item::Glue { from, to, .. } => (*from, *to),
            Item::Penalty { .. } => continue,
        };
        if b > a {
            first.get_or_insert(a);
            last = Some(b);
        }
    }
    let (mut a, b) = match (first, last) {
        (Some(a), Some(b)) => (a, b),
        _ => (0, 0),
    };
    // A line never carries the space it broke at — at either end. Leading
    // space has to move the offset with it, or the line would claim to start
    // one character before it does.
    while a < b && chars.get(a).is_some_and(|c| c.is_whitespace()) {
        a += 1;
    }
    let text: String = chars[a.min(chars.len())..b.min(chars.len())].iter().collect();
    let trimmed = text.trim_end();
    Line {
        text: trimmed.to_string(),
        offset: a,
        // Only a penalty that carries a hyphen draws one. The joints inside a
        // word the hyphenator could not split are breakpoints of width nothing,
        // and marking those hyphenated put a stray dash mid-URL — and pushed
        // the line a glyph past the margin.
        hyphen: matches!(stream.get(to), Some(Item::Penalty { width, .. }) if *width > 0.0),
    }
}

/// Fill each line in turn. Only reached when no set of breaks is settable —
/// a column narrower than a single word — and it still covers the paragraph.
fn greedy(src: &str, stream: &[Item], line_width: &dyn Fn(usize) -> f32) -> Vec<Line> {
    let chars: Vec<char> = src.chars().collect();
    let totals = Totals::of(stream);
    let mut lines = Vec::new();
    let mut start = 0usize;
    // The last place this line could have ended and still fitted.
    let mut last_fit: Option<usize> = None;
    let mut i = start;

    // Every line consumes at least one item, so there can never be more lines
    // than items. Reaching this means the loop stopped making progress, and it
    // is far better to come back with an answer a test can see is wrong than to
    // keep appending empty lines until the allocator gives up — which is what
    // the first version of this did, at eighty gigabytes.
    let ceiling = stream.len() + 2;
    while i <= stream.len() {
        if lines.len() > ceiling {
            // Loud where anyone is looking, safe where they are not. A test
            // build stops here and says so; a reader's build comes back with a
            // short answer rather than eating the machine.
            debug_assert!(false, "the line breaker stopped making progress");
            break;
        }
        let end = i == stream.len();
        // `i == start` is skipped, and that is not a tidiness: a break taken at
        // a penalty leaves `start` sitting on a breakpoint, which was then
        // recorded as "the last place that fitted". The next overlong line
        // broke there — producing a line of no length and moving `start`
        // nowhere — and the loop pushed empty lines until the allocator gave
        // up. The gate met it as an 80 GB allocation.
        if !end && (i == start || !is_breakpoint(stream, i)) {
            i += 1;
            continue;
        }
        let extra = match stream.get(i) {
            Some(Item::Penalty { width, .. }) => *width,
            _ => 0.0,
        };
        let natural = totals.width[i] - totals.width[start] + extra;
        let adds_a_hyphen = extra > 0.0;
        if natural <= line_width(lines.len()) {
            last_fit = Some(i);
            if end {
                break;
            }
            i += 1;
            continue;
        }
        // Past the measure. A break that adds a hyphen is never the answer
        // here — the dash only makes the overhang worse — so it is passed over
        // entirely rather than merely not remembered. Remembering it was not
        // enough: with nothing else recorded, the break below fell through to
        // this very candidate and set the hyphen anyway.
        if adds_a_hyphen && last_fit.is_none() {
            if end {
                break;
            }
            i += 1;
            continue;
        }
        if last_fit.is_none() {
            // Nothing fits at all: this is the least bad place there is, and
            // the line simply overhangs.
            last_fit = Some(i);
            if end {
                break;
            }
            i += 1;
            continue;
        }
        // Whatever happens, the line has to hold something and `start` has to
        // move forward. Every other arrangement of this loop is a way of not
        // terminating.
        let b = last_fit.take().filter(|&b| b > start).unwrap_or(i.max(start + 1));
        lines.push(one_line(&chars, stream, start, b));
        let next = match stream.get(b) {
            Some(Item::Glue { .. }) => b + 1,
            _ => b,
        };
        debug_assert!(next > start, "the greedy fallback made no progress");
        start = next.max(start + 1);
        i = start;
    }
    if start < stream.len() || lines.is_empty() {
        lines.push(one_line(&chars, stream, start, stream.len()));
    }
    keep_real_lines(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fake font: 10pt a narrow glyph, 20pt a wide one.
    fn fake(s: &str) -> f32 {
        s.graphemes(true)
            .map(|g| if is_wide(g) { 20.0 } else { 10.0 })
            .sum()
    }

    fn set(src: &str, width: f32, hyphens: bool) -> Vec<Line> {
        break_lines(src, &|_| width, &fake, hyphens)
    }

    /// The paragraph, read back off the lines it was broken into.
    fn rejoin(lines: &[Line]) -> String {
        lines
            .iter()
            .map(|l| l.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn no_line_is_wider_than_its_column() {
        let src = "The quick brown fox jumps over the lazy dog and keeps on running for a while";
        for w in [80.0f32, 130.0, 200.0, 340.0, 500.0] {
            for hy in [false, true] {
                for l in set(src, w, hy) {
                    let width = fake(&l.text) + if l.hyphen { fake("-") } else { 0.0 };
                    let single = l.text.split(' ').count() == 1;
                    assert!(
                        width <= w + 0.01 || single,
                        "at {w}pt (hyphens {hy}): {:?} is {width}pt",
                        l.text
                    );
                }
            }
        }
    }

    #[test]
    fn every_character_survives_the_break() {
        for src in [
            "The quick brown fox jumps over the lazy dog",
            "읽지 않은 책이 쌓인 서가를 안티라이브러리라 부른다",
            "한글 and latin mixed 함께 in one line 이렇게",
            "supercalifragilisticexpialidocious obstacle",
            "one",
            "",
        ] {
            for w in [60.0f32, 100.0, 180.0, 400.0] {
                for hy in [false, true] {
                    let lines = set(src, w, hy);
                    let got: String = rejoin(&lines).chars().filter(|c| !c.is_whitespace()).collect();
                    let want: String = src.chars().filter(|c| !c.is_whitespace()).collect();
                    assert_eq!(got, want, "{src:?} at {w}pt, hyphens {hy}");
                }
            }
        }
    }

    #[test]
    fn a_lines_text_is_the_paragraph_at_its_own_offset() {
        // The invariant everything else rests on. A hyphen is drawn, never
        // stored, so it must not appear here.
        let src = "The quick brown fox jumps over the lazy dog and runs onward";
        let chars: Vec<char> = src.chars().collect();
        for w in [70.0f32, 120.0, 250.0] {
            for hy in [false, true] {
                for l in set(src, w, hy) {
                    let end = l.offset + l.text.chars().count();
                    let says: String = chars[l.offset..end].iter().collect();
                    assert_eq!(says, l.text, "at {w}pt, hyphens {hy}");
                    assert!(!l.text.ends_with('-') || src.contains(&format!("{}-", l.text.trim_end_matches('-'))));
                }
            }
        }
    }

    /// Fill each line in turn, the way the reader used to: take words until the
    /// next one will not fit, then start again. Written out here so the claim
    /// that looking at the paragraph is different can be *shown* rather than
    /// asserted — the two agreeing would mean nothing had been gained.
    fn line_at_a_time(src: &str, width: f32) -> Vec<String> {
        let mut lines: Vec<String> = Vec::new();
        let mut cur = String::new();
        for word in src.split(' ') {
            let candidate = if cur.is_empty() {
                word.to_string()
            } else {
                format!("{cur} {word}")
            };
            if fake(&candidate) > width && !cur.is_empty() {
                lines.push(std::mem::take(&mut cur));
                cur.push_str(word);
            } else {
                cur = candidate;
            }
        }
        if !cur.is_empty() {
            lines.push(cur);
        }
        lines
    }

    #[test]
    fn looking_at_the_paragraph_beats_filling_each_line() {
        // Two claims, and neither is safe to make about one paragraph: any
        // single passage can come out the same either way, or occasionally
        // worse. Measured over several, at several measures.
        let passages = [
            "In olden times when wishing still helped one there lived a king \
whose daughters were all beautiful",
            "Read books are far less valuable than unread ones because the library \
should contain what you do not know",
            "A well set page does not hurry the eye the measure is narrow the \
leading generous and the margins wide",
        ];
        let widths = [140.0f32, 160.0, 180.0, 200.0, 220.0, 240.0, 260.0, 300.0];

        // How full the emptiest line of a paragraph is. A right edge with a
        // hole in it is exactly one line that stopped early.
        let emptiest = |ls: &[String]| ls.iter().map(|t| fake(t)).fold(f32::INFINITY, f32::min);

        let mut differed = 0usize;
        let (mut ours_total, mut theirs_total) = (0.0f32, 0.0f32);
        for src in passages {
            for w in widths {
                let ours: Vec<String> = set(src, w, false)
                    .iter()
                    .map(|l| l.text.clone())
                    .collect();
                let theirs = line_at_a_time(src, w);
                if ours != theirs {
                    differed += 1;
                }
                ours_total += emptiest(&ours);
                theirs_total += emptiest(&theirs);
                for line in &ours {
                    assert!(fake(line) <= w + 0.01, "{line:?} is wider than {w}pt");
                }
            }
        }

        // First: the two are not the same thing. Without this the comparison
        // below would pass trivially on a breaker that had quietly become a
        // line-at-a-time one again.
        assert!(
            differed > 0,
            "every paragraph came out exactly as filling each line in turn would set it"
        );
        // And second: looking at the paragraph leaves fuller lines behind.
        assert!(
            ours_total > theirs_total,
            "the emptiest lines total {ours_total:.0}pt against {theirs_total:.0}pt"
        );
    }

    #[test]
    fn hyphenation_evens_out_the_right_edge() {
        // What syllable breaks buy is not fewer lines — sometimes there are
        // more — but a right edge that stops jumping about, because a line can
        // now end part way through a word instead of throwing the whole word
        // over. Measured across several passages and measures rather than one:
        // any single paragraph can go either way, and a claim that only holds
        // for the example someone picked is not a claim.
        let passages = [
            "We call this an extraordinary demonstration of typesetting",
            "The library is a device for confronting what one does not yet know; \
the unread shelf is the useful half",
            "Read books are far less valuable than unread ones. The library should \
contain as much as your finances allow",
        ];
        // The widest space left at the end of a line that is not the last one.
        let worst_gap = |ls: &[Line], w: f32| -> f32 {
            if ls.len() < 2 {
                return 0.0;
            }
            ls[..ls.len() - 1]
                .iter()
                .map(|l| w - fake(&l.text) - if l.hyphen { fake("-") } else { 0.0 })
                .fold(0.0f32, f32::max)
        };
        let (mut ragged, mut hyphenated) = (0.0f32, 0.0f32);
        let mut hyphens_seen = 0usize;
        for src in passages {
            for w in [120.0f32, 180.0, 260.0, 340.0] {
                let a = set(src, w, false);
                let b = set(src, w, true);
                ragged += worst_gap(&a, w);
                hyphenated += worst_gap(&b, w);
                hyphens_seen += b.iter().filter(|l| l.hyphen).count();
            }
        }
        assert!(hyphens_seen > 0, "nothing was hyphenated at any measure");
        assert!(
            hyphenated < ragged,
            "hyphenation left the edge no better: {hyphenated:.0}pt of gap against {ragged:.0}pt"
        );
    }

    #[test]
    fn a_word_split_across_lines_carries_a_hyphen() {
        // The other half of the same rule: if a line stops inside a word, the
        // reader has to be told, and a dash is how. A word the hyphenator has
        // no patterns for is the exception — there is no honest place to put
        // one — and it has to be a real exception, not the common case.
        let src = "We call this an extraordinary demonstration of typesetting";
        for w in [120.0f32, 200.0, 300.0] {
            let lines = set(src, w, true);
            for pair in lines.windows(2) {
                let ends_mid_word = pair[0].offset + pair[0].text.chars().count() == pair[1].offset;
                if ends_mid_word {
                    assert!(
                        pair[0].hyphen,
                        "at {w}pt {:?} runs straight into {:?} with no hyphen",
                        pair[0].text, pair[1].text
                    );
                }
            }
        }
    }

    #[test]
    fn a_hyphen_never_leaves_a_single_letter_behind() {
        let src = "an extraordinary abandonment of every reasonable elaboration";
        for w in [60.0f32, 90.0, 130.0] {
            let lines = set(src, w, true);
            for (i, l) in lines.iter().enumerate() {
                if !l.hyphen {
                    continue;
                }
                // The piece left on this line, and the piece taken to the next.
                let head = l.text.rsplit(' ').next().unwrap_or("");
                assert!(
                    head.chars().count() >= MIN_AFFIX,
                    "at {w}pt line {i} breaks after {head:?}"
                );
                let tail = lines[i + 1].text.split(' ').next().unwrap_or("");
                assert!(
                    tail.chars().count() >= MIN_AFFIX,
                    "at {w}pt line {} opens with {tail:?}",
                    i + 1
                );
            }
        }
    }

    #[test]
    fn korean_breaks_between_glyphs_and_needs_no_hyphen() {
        let src = "읽지 않은 책이 쌓인 서가를 안티라이브러리라 부른다";
        for w in [80.0f32, 140.0, 220.0] {
            let lines = set(src, w, true);
            assert!(lines.iter().all(|l| !l.hyphen), "Korean was hyphenated");
            for l in &lines {
                assert!(fake(&l.text) <= w + 0.01, "{:?} at {w}pt", l.text);
            }
        }
    }

    #[test]
    fn a_paragraph_of_unbreakable_words_terminates_and_keeps_its_text() {
        // The greedy fallback is reached when nothing can be set within
        // tolerance, and it used to be able to break at the point it had just
        // started from: no progress, an empty line, and around again. This is
        // the shape that got it there — long words, a narrow column, and
        // hyphenation offering breaks that do not fit.
        for src in [
            "extraordinary demonstrations extraordinarily demonstrated",
            "antidisestablishmentarianism antidisestablishmentarianism",
            "https://example.com/very/long/path https://example.com/other",
        ] {
            // Including columns narrower than a single glyph. That is the
            // only shape that reaches the degenerate case: a break taken at a
            // split point, and the very next character too wide to follow it.
            // Every wider column hides it, which is why the first version of
            // this test — 20pt and up — passed against the defect.
            for w in [3.0f32, 5.0, 8.0, 9.0, 20.0, 30.0, 45.0, 60.0] {
                for hy in [false, true] {
                    let lines = set(src, w, hy);
                    assert!(
                        lines.len() <= src.chars().count() + 2,
                        "{src:?} at {w}pt made {} lines out of {} characters",
                        lines.len(),
                        src.chars().count()
                    );
                    assert!(
                        lines.iter().all(|l| !l.text.is_empty()),
                        "{src:?} at {w}pt produced a line with nothing on it"
                    );
                    let got: String = lines
                        .iter()
                        .flat_map(|l| l.text.chars())
                        .filter(|c| !c.is_whitespace())
                        .collect();
                    let want: String = src.chars().filter(|c| !c.is_whitespace()).collect();
                    assert_eq!(got, want, "{src:?} at {w}pt, hyphens {hy}");
                }
            }
        }
    }

    #[test]
    fn a_word_too_long_for_the_hyphenator_does_not_take_the_reader_down() {
        // `hypher` keeps the word in a fixed buffer and panics past its size.
        // One long URL in a document is not a rare thing.
        for n in [40usize, 44, 45, 46, 60, 200, 900] {
            let word = "a".repeat(n);
            let src = format!("before {word} after");
            for w in [50.0f32, 200.0, 1000.0] {
                let lines = set(&src, w, true);
                let got: String = lines
                    .iter()
                    .flat_map(|l| l.text.chars())
                    .filter(|c| !c.is_whitespace())
                    .collect();
                assert_eq!(got, src.replace(' ', ""), "a {n} character word at {w}pt");
            }
        }
    }

    #[test]
    fn a_hyphenated_line_pays_for_its_own_dash() {
        // The hyphen belongs to the line that ends with it. Counting it against
        // the *next* line made that one a dash too wide — and being the line
        // after a hyphenated one, it was the hardest to notice.
        let src = "A second paragraph follows here and demonstrates the point";
        for w in [70.0f32, 90.0, 120.0, 160.0, 210.0] {
            let lines = set(src, w, true);
            for (i, l) in lines.iter().enumerate() {
                let width = fake(&l.text) + if l.hyphen { fake("-") } else { 0.0 };
                let single = l.text.graphemes(true).count() <= 1;
                assert!(
                    width <= w + 0.01 || single,
                    "at {w}pt line {i} {:?}{} is {width}pt",
                    l.text,
                    if l.hyphen { " plus a dash" } else { "" }
                );
            }
        }
    }

    #[test]
    fn a_column_narrower_than_one_word_still_produces_lines() {
        let lines = set("antidisestablishmentarianism", 30.0, false);
        assert!(!lines.is_empty());
        let joined: String = lines.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(joined, "antidisestablishmentarianism");
    }

    #[test]
    fn the_column_width_may_change_from_line_to_line() {
        // The well a drop cap opens: the first rows are short, the rest full.
        let src = "The quick brown fox jumps over the lazy dog and keeps running onward";
        let lines = break_lines(src, &|i| if i < 3 { 90.0 } else { 200.0 }, &fake, false);
        for (i, l) in lines.iter().enumerate() {
            let limit = if i < 3 { 90.0 } else { 200.0 };
            let single = l.text.split(' ').count() == 1;
            assert!(
                fake(&l.text) <= limit + 0.01 || single,
                "line {i} is {:?} in a {limit}pt column",
                l.text
            );
        }
    }

    #[test]
    fn the_fallback_is_reached_often_enough_to_be_worth_testing() {
        // Measured, not assumed. Half of these go through it, so it is not a
        // branch anyone can delete as unreachable — and it is why it carries
        // its own tests above rather than being treated as a formality.
        let before = fallbacks();
        for src in [
            &"a".repeat(500),
            "antidisestablishmentarianism antidisestablishmentarianism",
            "https://example.com/very/long/path https://example.com/other",
        ] {
            for w in [5.0f32, 20.0, 45.0] {
                let _ = set(src, w, true);
            }
        }
        assert!(
            fallbacks() > before,
            "no paragraph reached the fallback, so its tests prove nothing"
        );
    }

    #[test]
    fn an_empty_paragraph_yields_one_empty_line() {
        let lines = set("", 100.0, true);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "");
    }
}

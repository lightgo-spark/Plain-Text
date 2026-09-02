//! Benchmark harness: how long the reader takes to open, set and search a
//! book, measured on the same machine that runs the app.
//!
//! Usage:
//!     bench                 # run every stage on the built-in corpus sizes
//!     bench --corpus        # write the corpus files and stop
//!     bench --file PATH     # measure one real file
//!     bench --dump PATH     # print the paragraphs the importer produced
//!
//! Timings are wall clock, best of N runs after one warm-up, printed as a
//! table and as JSON for the report.

use anti_library::gui::layout::{self, Metrics, Setup};
use anti_library::text::Document;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

const RUNS: usize = 5;

/// Stand-in for a real font: 10pt per narrow glyph, 20pt per wide one. The
/// point is to measure the typesetter, not the font engine, and to keep the
/// numbers reproducible on machines with different fonts installed.
fn fake_measure(s: &str) -> f32 {
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

fn best_of(runs: usize, mut f: impl FnMut()) -> Duration {
    f(); // warm up: first touch pays for page faults and allocator growth
    let mut best = Duration::MAX;
    for _ in 0..runs {
        let t = Instant::now();
        f();
        best = best.min(t.elapsed());
    }
    best
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

/// Build a mixed Korean/English corpus of roughly `target` bytes.
fn corpus(target: usize) -> String {
    const KO: &[&str] = &[
        "읽지 않은 책이 쌓인 서가를 안티라이브러리라 부른다. 이미 읽은 책보다 아직 읽지 않은 책이 더 많은 것을 아는 일, 그것이 서재의 쓸모다.",
        "서재는 자랑이 아니라 도구다. 읽은 책은 지나간 것이고, 읽지 않은 책은 아직 남아 있는 질문이다.",
        "책장을 넘기는 손은 느리고, 문장은 그보다 느리게 스며든다. 조판이 좋은 책은 읽는 속도를 재촉하지 않는다.",
    ];
    const EN: &[&str] = &[
        "The library is a device for confronting what one does not yet know; the unread shelf is the useful half, and the read half is merely a record of where the reader has already been.",
        "Read books are far less valuable than unread ones. The library should contain as much of what you do not know as your finances allow.",
        "A well set page does not hurry the eye. The measure is narrow, the leading generous, and the margins are wide enough to rest a thumb.",
    ];
    let mut out = String::with_capacity(target + 4096);
    let mut i = 0usize;
    while out.len() < target {
        if i.is_multiple_of(40) {
            out.push_str(&format!("\nChapter {}\n\n", i / 40 + 1));
        }
        if i.is_multiple_of(37) {
            out.push_str(&format!("제 {} 장  긴 문서\n\n", i / 37 + 1));
        }
        out.push_str(KO[i % KO.len()]);
        out.push(' ');
        out.push_str(EN[i % EN.len()]);
        out.push_str("\n\n");
        i += 1;
    }
    out
}

struct Row {
    label: String,
    bytes: usize,
    chars: usize,
    load: Duration,
    typeset: Duration,
    search: Duration,
    page_jump: Duration,
    pages: usize,
    rows: usize,
}

fn measure_file(label: &str, path: &Path) -> Row {
    let bytes = std::fs::metadata(path).map(|m| m.len() as usize).unwrap_or(0);
    let doc = match Document::load(path) {
        Ok(doc) => doc,
        Err(e) => {
            eprintln!("cannot read {}: {e}", path.display());
            std::process::exit(1);
        }
    };
    let load = best_of(RUNS, || {
        let _ = Document::load(path);
    });

    let setup = Setup {
        width: 620.0,
        height: 780.0,
        metrics: Metrics::default(),
        justify: true,
        drop_caps: true,
        chapter_breaks: true,
        hyphenate: true,
    };
    let typeset = best_of(RUNS, || {
        let _ = layout::typeset(&doc, &setup, &fake_measure, &fake_measure);
    });
    let l = layout::typeset(&doc, &setup, &fake_measure, &fake_measure);

    // Searching scans the document, the way the reader's find does. It used to
    // scan the wrapped rows here, which measured the wrong thing as well as
    // answering the wrong question: the reader types into the search box a
    // character at a time, so this is the cost of one keystroke.
    let needle = "안티라이브러리";
    let search = best_of(RUNS, || {
        std::hint::black_box(doc.search(needle).len());
    });

    // Jumping to a percentage: offset -> row -> page, 1000 times.
    let page_jump = best_of(RUNS, || {
        for i in 0..1000 {
            let off = doc.chars * i / 1000;
            std::hint::black_box(l.page_of_offset(off));
        }
    });

    Row {
        label: label.to_string(),
        bytes,
        chars: doc.chars,
        load,
        typeset,
        search,
        page_jump,
        pages: l.pages.len(),
        rows: l.rows.len(),
    }
}

/// The value that belongs to `flag`, or a usage line and a clean exit.
///
/// A missing value used to run off the end of the argument list, so the reader
/// got a panic and a stack trace where they wanted to be told what to type.
fn arg_after<'a>(args: &'a [String], at: usize, flag: &str) -> &'a str {
    match args.get(at + 1) {
        Some(v) if !v.starts_with("--") => v,
        _ => {
            eprintln!("{flag} needs a file: antilib-bench {flag} <path>");
            std::process::exit(2);
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = std::env::temp_dir().join("antilib-bench");
    std::fs::create_dir_all(&dir).expect("corpus dir");

    if let Some(i) = args.iter().position(|a| a == "--dump") {
        // Print what the importer produced, for checking a converter by eye.
        let path = PathBuf::from(arg_after(&args, i, "--dump"));
        let doc = match Document::load(&path) {
            Ok(doc) => doc,
            Err(e) => {
                eprintln!("cannot read {}: {e}", path.display());
                std::process::exit(1);
            }
        };
        eprintln!("format: {} | chars: {}", doc.format.name(), doc.chars);
        for p in doc.paragraphs.iter().take(40) {
            println!("[{:>6}] {:?}", p.offset, p.text);
        }
        return;
    }

    if let Some(i) = args.iter().position(|a| a == "--file") {
        let path = PathBuf::from(arg_after(&args, i, "--file"));
        let label = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.display().to_string());
        let row = measure_file(&label, &path);
        print_table(&[row]);
        return;
    }

    let sizes: &[(&str, usize)] = &[
        ("64 KB", 64 * 1024),
        ("1 MB", 1024 * 1024),
        ("5 MB", 5 * 1024 * 1024),
        ("20 MB", 20 * 1024 * 1024),
    ];
    let mut paths = Vec::new();
    for (label, size) in sizes {
        let path = dir.join(format!("corpus-{}.txt", label.replace(' ', "")));
        if std::fs::metadata(&path).map(|m| m.len() as usize).unwrap_or(0) < *size {
            std::fs::write(&path, corpus(*size)).expect("write corpus");
        }
        paths.push((label.to_string(), path));
    }
    if args.iter().any(|a| a == "--corpus") {
        println!("corpus written to {}", dir.display());
        return;
    }

    let rows: Vec<Row> = paths
        .iter()
        .map(|(label, path)| measure_file(label, path))
        .collect();
    print_table(&rows);
}

fn print_table(rows: &[Row]) {
    println!(
        "{:<8} {:>10} {:>11} {:>10} {:>11} {:>10} {:>11} {:>8} {:>9}",
        "size", "bytes", "chars", "load ms", "typeset ms", "find ms", "1k jumps", "pages", "rows"
    );
    for r in rows {
        println!(
            "{:<8} {:>10} {:>11} {:>10.1} {:>11.1} {:>10.2} {:>11.2} {:>8} {:>9}",
            r.label,
            r.bytes,
            r.chars,
            ms(r.load),
            ms(r.typeset),
            ms(r.search),
            ms(r.page_jump),
            r.pages,
            r.rows
        );
    }
    println!("\n[");
    for (i, r) in rows.iter().enumerate() {
        println!(
            "  {{\"size\":\"{}\",\"bytes\":{},\"chars\":{},\"load_ms\":{:.2},\"typeset_ms\":{:.2},\"find_ms\":{:.3},\"jump1000_ms\":{:.3},\"pages\":{},\"rows\":{}}}{}",
            r.label,
            r.bytes,
            r.chars,
            ms(r.load),
            ms(r.typeset),
            ms(r.search),
            ms(r.page_jump),
            r.pages,
            r.rows,
            if i + 1 == rows.len() { "" } else { "," }
        );
    }
    println!("]");
}

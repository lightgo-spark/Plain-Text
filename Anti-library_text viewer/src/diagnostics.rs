//! What to attach to a bug report.
//!
//! Somebody whose reader misbehaved has one useful thing to send and no way to
//! know what it is. This writes it: the version, the machine, what the shelf
//! holds, and the crash record — which is the part that actually says what
//! went wrong.
//!
//! What it deliberately leaves out is every file name and every path. A
//! diagnostics file is written to be sent to a stranger, and which books
//! somebody reads is their business. Counts carry the fault; titles do not.

use crate::library::Library;
use std::fmt::Write as _;

/// The report, as plain text the reader can look over before sending it.
pub fn report(lib: &Library) -> String {
    let books = lib.books.len();
    let bookmarks: usize = lib.books.values().map(|r| r.bookmarks.len()).sum();
    let marks = || lib.books.values().flat_map(|r| r.highlights.iter());
    let highlights = marks().count();
    let notes = marks().filter(|h| !h.note.is_empty()).count();
    let stale = marks().filter(|h| h.stale).count();

    let mut out = String::new();
    let _ = writeln!(out, "anti-library diagnostics");
    let _ = writeln!(out, "------------------------");
    let _ = writeln!(out, "version      {}", env!("CARGO_PKG_VERSION"));
    let _ = writeln!(out, "built for    {}", std::env::consts::OS);
    let _ = writeln!(out, "shelf        {books} book(s)");
    let _ = writeln!(
        out,
        "marks        {bookmarks} bookmark(s), {highlights} highlight(s)"
    );
    let _ = writeln!(
        out,
        "             {notes} with a note, {stale} no longer matching their file"
    );
    if let Some(kept) = lib.damaged_store() {
        // The name, not the path: it says which run set a store aside without
        // saying where on this machine anything lives.
        let _ = writeln!(
            out,
            "damaged      a store was set aside as {}",
            kept.file_name().unwrap_or_default().to_string_lossy()
        );
    }
    if lib.is_sealed() {
        let _ = writeln!(
            out,
            "sealed       the store could not be read, so nothing is being saved"
        );
    }
    let _ = writeln!(out);
    let _ = writeln!(out, "last crash");
    let _ = writeln!(out, "----------");
    match crate::crash::last() {
        Some(line) => {
            let _ = writeln!(out, "{line}");
        }
        None => {
            let _ = writeln!(out, "none recorded");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::{Bookmark, Highlight, Ink};

    /// A report is for sending on, so it must not carry what the reader is
    /// reading. This is the whole point of the module and the easiest thing to
    /// lose: one `{key}` added to a line and every title goes out with it.
    #[test]
    fn the_report_names_no_book_and_no_path() {
        let mut lib = Library::default();
        let secret_path = r"C:\Users\someone\Documents\a private diary.txt";
        let rec = lib.record(secret_path);
        rec.title = "A Private Diary".into();
        rec.bookmarks.push(Bookmark {
            offset: 1,
            label: "the part about my brother".into(),
        });
        rec.highlights.push(Highlight {
            start: 0,
            end: 4,
            ink: Ink::Rose,
            text: "something I would not want quoted".into(),
            note: "nor this".into(),
            stale: false,
        });

        let out = report(&lib);
        for leak in [
            secret_path,
            "A Private Diary",
            "the part about my brother",
            "something I would not want quoted",
            "nor this",
            "diary",
        ] {
            assert!(
                !out.contains(leak),
                "the report gives away {leak:?}:\n{out}"
            );
        }
        // It still has to be worth sending.
        assert!(out.contains("1 book(s)"), "{out}");
        assert!(out.contains("1 bookmark(s), 1 highlight(s)"), "{out}");
        assert!(out.contains(env!("CARGO_PKG_VERSION")), "{out}");
    }

    #[test]
    fn an_empty_shelf_still_produces_something_sendable() {
        let out = report(&Library::default());
        assert!(out.contains("0 book(s)"), "{out}");
        assert!(out.contains("last crash"), "{out}");
    }
}

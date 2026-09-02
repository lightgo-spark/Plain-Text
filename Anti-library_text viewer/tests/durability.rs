//! What happens to the reader's own work when things go wrong.
//!
//! Bookmarks, highlights and notes are the only thing in this program the
//! reader made themselves; a book can be opened again, a note cannot. These
//! ask what becomes of that work when the store is damaged, when two readers
//! are open at once, and when a save is cut off partway.

use anti_library::library::{Bookmark, Highlight, Ink, Library};
use std::path::PathBuf;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("antilib-durability-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("library.json")
}

fn a_marked_book(store: &std::path::Path, note: &str) {
    let mut lib = Library::load_from(store.to_path_buf());
    let rec = lib.record("book.txt");
    rec.offset = 4200;
    rec.title = "Book".into();
    rec.bookmarks.push(Bookmark {
        offset: 10,
        label: "chapter two".into(),
    });
    rec.highlights.push(Highlight {
        start: 0,
        end: 4,
        ink: Ink::Yellow,
        text: "text".into(),
        note: note.into(),
        stale: false,
    });
    lib.save().unwrap();
}

/// A store that cannot be parsed must not be written over.
///
/// The reader loads an unreadable library as an empty one — which is right, a
/// broken file must not stop someone reading. What follows is not: the empty
/// library keeps the path, and the next save writes it back over the file. A
/// year of highlights is then gone, and nothing said a word.
#[test]
fn a_damaged_store_is_not_written_over_by_an_empty_one() {
    let store = scratch("damaged");
    a_marked_book(&store, "why this matters");
    let good = std::fs::read_to_string(&store).unwrap();

    // Half a file: what a full disk or a power cut leaves behind.
    let cut = &good[..good.len() / 2];
    std::fs::write(&store, cut).unwrap();

    let mut lib = Library::load_from(store.clone());
    assert!(lib.books.is_empty(), "a damaged store still reads as empty");

    // The reader turns a page, which is all it takes.
    lib.record("other.txt").offset = 1;
    lib.save().unwrap();

    let salvaged = std::fs::read_dir(store.parent().unwrap())
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect::<Vec<_>>();
    assert!(
        salvaged.iter().any(|n| n.contains("damaged") || n.contains("bak")),
        "the damaged store was thrown away instead of being kept: {salvaged:?}"
    );
}

/// Two readers, both open, both saving. Neither may erase the other's work.
///
/// The README says the terminal and desktop readers carry on from each other,
/// and they share one file to do it. Each holds the whole library in memory
/// from the moment it started and writes all of it back, so whichever saves
/// last silently erases everything the other did after it loaded.
#[test]
fn two_readers_open_at_once_do_not_erase_each_others_marks() {
    let store = scratch("shared");
    a_marked_book(&store, "first");

    // The desktop reader starts and holds the library it found.
    let mut desktop = Library::load_from(store.clone());

    // The terminal reader marks a passage and saves.
    {
        let mut terminal = Library::load_from(store.clone());
        terminal.record("book.txt").highlights.push(Highlight {
            start: 100,
            end: 120,
            ink: Ink::Mint,
            text: "marked in the terminal".into(),
            note: "from the terminal".into(),
            stale: false,
        });
        terminal.save().unwrap();
    }

    // The desktop reader turns a page and saves, hours later.
    desktop.record("book.txt").offset = 9000;
    desktop.save().unwrap();

    let after = Library::load_from(store.clone());
    let marks = &after.get("book.txt").unwrap().highlights;
    assert!(
        marks.iter().any(|h| h.note == "from the terminal"),
        "the terminal reader's highlight was erased by the desktop reader's save"
    );
}

/// A save that reaches the disk must be a save that survives losing power.
///
/// `write` then `rename` puts the new name in place atomically, but the bytes
/// under it are still in the cache: the rename can be on disk when the content
/// is not, and the reader comes back to a library.json of zeroes. The library's
/// own test claims "a reader whose machine dies mid-save keeps their bookmarks"
/// — that claim needs the data flushed before the name is swapped.
#[test]
fn a_saved_library_is_on_the_disk_before_the_name_is_swapped() {
    let read = |f: &str| {
        std::fs::read_to_string(format!("{}/{f}", env!("CARGO_MANIFEST_DIR"))).unwrap()
    };
    let library = read("src/library.rs");

    // The flush lives in the one function both stores write through.
    let atomic = library
        .split_once("fn write_atomic(")
        .expect("the atomic write is still called write_atomic")
        .1;
    let body = &atomic[..atomic.find("\n}").unwrap_or(atomic.len())];
    assert!(
        body.contains("sync_all") || body.contains("sync_data"),
        "the atomic write renames a file it never flushed:\n{body}"
    );
    assert!(
        body.contains("rename"),
        "the atomic write no longer swaps the name in:\n{body}"
    );

    // And every store goes through it. A second writer that calls
    // `fs::write` directly puts the hole straight back.
    for (file, what) in [
        ("src/library.rs", "the library"),
        ("src/gui/settings.rs", "the settings"),
    ] {
        let source = read(file);
        // The product, not its tests — a test may write whatever file it likes.
        let product = source
            .split_once("#[cfg(test)]")
            .map(|(before, _)| before.to_string())
            .unwrap_or(source);
        assert!(
            product.contains("write_atomic"),
            "{what} is written without the atomic write ({file})"
        );
        assert!(
            !product.contains("fs::write("),
            "{what} still has a plain fs::write, which is the unflushed path ({file})"
        );
    }
}

/// Opening the library must not wait on hardware that is not there.
///
/// Every record is refiled at load, and refiling resolves the path against the
/// filesystem. A book left on a share that is no longer mounted costs a
/// timeout each, before the window is drawn — and nothing in the reader can
/// take a book off the shelf, so they only ever accumulate.
#[test]
fn a_library_of_unreachable_books_still_opens_promptly() {
    let store = scratch("unreachable");
    let mut lib = Library::load_from(store.clone());
    for i in 0..20 {
        let key = format!(r"\\bookshelf-nas\books\volume{i}\book.txt");
        let rec = lib.record(&key);
        rec.title = format!("Book {i}");
        rec.last_opened = 1_700_000_000 + i as u64;
    }
    lib.save().unwrap();

    let at = std::time::Instant::now();
    let back = Library::load_from(store.clone());
    let took = at.elapsed();
    assert_eq!(back.books.len(), 20);
    assert!(
        took.as_millis() < 500,
        "opening a library of 20 unreachable books took {took:?} — that is the \
         wait before the window appears, and nothing can take a book off the shelf"
    );
}

/// Merging must not undo an erase.
///
/// The fix for two open readers is to fold the file on disk back in before
/// writing. Done as a union, that quietly resurrects every highlight the
/// reader has just deleted — the copy on disk still has it, so it comes
/// straight back, and deleting anything becomes impossible while a second
/// reader is open. The merge is three-way for this reason.
#[test]
fn a_deleted_highlight_does_not_come_back_from_the_file_on_disk() {
    let store = scratch("erase");
    a_marked_book(&store, "the passage");

    let mut reader = Library::load_from(store.clone());
    let rec = reader.record("book.txt");
    assert_eq!(rec.highlights.len(), 1);
    rec.highlights.clear();
    let bookmarks_before = rec.bookmarks.len();
    reader.save().unwrap();

    let after = Library::load_from(store.clone());
    let rec = after.get("book.txt").unwrap();
    assert!(
        rec.highlights.is_empty(),
        "the erased highlight came back out of the file on disk: {:?}",
        rec.highlights
    );
    assert_eq!(
        rec.bookmarks.len(),
        bookmarks_before,
        "the merge lost a bookmark nobody touched"
    );
}

/// The other reader's *new* work still arrives, in the same save.
///
/// The pair of this one: three-way merging must keep a mark the other reader
/// added since — otherwise the fix for the erase has simply put the data loss
/// back the other way round.
#[test]
fn an_erase_here_and_an_addition_there_both_survive_one_save() {
    let store = scratch("both");
    a_marked_book(&store, "the passage");

    let mut desktop = Library::load_from(store.clone());
    desktop.record("book.txt").highlights.clear();

    {
        let mut terminal = Library::load_from(store.clone());
        terminal.record("book.txt").highlights.push(Highlight {
            start: 500,
            end: 520,
            ink: Ink::Rose,
            text: "added elsewhere".into(),
            note: "from the terminal".into(),
            stale: false,
        });
        terminal.save().unwrap();
    }

    desktop.save().unwrap();

    let after = Library::load_from(store.clone());
    let marks = &after.get("book.txt").unwrap().highlights;
    assert_eq!(
        marks.len(),
        1,
        "expected exactly the other reader's new mark, got {marks:?}"
    );
    assert_eq!(marks[0].note, "from the terminal");
}

/// The terminal is the reader's, and it must be handed back.
///
/// The cleanup used to sit after the event loop, where a panic goes straight
/// past it: what is left is a shell with no echo, in which Ctrl+C does nothing
/// and the only way out is to close the window. The fix is structural — a
/// guard whose `Drop` runs on every way out — so this checks the structure,
/// which is the thing that can be lost again.
#[test]
fn the_terminal_is_handed_back_however_the_reader_leaves() {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/bin/antilib.rs"
    ))
    .unwrap();

    assert!(
        source.contains("impl Drop for TerminalGuard"),
        "nothing restores the terminal when the stack unwinds"
    );
    assert!(
        source.contains("restore_terminal_on_panic"),
        "the panic message is written onto the alternate screen, where it dies with it"
    );

    // And the guard has to be standing before the loop that can panic.
    let guard = source
        .find("let _guard = TerminalGuard")
        .expect("the guard is never actually stood up");
    let looping = source
        .find("event_loop(&mut terminal")
        .expect("the event loop moved");
    assert!(
        guard < looping,
        "the guard is set up after the loop it is meant to cover"
    );
}

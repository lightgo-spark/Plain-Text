//! Persistent reading progress and bookmarks, stored next to the user's data dir.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Bookmark {
    pub offset: usize,
    pub label: String,
}

/// The colour a passage was marked with. Stored by name so the file stays
/// readable and survives a change of palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Ink {
    Yellow,
    Mint,
    Sky,
    Rose,
}

impl Ink {
    pub const ALL: [Ink; 4] = [Ink::Yellow, Ink::Mint, Ink::Sky, Ink::Rose];

    pub fn name(self) -> &'static str {
        match self {
            Ink::Yellow => "Yellow",
            Ink::Mint => "Mint",
            Ink::Sky => "Sky",
            Ink::Rose => "Rose",
        }
    }
}

/// A marked passage: the character range in the document, the colour, and the
/// text as it read when it was marked (so the list is useful even if the file
/// on disk later changes).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Highlight {
    pub start: usize,
    pub end: usize,
    pub ink: Ink,
    #[serde(default)]
    pub text: String,
    /// What the reader had to say about the passage. Empty for a plain mark,
    /// and left out of the file entirely when it is, so a library written
    /// before notes existed reads back byte for byte the same.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
    /// Set when the marked words could not be found in the file any more.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub stale: bool,
}

impl Highlight {
    pub fn overlaps(&self, start: usize, end: usize) -> bool {
        self.start < end && start < self.end
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BookRecord {
    #[serde(default)]
    pub offset: usize,
    #[serde(default)]
    pub bookmarks: Vec<Bookmark>,
    #[serde(default)]
    pub highlights: Vec<Highlight>,
    #[serde(default)]
    pub title: String,
    /// When the book was last opened, in seconds since the epoch. Zero for
    /// records written before the reader kept track.
    #[serde(default)]
    pub last_opened: u64,
}

impl BookRecord {
    /// Fold another reader's copy of this record into this one, against the
    /// state both of them started from.
    ///
    /// The difference from [`absorb`] is what happens to a mark that is in
    /// `other` and not here: absorbing takes it, merging asks `base` whether
    /// it was ever here. If it was, this reader erased it — and an erase the
    /// other reader has not seen yet must not be undone by their save.
    ///
    /// [`absorb`]: BookRecord::absorb
    pub fn merge_from(&mut self, base: Option<&BookRecord>, other: BookRecord) {
        for b in other.bookmarks {
            let deleted_here = base.is_some_and(|r| r.bookmarks.iter().any(|m| m.offset == b.offset));
            let already_here = self.bookmarks.iter().any(|m| m.offset == b.offset);
            if !already_here && !deleted_here {
                self.bookmarks.push(b);
            }
        }
        self.bookmarks.sort_by_key(|b| b.offset);
        for h in other.highlights {
            let same = |m: &Highlight| m.start == h.start && m.end == h.end && m.ink == h.ink;
            let deleted_here = base.is_some_and(|r| r.highlights.iter().any(same));
            let already_here = self.highlights.iter().any(same);
            if !already_here && !deleted_here {
                self.highlights.push(h);
            }
        }
        self.highlights.sort_by_key(|h| h.start);
        // Where each was reading is not something to merge — it is one place,
        // and the reader who was there most recently is the one to believe.
        if other.last_opened > self.last_opened {
            self.last_opened = other.last_opened;
            self.offset = other.offset;
            if !other.title.is_empty() {
                self.title = other.title;
            }
        } else if self.title.is_empty() && !other.title.is_empty() {
            self.title = other.title;
        }
    }

    /// Take in everything `other` holds that this record does not.
    ///
    /// Used when two entries turn out to name the same book. Marks are the
    /// reader's own work, so the union is kept; the reading position and title
    /// come from whichever record was opened more recently.
    pub fn absorb(&mut self, other: BookRecord) {
        for b in other.bookmarks {
            if !self.bookmarks.iter().any(|m| m.offset == b.offset) {
                self.bookmarks.push(b);
            }
        }
        self.bookmarks.sort_by_key(|b| b.offset);
        for h in other.highlights {
            if !self
                .highlights
                .iter()
                .any(|m| m.start == h.start && m.end == h.end && m.ink == h.ink)
            {
                self.highlights.push(h);
            }
        }
        self.highlights.sort_by_key(|h| h.start);
        if other.last_opened > self.last_opened {
            self.last_opened = other.last_opened;
            self.offset = other.offset;
            if !other.title.is_empty() {
                self.title = other.title;
            }
        } else if self.title.is_empty() && !other.title.is_empty() {
            self.title = other.title;
        }
    }
}

/// What a restore actually put back, so the reader is told rather than
/// reassured. "Restored." is not an answer to "did my notes survive?".
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Restored {
    /// Books that were not on the shelf at all.
    pub added: usize,
    /// Books already here, whose marks were folded together.
    pub merged: usize,
    pub bookmarks: usize,
    pub highlights: usize,
}

impl std::fmt::Display for Restored {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.added == 0 && self.merged == 0 {
            return write!(f, "that backup held nothing this shelf does not already have");
        }
        write!(
            f,
            "{} book(s) added, {} already here; {} bookmark(s) and {} highlight(s) came back",
            self.added, self.merged, self.bookmarks, self.highlights
        )
    }
}

/// Seconds since the epoch, for stamping a record as just read.
pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Library {
    #[serde(default)]
    pub books: BTreeMap<String, BookRecord>,
    #[serde(skip)]
    path: Option<PathBuf>,
    /// The library as it stood on disk when this copy was read, and after each
    /// save. It is what tells a mark this reader *deleted* apart from one
    /// another reader has *added* since — without it, merging two open readers
    /// can only take the union, and the union resurrects every erased
    /// highlight the moment the other reader saves.
    #[serde(skip)]
    baseline: BTreeMap<String, BookRecord>,
    /// Where a store that could not be read was put, so the reader can be told
    /// that their marks are not gone, only set aside.
    #[serde(skip)]
    damaged: Option<PathBuf>,
    /// Set when a damaged store could not even be moved out of the way. The
    /// library then refuses to save: whatever is in that file is the reader's
    /// only copy, and writing over it is the one unrecoverable move.
    #[serde(skip)]
    sealed: bool,
}

fn default_path() -> Option<PathBuf> {
    let base = dirs::data_dir().or_else(dirs::home_dir)?;
    Some(base.join("anti-library").join("library.json"))
}

/// The name a book is filed under.
///
/// One file has to come out as one key however the reader was pointed at it.
/// It used to not: the terminal reader canonicalised its argument and the
/// desktop reader did not, so `antilib book.txt`, `antilib-gui book.txt` and
/// the same book opened from the file dialog filed *three* separate records —
/// three sets of bookmarks, three reading positions, and a README promising
/// the two readers carried on from each other.
///
/// Canonicalising settles the relative paths, the `..` and the symlinks; the
/// verbatim prefix Windows adds (`\\?\C:\…`) is then taken back off, because
/// it is the same path written another way and no reader should ever see it.
pub fn key_for(path: &std::path::Path) -> String {
    let full = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let s = full.to_string_lossy().to_string();
    match s.strip_prefix(r"\\?\") {
        // `\\?\UNC\server\share` is the verbatim form of `\\server\share`.
        Some(rest) => match rest.strip_prefix("UNC\\") {
            Some(unc) => format!(r"\\{unc}"),
            None => rest.to_string(),
        },
        None => s,
    }
}

/// Is this key already in the form [`key_for`] produces?
///
/// Answered without touching the filesystem, which is the whole point: the
/// question is asked of every book each time the library opens, and a book on
/// an unplugged drive charges a timeout for the answer. Resolving a path that
/// is already absolute, verbatim-free and free of `.` or `..` cannot move it,
/// so such a key is left alone.
fn looks_settled(key: &str) -> bool {
    if key.starts_with(r"\\?\") || key.is_empty() {
        return false;
    }
    if !std::path::Path::new(key).is_absolute() {
        return false;
    }
    !key.split(['/', '\\']).any(|seg| seg == "." || seg == "..")
}

/// Move a store that could not be read out of the way, and say where it went.
///
/// Renaming rather than deleting is the whole of it: whatever is in that file
/// is the only copy of somebody's reading, and the reader may well be able to
/// mend it by hand — a truncated JSON file is usually one bracket short.
fn quarantine(path: &std::path::Path) -> Option<PathBuf> {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "library".into());
    for n in 0..100 {
        let suffix = if n == 0 {
            format!("{}", now())
        } else {
            format!("{}-{n}", now())
        };
        let target = path.with_file_name(format!("{stem}.damaged-{suffix}.json"));
        if target.exists() {
            continue;
        }
        if std::fs::rename(path, &target).is_ok() {
            return Some(target);
        }
        return None;
    }
    None
}

/// Write `data` to `path` so that a machine losing power leaves either the old
/// file or the new one, never half of either.
///
/// The rename is the atomic part, but renaming a file whose bytes are still in
/// the cache can put the new *name* on the disk over the old *contents* — and
/// what comes back is an empty library. `sync_all` is what makes the rename
/// mean what the comment beside it has always claimed.
pub(crate) fn write_atomic(path: &std::path::Path, data: &[u8]) -> Result<()> {
    use std::io::Write;
    // Two readers saving at once must not share a temporary file.
    let tmp = path.with_extension(format!("json.tmp{}", std::process::id()));
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
    }
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e.into());
    }
    Ok(())
}

/// Fold the library on disk into this reader's, against the state both started
/// from.
///
/// Three-way, not a union: a record missing from `mine` but present in `base`
/// was deleted here and must stay deleted, while one present in `theirs` and
/// absent from `base` was added by the other reader and must be kept.
fn merge_books(
    base: &BTreeMap<String, BookRecord>,
    mine: &BTreeMap<String, BookRecord>,
    theirs: BTreeMap<String, BookRecord>,
) -> BTreeMap<String, BookRecord> {
    let mut out = mine.clone();
    for (key, other) in theirs {
        match out.get_mut(&key) {
            Some(ours) => ours.merge_from(base.get(&key), other),
            // Not here: either this reader took it off the shelf, or the
            // other reader put it on since.
            None => {
                if !base.contains_key(&key) {
                    out.insert(key, other);
                }
            }
        }
    }
    out
}

impl Library {
    /// Load the library, or return an empty one when the file is missing or
    /// unreadable — a corrupt store must never block reading a book.
    pub fn load() -> Library {
        match default_path() {
            Some(p) => Self::load_from(p),
            None => Library::default(),
        }
    }

    pub fn load_from(path: PathBuf) -> Library {
        // A store that is merely absent is an ordinary first run. A store that
        // is *there* and cannot be read is the reader's work in a form we do
        // not understand, and the one thing that must not happen next is
        // writing over it — which is exactly what used to happen on the next
        // page turn, silently, taking years of highlights with it.
        let (mut lib, damaged, sealed) = match std::fs::read_to_string(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                (Library::default(), None, false)
            }
            // Locked, or unreadable for want of permission. Nothing is wrong
            // with the file, so leave it exactly as it is.
            Err(_) => (Library::default(), None, true),
            Ok(text) => match serde_json::from_str::<Library>(&text) {
                Ok(l) => (l, None, false),
                Err(_) => match quarantine(&path) {
                    Some(kept) => (Library::default(), Some(kept), false),
                    None => (Library::default(), None, true),
                },
            },
        };
        lib.path = Some(path);
        lib.damaged = damaged;
        lib.sealed = sealed;
        lib.rekey();
        lib.baseline = lib.books.clone();
        lib
    }

    /// Where a store that could not be read was set aside, if that happened.
    ///
    /// The reader shows this once: their marks are not gone, and a file they
    /// can hand to someone is worth more than an apology.
    pub fn damaged_store(&self) -> Option<&std::path::Path> {
        self.damaged.as_deref()
    }

    /// True when the library will not write itself out, because doing so would
    /// destroy a store it could not read.
    pub fn is_sealed(&self) -> bool {
        self.sealed
    }

    /// Write the whole shelf out to a file the reader keeps.
    ///
    /// The store looks after itself now — it is written atomically and a
    /// damaged one is set aside rather than overwritten — but every one of
    /// those guards is about *this* machine. A disk that dies, a laptop that
    /// is replaced, a folder deleted by hand: none of them are things the
    /// program can survive on its own, and highlights are the one thing in it
    /// that cannot be made again.
    pub fn export_to(&self, path: &std::path::Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            if !dir.as_os_str().is_empty() {
                std::fs::create_dir_all(dir)?;
            }
        }
        let out = Library {
            books: self.books.clone(),
            path: None,
            baseline: BTreeMap::new(),
            damaged: None,
            sealed: false,
        };
        write_atomic(path, serde_json::to_string_pretty(&out)?.as_bytes())
    }

    /// Read a shelf out of a file and fold it into this one.
    ///
    /// A union, deliberately: restoring is asking for work back, and the
    /// three-way merge of [`Self::save`] would read a book missing from the
    /// backup as one this reader had deleted. Nothing here is ever removed —
    /// the worst a restore can do is give something back twice.
    pub fn import_from(&mut self, path: &std::path::Path) -> Result<Restored> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("cannot read {}", path.display()))?;
        let mut other: Library = serde_json::from_str(&text)
            .with_context(|| format!("{} is not a library file", path.display()))?;
        other.rekey();

        let mut report = Restored::default();
        for (key, rec) in other.books {
            match self.books.get_mut(&key) {
                Some(mine) => {
                    let (was_b, was_h) = (mine.bookmarks.len(), mine.highlights.len());
                    mine.absorb(rec);
                    report.bookmarks += mine.bookmarks.len() - was_b;
                    report.highlights += mine.highlights.len() - was_h;
                    report.merged += 1;
                }
                None => {
                    report.bookmarks += rec.bookmarks.len();
                    report.highlights += rec.highlights.len();
                    self.books.insert(key, rec);
                    report.added += 1;
                }
            }
        }
        Ok(report)
    }

    /// Take a book off the shelf.
    ///
    /// Every book ever opened stayed in the library for good, which made the
    /// start screen a list nobody could edit and — because refiling asks the
    /// filesystem about every key — made opening the library slower for ever
    /// after reading something off a drive that is no longer plugged in.
    pub fn forget(&mut self, key: &str) -> bool {
        self.books.remove(key).is_some()
    }

    /// File every record under [`key_for`], joining the ones that turn out to
    /// be the same book.
    ///
    /// A library written before the keys were settled holds the same book
    /// several times over, each copy with a piece of the reader's work in it.
    /// Throwing the duplicates away would throw those pieces away with them,
    /// so they are folded together: every bookmark, every highlight, and the
    /// reading position of whichever copy was opened last.
    fn rekey(&mut self) {
        let stale: Vec<String> = self
            .books
            .keys()
            // `key_for` asks the filesystem, and a book on a share that is no
            // longer mounted answers only after a timeout — twenty of them
            // cost 2.7 seconds before the window appeared. A key already in
            // settled form cannot be changed by resolving it, so it is not
            // worth a question: only the shapes that predate `key_for` are.
            .filter(|k| !looks_settled(k))
            .filter(|k| key_for(std::path::Path::new(k)) != **k)
            .cloned()
            .collect();
        for old in stale {
            let Some(rec) = self.books.remove(&old) else {
                continue;
            };
            let new = key_for(std::path::Path::new(&old));
            match self.books.get_mut(&new) {
                Some(kept) => kept.absorb(rec),
                None => {
                    self.books.insert(new, rec);
                }
            }
        }
    }

    /// Write the library out, keeping whatever another reader has done to it.
    ///
    /// Both readers share one file and each holds the whole of it, so a plain
    /// write puts back a picture of the library taken when this reader
    /// started — erasing every mark the other one made since. The file on disk
    /// is therefore read again here and folded in against [`Self::baseline`],
    /// which is what separates *the other reader added this* from *I deleted
    /// this*: a union would do the first and undo the second.
    pub fn save(&mut self) -> Result<()> {
        let Some(path) = self.path.clone() else {
            return Ok(());
        };
        if self.sealed {
            anyhow::bail!(
                "the library file could not be read and has been left untouched, \
                 so nothing new can be saved over it — move {} aside to start a fresh one",
                path.display()
            );
        }
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }

        let merged = match std::fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str::<Library>(&text) {
                Ok(mut disk) => {
                    disk.rekey();
                    merge_books(&self.baseline, &self.books, disk.books)
                }
                // Damaged between load and save. Keep it — it is still the
                // only copy of whatever it holds — and carry on with ours.
                Err(_) => {
                    if let Some(kept) = quarantine(&path) {
                        self.damaged = Some(kept);
                    }
                    self.books.clone()
                }
            },
            Err(_) => self.books.clone(),
        };

        let out = Library {
            books: merged,
            path: None,
            baseline: BTreeMap::new(),
            damaged: None,
            sealed: false,
        };
        write_atomic(&path, serde_json::to_string_pretty(&out)?.as_bytes())?;
        self.books = out.books;
        self.baseline = self.books.clone();
        Ok(())
    }

    pub fn record(&mut self, key: &str) -> &mut BookRecord {
        self.books.entry(key.to_string()).or_default()
    }

    pub fn get(&self, key: &str) -> Option<&BookRecord> {
        self.books.get(key)
    }

    /// Books most recently opened first, for the reader's start screen.
    ///
    /// Records written before the reader kept a timestamp sort last but keep
    /// their order, so an old library still lists something sensible.
    pub fn recent(&self) -> Vec<(&String, &BookRecord)> {
        let mut books: Vec<(&String, &BookRecord)> = self.books.iter().collect();
        books.sort_by(|a, b| b.1.last_opened.cmp(&a.1.last_opened).then(a.0.cmp(b.0)));
        books
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("antilib-test-{name}.json"))
    }

    #[test]
    fn round_trips_progress_and_bookmarks() {
        let p = tmp("roundtrip");
        let _ = std::fs::remove_file(&p);
        let mut lib = Library::load_from(p.clone());
        let rec = lib.record("book.txt");
        rec.offset = 4200;
        rec.title = "Book".into();
        rec.bookmarks.push(Bookmark {
            offset: 10,
            label: "start".into(),
        });
        lib.save().unwrap();

        let again = Library::load_from(p.clone());
        let rec = again.get("book.txt").unwrap();
        assert_eq!(rec.offset, 4200);
        assert_eq!(rec.bookmarks.len(), 1);
        assert_eq!(rec.bookmarks[0].label, "start");
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn highlights_round_trip_and_old_files_still_load() {
        let p = tmp("highlights");
        let _ = std::fs::remove_file(&p);
        // A file written before highlights existed must still load.
        std::fs::write(
            &p,
            r#"{"books":{"a.txt":{"offset":7,"bookmarks":[],"title":"A"}}}"#,
        )
        .unwrap();
        let mut lib = Library::load_from(p.clone());
        assert_eq!(lib.get("a.txt").unwrap().offset, 7);
        assert!(lib.get("a.txt").unwrap().highlights.is_empty());

        lib.record("a.txt").highlights.push(Highlight {
            start: 10,
            end: 20,
            ink: Ink::Mint,
            text: "marked".into(),
            note: String::new(),
            stale: false,
        });
        lib.save().unwrap();
        let again = Library::load_from(p.clone());
        let h = &again.get("a.txt").unwrap().highlights[0];
        assert_eq!((h.start, h.end, h.ink), (10, 20, Ink::Mint));
        assert_eq!(h.text, "marked");
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn overlap_is_half_open_so_touching_ranges_do_not_merge() {
        let h = Highlight {
            start: 10,
            end: 20,
            ink: Ink::Yellow,
            text: String::new(),
            note: String::new(),
            stale: false,
        };
        assert!(h.overlaps(15, 25));
        assert!(h.overlaps(0, 11));
        assert!(!h.overlaps(20, 30), "a range starting at the end is separate");
        assert!(!h.overlaps(0, 10));
    }

    #[test]
    fn corrupt_store_yields_empty_library() {
        let p = tmp("corrupt");
        std::fs::write(&p, "{ this is not json").unwrap();
        let lib = Library::load_from(p.clone());
        assert!(lib.books.is_empty());
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn save_overwrites_existing_file() {
        let p = tmp("overwrite");
        let _ = std::fs::remove_file(&p);
        let mut lib = Library::load_from(p.clone());
        lib.record("a").offset = 1;
        lib.save().unwrap();
        lib.record("a").offset = 2;
        lib.save().unwrap();
        assert_eq!(Library::load_from(p.clone()).get("a").unwrap().offset, 2);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn save_replaces_the_file_without_ever_removing_it() {
        // The old file must still be readable right up to the swap: a reader
        // whose machine dies mid-save keeps their bookmarks.
        let p = tmp("atomic");
        let _ = std::fs::remove_file(&p);
        let mut lib = Library::load_from(p.clone());
        lib.record("a").offset = 1;
        lib.save().unwrap();
        lib.record("a").offset = 2;
        lib.save().unwrap();
        assert_eq!(Library::load_from(p.clone()).get("a").unwrap().offset, 2);
        assert!(
            !p.with_extension("json.tmp").exists(),
            "the temporary file was left behind"
        );
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn recent_lists_the_last_opened_book_first() {
        let mut lib = Library::default();
        lib.record("a.txt").last_opened = 100;
        lib.record("b.txt").last_opened = 300;
        lib.record("c.txt").last_opened = 200;
        lib.record("z-old.txt").last_opened = 0; // written before timestamps
        let order: Vec<&str> = lib.recent().iter().map(|(p, _)| p.as_str()).collect();
        assert_eq!(order, ["b.txt", "c.txt", "a.txt", "z-old.txt"]);
    }

    #[test]
    fn one_file_gets_one_key_however_it_was_named() {
        // The defect: the terminal reader canonicalised its argument and the
        // desktop reader did not, so the same book was filed under a relative
        // path, an absolute one, and the verbatim form Windows canonicalises
        // to. Three records, three reading positions, and a README claiming
        // the two readers carried on from each other.
        let dir = std::env::temp_dir().join("antilib-key-test");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("book.txt");
        std::fs::write(&file, "text").unwrap();

        let absolute = key_for(&file);
        let canonical = key_for(&std::fs::canonicalize(&file).unwrap());
        let roundabout = key_for(&dir.join("..").join("antilib-key-test").join("book.txt"));
        assert_eq!(absolute, canonical);
        assert_eq!(absolute, roundabout);
        assert!(
            !absolute.starts_with(r"\\?\"),
            "the verbatim prefix reached the key: {absolute}"
        );
        let _ = std::fs::remove_file(&file);
    }

    #[test]
    fn a_path_that_cannot_be_resolved_is_still_a_usable_key() {
        // A book on a drive that is not plugged in must still be listed under
        // the name it was last seen at, not lost.
        let missing = PathBuf::from("Z:\\gone\\book.txt");
        assert_eq!(key_for(&missing), missing.to_string_lossy());
    }

    #[test]
    fn an_old_library_is_refiled_without_losing_anyones_marks() {
        let dir = std::env::temp_dir().join("antilib-rekey-test");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("book.txt");
        std::fs::write(&file, "text").unwrap();
        let verbatim = std::fs::canonicalize(&file).unwrap().to_string_lossy().to_string();
        let plain = key_for(&file);
        assert_ne!(verbatim, plain, "this platform needs no rekeying to test");

        // Two records for one book: the terminal reader's and the desktop's.
        // The store is cleared first — `load_from` reads what is there, so a
        // file left by an earlier run would be merged into this one's answer.
        let store = dir.join("library.json");
        let _ = std::fs::remove_file(&store);
        let mut seed = Library::load_from(store.clone());
        {
            let a = seed.record(&verbatim);
            a.offset = 900;
            a.last_opened = 200;
            a.title = "Book".into();
            a.bookmarks.push(Bookmark { offset: 10, label: "from the terminal".into() });
        }
        {
            let b = seed.record(&plain);
            b.offset = 100;
            b.last_opened = 100;
            b.bookmarks.push(Bookmark { offset: 20, label: "from the desktop".into() });
            b.highlights.push(Highlight {
                start: 0, end: 4, ink: Ink::Sky,
                text: "text".into(), note: "kept".into(), stale: false,
            });
        }
        seed.save().unwrap();

        let lib = Library::load_from(store.clone());
        assert_eq!(lib.books.len(), 1, "the book is still filed twice: {:?}", lib.books.keys());
        let rec = lib.get(&plain).expect("filed under the settled key");
        assert_eq!(rec.offset, 900, "the more recent reading position wins");
        assert_eq!(rec.title, "Book");
        assert_eq!(rec.bookmarks.len(), 2, "a bookmark was thrown away");
        assert_eq!(rec.highlights.len(), 1, "a highlight was thrown away");
        assert_eq!(rec.highlights[0].note, "kept");
        let _ = std::fs::remove_file(&store);
        let _ = std::fs::remove_file(&file);
    }

    #[test]
    fn a_note_is_kept_and_an_old_file_without_one_still_loads() {
        let p = tmp("notes");
        std::fs::write(
            &p,
            r#"{"books":{"a.txt":{"highlights":[{"start":1,"end":2,"ink":"Rose","text":"x"}]}}}"#,
        )
        .unwrap();
        let mut lib = Library::load_from(p.clone());
        assert_eq!(lib.get("a.txt").unwrap().highlights[0].note, "");
        lib.record("a.txt").highlights[0].note = "why this matters".into();
        lib.save().unwrap();
        let again = Library::load_from(p.clone());
        assert_eq!(again.get("a.txt").unwrap().highlights[0].note, "why this matters");
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn a_library_with_no_notes_writes_no_note_field() {
        // Old readers must be able to read what a new one writes.
        let mut lib = Library::default();
        lib.record("a.txt").highlights.push(Highlight {
            start: 0, end: 1, ink: Ink::Yellow,
            text: "x".into(), note: String::new(), stale: false,
        });
        let json = serde_json::to_string(&lib).unwrap();
        assert!(!json.contains("note"), "{json}");
    }

    #[test]
    fn missing_file_is_not_an_error() {
        let p = tmp("missing-never-created");
        let _ = std::fs::remove_file(&p);
        assert!(Library::load_from(p).books.is_empty());
    }
}

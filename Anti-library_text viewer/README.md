# Anti-library

A Rust reader that sets documents **like a book**. A desktop reader and a
terminal reader share one library file — reading position and bookmarks from
either, highlights from the desktop one.

Formats: **.txt · .md · Word (.docx) · PDF · EPUB · ODT · RTF · HTML** — the
prose is pulled out of each and set by the same typesetter.

```sh
cargo build --release

./target/release/antilib-gui sample.txt     # desktop reader (egui)
./target/release/antilib     sample.txt     # terminal reader (ratatui)
./target/release/antilib-bench              # benchmarks
```

## The desktop reader

### View modes (`V`, or the toolbar)

| Mode | What it is |
| --- | --- |
| **Book** | Two leaves side by side, with a spine between them |
| **Page** | One page, turned at a time |
| **Scroll** | A single unbroken column |

`Focus` dims everything but the line being read.

### Selection and highlights

- **Drag** to select, **double-click** for a word, `Ctrl+A` for the visible leaves
- Drag **past the top or bottom** and the leaf turns with the selection still
  running — a passage that crosses a page boundary can be caught whole
- `Ctrl+Z` **undoes** the last 40 changes (highlights, notes, bookmarks). The
  selection toolbar has a button for it too
- The small toolbar over a selection: four inks (Yellow / Mint / Sky / Rose),
  Copy, Erase
- Keys: `1` `2` `3` `4` to mark, `0` / `Delete` to erase, `Ctrl+C` to copy,
  `Esc` to drop the selection
- A highlight whose file has changed underneath it is **re-anchored on the words
  it was made on**. What cannot be found is kept and shown faded, not deleted
- Marks collect in the **Highlights** drawer — filter by ink, click to jump,
  right-click to **add a note** / copy / delete, `Copy all` (Markdown to the
  clipboard) and `Export…` (a `.md` file)
- **Notes**: an ink says only *that* a passage mattered. Why it mattered goes in
  a note — right-click → `Add note…`. Notes show in the drawer and come out as
  block quotes in the Markdown export

Copied text comes back as prose: a space is restored where a Latin line broke,
Hangul is joined without one, and paragraphs keep their blank line.

### Typesetting

- Line breaking is by pixel, not by character count: Latin breaks between words,
  CJK between glyphs
- **Paragraph-at-a-time line breaking (Knuth–Plass)** — instead of filling each
  line and moving on, the whole paragraph is considered and the breaks are
  chosen together. Where tightening one line makes the next three much better,
  it is tightened. This is what TeX has done since 1981.
- **Hyphenation** — English words are broken at their syllables and a dash is
  drawn (`hypher`, using TeX patterns). The dash is **drawn but never stored**:
  a row's text has to be the document's text or selection and highlights land on
  the wrong characters. Breaks leaving a single letter are refused, and Hangul is
  not hyphenated (it already breaks between glyphs).
- **Justification** — Latin stretches the spaces between words, Hangul the space
  between characters. Hangul has few spaces, so stretching only those opens
  rivers of white. A line that would stretch too far is left ranged left instead.
  Lines are never **shrunk** — the painter cannot shrink, and a typesetter that
  assumes it can pushes glyphs past the measure.
- **Drop caps** on the first paragraph of a chapter, running heads, folios, and a
  new leaf for each chapter
- The measure stays in the classic 45–75 character range (adjustable in settings)

### Everything else

| Key | Action |
| --- | --- |
| `→` `Space` `PgDn` / `←` `PgUp` | Turn a leaf (a screenful in Scroll) |
| `↑` `↓` | Scroll a line (Scroll mode) |
| Wheel | Scrolls in Scroll mode, turns a leaf per notch otherwise |
| `Ctrl` + wheel | Type size |
| **Wheel click** | Start auto-scrolling — move the pointer above or below the anchor and the page keeps going, faster the further you move. Stop with another wheel click, a left click, `Esc`, or the wheel |
| **Wheel drag** | Push the page the way a hand pushes paper |
| `Home` / `End` | Start / end |
| `Ctrl+F` → type → `Enter` | Search; `n` for the next hit |
| `Ctrl+B` | Bookmark; `Ctrl+O` opens the list |
| `C` / `H` | Contents drawer / highlights drawer |
| `T` / `V` | Cycle theme / cycle view mode |
| `Ctrl` + `+` `-` | Type size |

Four themes: Paper · Sepia · Night · Ink. Dropping a file on the window opens it.

## The terminal reader

`antilib <file>` — two leaves, contents (`c`), bookmarks (`b` / `m`), search
(`/`), themes (`t`), and `?` for the full list. Reading position and bookmarks
go to the **same library entry** as the desktop reader, so a book left in one
carries on in the other. One file gets one entry however it was named — relative
path, absolute path, or the file dialog. With both readers open at once, the one
that saves last does not erase the other's highlights.

`antilib --recent` lists the shelf **most recently read first**;
`antilib --forget <file>` takes a book off it (the file itself is untouched);
`antilib --version` prints the version. The desktop reader has the same thing on
each RECENT row of its start screen — a book that stays on the list for ever is
one the reader asks the filesystem about at every start, and after the drive it
lived on is unplugged, that is a wait before the window appears.

### Search

Search runs over the **document**, not over the typeset lines. A phrase broken
across a line break is still found (`quick brown` split in two, a Hangul word
split between glyphs), the answer does not change with the window width or the
type size, and text that has not been set yet is already searchable. The hit at
the reading position is painted darker than the others.

### Accessibility

- The window publishes an accessibility tree (`accesskit`). The page is painted
  rather than built from widgets, so **the text of the open leaves is published
  as a label** for a screen reader to read
- Body type runs **12–64 pt** for low vision. All four themes hold body text at
  **7:1 contrast or better** (WCAG AAA) and secondary text at 4.5:1, checked by
  the quality gate

## How it works

- **Format detection**: the file's own first bytes are believed before its
  extension (`%PDF`, `PK`, `{\rtf`). A PDF saved as `.txt` still opens as a PDF
- **Word / ODT**: paragraph elements (`w:p`, `text:p`) are read and field codes
  and styles are dropped. Entities like `&amp;` and `&#8217;` come back as the
  characters they stand for (the same in HTML and EPUB)
- **EPUB**: chapters are joined in spine order. A manifest `href` is a URL, so
  `%20` and friends are decoded to match the zip entry name; a chapter that is
  still missing is reported **by name**
- **PDF**: the lines a page broke are **reflowed** into paragraphs. Short lines
  end a paragraph, hyphens rejoin a word, and Hangul is joined according to the
  word-boundary signal the extractor left in the line endings
- **Encoding**: a byte order mark is believed as it stands (UTF-8, UTF-16LE,
  UTF-16BE); without one the bytes go through UTF-8 → EUC-KR → Shift_JIS →
  CP1252. HTML's own `<meta charset>` is read first. RTF's `\'hh` bytes are read
  in the code page the document declares in `\ansicpg`
- **Reading position** is stored as a **character offset into the document**, not
  as a screen coordinate, so changing the window size, the type size or the view
  mode keeps your place
- **Where things are kept**: `%APPDATA%\anti-library\library.json` (progress,
  bookmarks, highlights) and `reader.json` (settings). **Both** are written to a
  temporary file, **flushed to the disk** (`sync`), and only then renamed into
  place. If the name reaches the disk while the content is still in the cache, a
  power cut leaves an empty file — and an empty file is the doorway to the next
  point
- **A damaged library is never written over**: a store that cannot be read is
  **moved aside** as `library.damaged-<time>.json`, a new one is started, and the
  reader is told where the old one went. It used to be read as an empty library
  that was then written back over the original at the next page turn. Highlights
  and notes are the part that cannot be made again, so when the file cannot even
  be moved, **nothing is saved at all** (and it says so)
- **Two readers open at once** do not erase each other. Each save re-reads the
  file on disk and folds it in **three-way against the state it started from** —
  what the other reader added is taken, what this reader deleted stays deleted.
  (A union would resurrect every highlight you just erased.)
- **Contents**: short lines shaped like `# Title`, `Chapter N`, `PART …`,
  `제 3 장` or `1.` are taken for chapter headings
- **While opening**: reading and converting happen on **another thread**. A PDF
  of several hundred pages does not freeze the window; it keeps painting with
  `Opening …` on it
- **Size ceilings**: EPUB, DOCX and ODT are zip archives, and a zip says nothing
  trustworthy about how large it is (2 MB can unpack to 2 GB). One part may reach
  **128 MB**, a whole document **256 MB**, a plain file **512 MB**; past that the
  read stops and **says that its size is the reason**. The count is made while
  decompressing, not taken from the header — that number was chosen by whoever
  built the file
- **Crash record**: a windowed build has no console, so a panic would go nowhere.
  It is written to `crash.log` and mentioned at the next start. The file drops
  its older half past 256 KB (it used to grow without end on the machine having
  the worst time). The terminal reader **puts the screen back** before it dies —
  it used to leave a shell with no echo
- **Invisible characters**: soft hyphens (`&shy;`), zero-width spaces and a BOM
  in mid-document are dropped on the way in. Left in, they sit inside words where
  nobody can see them and quietly break a search for `cooperate`, and the copy too
- **Unicode normalisation**: text is **composed to NFC** as it is read. A file
  that spells `é` as `e` plus an accent, or Hangul as separate jamo — which is
  what a Mac usually writes — used to be unfindable with a query typed on
  Windows. Combining marks are drawn over the character before them, so they are
  **measured as zero width**; measuring them puts that line's selection and
  highlights to the right of the glyphs

### What it does not do (on purpose)

This is a reader for prose. Images, tables and equations are **dropped**. Only
text is taken out of a PDF, so a scan does not open (and says so), and the
original page layout is not reproduced. Word comments and tracked changes, and
RTF headers and footers, are not body text and are not read. A DRM-protected
EPUB cannot be opened.

**Footnotes are not dropped** — notes and endnotes from RTF and Word
(`footnotes.xml`, `endnotes.xml`) are gathered into `Footnotes` / `Endnotes`
chapters at the end of the document, where a printed book puts them.

**Arabic and Hebrew cannot be set.** The painting engine (epaint) places glyphs
left to right only — its own source says so. Such a document **says that when it
opens**, rather than showing itself quietly reversed.

**Font fallback goes as far as the listed faces.** Latin, Hangul and CJK are
found; other scripts (Thai, Indic and so on) are not. Those characters are drawn
as tofu (□), and there the **measured width and the painted width disagree** — a
missing glyph measures zero while the replacement has width — which puts that
line's selection and highlights out with it. `tests/painted_width.rs` checks that
the two agree within **0.5 pt** for the characters this machine has a face for,
and prints the ones it skipped.

There is no sync, no cloud backup, no reading across devices, and no search
across the highlights of every book. The library is one JSON file on this
machine — and **that file can be exported and restored whole** (see *Moving the
shelf* below).

**There is no automatic update.** The reader does not reach the network by
itself: put a release page in the settings and `Check for updates` opens it in a
browser, and with no address set the button is not there. `dist.ps1` also writes
`latest.json` (version, files, SHA-256) for a release page to serve as it is.

**Code signing happens when there is a certificate.**
`./dist.ps1 -CertThumbprint <thumbprint>` signs the executables and the
installer; without one the run **says loudly that it did not sign**. An unsigned
exe is stopped by SmartScreen the first time — `SHA256SUMS.txt` is there so a
download can be checked instead.

Signing is done **before** the zip is built, and the unpack check then compares
what is inside the archive against what the run claims: signing after the zip
produced a signed folder and an unsigned download while reporting success.

A certificate is not something a repository can hold. Three ways to get one, in
rough order of cost:

| | What it gives |
| --- | --- |
| **Azure Trusted Signing** | Microsoft-operated, about $10/month, and the certificate never leaves their service. Needs a verified identity — three years of business history for an organisation, or an individual account. |
| **SignPath Foundation** | Free for open source projects that qualify. The signing runs in their infrastructure from your CI. |
| **OV / EV certificate** | Bought from a CA (Sectigo, DigiCert and others), roughly $200–600 a year. EV comes on a hardware token and clears SmartScreen immediately; OV builds reputation over time. |

Whichever it is, `dist.ps1` wants only the thumbprint of a certificate in the
current user's store.

## Installing

Either of two ways.

- **Installer** `anti-library-<version>-windows-x64-setup.exe` — **no
  administrator rights.** It puts the program in `%LOCALAPPDATA%\Programs\Anti-library`,
  adds a Start menu entry, and registers an uninstall entry under Settings ›
  Apps. Associating `.epub` and adding to PATH are **off by default** — those are
  not an installer's decisions to make about somebody's machine.
- **zip** — unpack it and run `antilib-gui.exe`. Nothing is installed.

**Nothing else has to be installed either.** The shipped executables are built
with `-C target-feature=+crt-static`, so the C runtime is inside them: no Visual
C++ redistributable, no `VCRUNTIME140.dll`. Without that the program refuses to
start on a machine that has never had a C++ toolchain near it, with a message
that explains nothing. `dist.ps1` reads the import table of each staged binary
and fails the build if a CRT import is still there — a flag that is silently
dropped produces a package that looks identical and dies on somebody else's
computer.

**Uninstalling keeps your reading positions, bookmarks, highlights and notes.**
Deleting them is a separate item on the uninstall page, and it is **unticked** —
so that someone reclaiming disk space does not lose years of marginalia.

To check a download:

```powershell
Get-FileHash .\anti-library-2.0.0-windows-x64.zip -Algorithm SHA256
# compare with SHA256SUMS.txt
```

## Moving the shelf

The library is one file on this machine. A disk that dies or a laptop that is
replaced is not the kind of thing a program survives on its own, and highlights
are the only thing here that cannot be made again.

```sh
antilib --backup  shelf.json     # every book, bookmark, highlight and note
antilib --restore shelf.json     # fold it back in — nothing is removed
antilib --forget  <file>         # take one book off the shelf (the file stays)
antilib --diagnostics [file]     # what to attach to a bug report
```

The desktop reader has the same under settings (the gear) › **LIBRARY**.

- **Restoring is a union.** Unlike the three-way merge a save uses, it does not
  read a book missing from the backup as one you deleted. The worst a restore can
  do is give something back twice.
- **A diagnostics file carries no file names and no paths.** It is written to be
  sent to a stranger, and what somebody reads is their business. It holds counts,
  the version, and the crash record.

## Performance

Measured by `antilib-bench` on a synthetic Hangul/Latin corpus (this machine,
release build, best of five):

| Document | Load | Typeset (whole) | Search (one keystroke) | 1000 page jumps |
| --- | --- | --- | --- | --- |
| 64 KB | 2.5 ms | 4.0 ms | 0.17 ms | 0.02 ms |
| 1 MB | 7.5 ms | 58.6 ms | 2.60 ms | 0.04 ms |
| 5 MB | 31.7 ms | 317.7 ms | 13.22 ms | 0.07 ms |
| 20 MB | 113.7 ms | 1312.1 ms | 52.69 ms | 0.10 ms |

**Two of these numbers are larger than they used to be. They were paid for, and
what they bought is written down.**

*Typesetting* was 889 ms at 20 MB when lines were filled one at a time. Choosing
the breaks for a whole paragraph costs about 1.5× that (measured at 1 MB:
44.8 ms line-at-a-time → 52.9 ms paragraph-optimal → 58.6 ms with hyphenation).
But this is the **whole** document, and the reader never sets the whole document
— it works to a per-frame budget, so the time to the first page is the number in
the paragraph below, not the one in this table.

*Search* now scans **the document rather than the typeset lines**. That is slower
than the old figure (35.9 ms at 20 MB), but the old figure belonged to a search
that could not find a phrase broken across a line. Scanning the document also
means the part that has not been set yet is in the results from the first
keystroke.

Typesetting runs **a few milliseconds per frame**. The first screen is set as far
as it needs to be and shown at once; the rest catches up while you read (the
footer shows `setting 42%`). A 20 MB document has its first characters on screen
in about **0.46 s** — Notepad takes 0.76 s on the same file.

If your place is somewhere not yet set, the reader goes there the moment it is
reached. The page count carries a `+` (`695+`) until typesetting finishes, and
progress is computed from the position in the document rather than from page
numbers, so it does not jump about while the book is being set.

## The icon

`assets/icon.ico` is the executable's icon (`build.rs` → a Windows resource) and
the window and taskbar icon. It is compiled into the binary, so copying the exe
alone carries it. Replace the file and rebuild to change it.

## Licence

Anti-library is MIT (see `LICENSE`). It is built from 477 crates, and what they
require travels with it:

- `NOTICES.md` — which licence applies to which crate, generated from the build
  by `tools/notices.py` so it cannot claim something the build does not contain.
  Where a crate offers a choice, the one taken is written down (Apache-2.0 over
  GPL-2.0 for `self_cell`, MIT over LGPL for `r-efi`).
- `THIRD-PARTY-LICENSES.md` — the **full text** of each of those licences with
  the copyright notices they carry, gathered by `tools/licenses.py`. MIT, Apache-2.0,
  BSD, the SIL Open Font Licence and the Ubuntu Font Licence all ask for the text
  rather than a statement about it, and a reader who downloads the zip has no
  crate registry to be pointed at. Both files ship in the zip and the installer,
  and CI fails if either has drifted from the build.

**No typeface is redistributed except one.** The reader uses faces already on the
machine; when it finds none it falls back to the face embedded in
`epaint_default_fonts`, which carries Hack, Ubuntu and Noto Emoji under the SIL
Open Font Licence and the Ubuntu Font Licence. Those two texts are in
`THIRD-PARTY-LICENSES.md` for that reason.

`assets/icon.ico` is drawn for this project. `sample.txt` is written for it.
Nothing here decrypts, strips or works around DRM — a protected EPUB is refused,
not opened.

## Tests

```sh
./ci.ps1                    # every gate below
./ci.ps1 -Quick             # skips the mutation check
./dist.ps1                  # zip + installer + SHA256SUMS + latest.json
./dist.ps1 -VerifyInstaller # installs it, runs it, uninstalls it, and checks
./dist.ps1 -CertThumbprint <thumbprint>   # signing (done *before* the zip)

cargo test --release              # 223 tests
cargo clippy --all-targets        # no warnings
cargo run --bin antilib-qa        # the quality gate, 45,259 checks
python tools/mutate.py            # mutation check, 37 of 37
python tools/check_mutations.py   # no mutation left behind in the tree
python tools/notices.py --check   # NOTICES.md matches the real dependencies
python tools/licenses.py --check  # and the licence texts match it too
python tools/check_licence_cover.py   # every licence named has its text enclosed
python tools/stability.py --qc 1500 --qa 100   # the same answer, over and over
```

The **quality gate** (`src/qa.rs`) does a different job from the unit tests.
Rather than pinning one rule with one example, it runs every rule over a matrix
of 20 documents × widths, heights, queries and ranges, and holds **each answer
against one derived a different way** — search against exhaustive scanning, a
slice against the original string, incremental typesetting against setting the
whole book at once. It counts its checks and prints the number, so "the gate
passed" is a figure rather than a feeling.

The **mutation check** (`tools/mutate.py`) puts each defect back, one at a time,
and requires the test written for it to go red. A test that passes is not yet
evidence. `tools/check_mutations.py` runs first in CI and refuses a tree with a
mutation still in it — the restore is in a `finally`, which does not run when the
process is killed, and a tree in that state builds and passes while carrying a
defect somebody put back on purpose.

Typesetting, selection, persistence and format conversion are checked without a
window (a fake font is injected); the terminal reader is checked by drawing real
frames into a `TestBackend`. `tests/reported_faults.rs` reproduces once-reported
defects against real files. `tests/durability.rs` asks what becomes of the
reader's own work when the store is damaged, when two readers are open at once,
and when a save is cut off partway. `antilib-bench --dump <file>` prints the
converted paragraphs for looking at a new format by eye.

### About the Korean in the source

The reader supports Korean documents, and the tests say so in Korean because
there is no other way to say it. EUC-KR and UTF-16 decoding, NFC composition of
jamo, justification that stretches between glyphs rather than between words,
line breaking without spaces, and recognised as a chapter heading are
all behaviour that only Hangul exercises. Those strings are fixtures and product
logic, not prose — translating them would delete the coverage and the feature.

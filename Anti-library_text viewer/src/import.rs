//! Turning other document formats into the plain paragraphs the reader sets.
//!
//! Everything ends up as text with blank lines between paragraphs, which is
//! exactly what [`crate::text::Document`] already knows how to read. Layout,
//! images and formatting are dropped on purpose: this is a reader for prose.

use anyhow::{bail, Context, Result};
use quick_xml::events::Event;
use quick_xml::Reader;
use std::io::Read;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Plain text, Markdown, logs — read as they are.
    Text,
    /// Word 2007 and later (`.docx`).
    Docx,
    /// OpenDocument text (`.odt`).
    Odt,
    Epub,
    Pdf,
    Rtf,
    Html,
}

impl Format {
    pub fn name(self) -> &'static str {
        match self {
            Format::Text => "text",
            Format::Docx => "Word",
            Format::Odt => "OpenDocument",
            Format::Epub => "EPUB",
            Format::Pdf => "PDF",
            Format::Rtf => "RTF",
            Format::Html => "HTML",
        }
    }

    /// Extensions offered in the open dialog.
    pub const EXTENSIONS: &'static [&'static str] = &[
        "txt", "md", "markdown", "log", "text", "docx", "odt", "epub", "pdf", "rtf", "htm", "html",
        "xhtml", "csv", "json", "rs", "py",
    ];
}

/// Decide the format from the extension, then confirm with the file's own
/// first bytes — a `.txt` that is really a PDF should still open as a PDF.
pub fn detect(path: &Path, head: &[u8]) -> Format {
    if head.starts_with(b"%PDF") {
        return Format::Pdf;
    }
    if head.starts_with(b"{\\rtf") {
        return Format::Rtf;
    }
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let zipped = head.starts_with(b"PK\x03\x04");
    match ext.as_str() {
        "docx" if zipped => Format::Docx,
        "odt" if zipped => Format::Odt,
        "epub" if zipped => Format::Epub,
        "pdf" => Format::Pdf,
        "rtf" => Format::Rtf,
        "htm" | "html" | "xhtml" => Format::Html,
        // A zip with no telling extension: look inside for a part only one of
        // the formats has. An unreadable archive falls back to EPUB, which is
        // the zip a reader is most often handed.
        _ if zipped => zip_format(path).unwrap_or(Format::Epub),
        _ => Format::Text,
    }
}

/// Which document a zip archive really holds, judged by the parts inside it.
fn zip_format(path: &Path) -> Option<Format> {
    let zip = open_zip(path).ok()?;
    let names: Vec<String> = zip.file_names().map(str::to_string).collect();
    let has = |n: &str| names.iter().any(|f| f == n);
    if has("word/document.xml") {
        return Some(Format::Docx);
    }
    if has("META-INF/container.xml") {
        return Some(Format::Epub);
    }
    if has("content.xml") {
        return Some(Format::Odt);
    }
    None
}

/// Does this look like a file of bytes rather than of words?
///
/// A reader that happily paints an executable across the page is worse than
/// one that says no: a NUL byte, or a page full of control characters, means
/// the file was never text.
pub fn looks_binary(head: &[u8]) -> bool {
    if head.is_empty() {
        return false;
    }
    // A byte order mark is the file saying what it is. UTF-16 pads every ASCII
    // character with a NUL, so without this a Notepad "Unicode" file — about as
    // ordinary as text gets — would be turned away as binary data.
    if crate::text::bom_encoding(head).is_some() {
        return false;
    }
    if head.contains(&0) {
        return true;
    }
    // Valid UTF-8 (or a legacy codepage) has very few control characters.
    let controls = head
        .iter()
        .filter(|b| **b < 0x09 || (**b > 0x0d && **b < 0x20))
        .count();
    controls * 10 > head.len()
}

/// Read `path` and return its text, plus the format it turned out to be.
/// Plain text is returned as `None` so the caller can keep its own decoding
/// (encoding detection lives in `text.rs`).
pub fn extract(path: &Path) -> Result<(Option<String>, Format)> {
    let mut head = [0u8; 4096];
    let read = {
        let mut f = std::fs::File::open(path)
            .with_context(|| format!("cannot open {}", path.display()))?;
        // Asked before a byte is read: every path below this ends up holding
        // the whole document in memory, so the size of the file is the size of
        // the promise being made to the machine.
        if let Ok(meta) = f.metadata() {
            if meta.len() > MAX_FILE_BYTES {
                return Err(too_large(
                    &path.file_name().unwrap_or_default().to_string_lossy(),
                    MAX_FILE_BYTES,
                ));
            }
        }
        f.read(&mut head).unwrap_or(0)
    };
    let head = &head[..read];
    let format = detect(path, head);
    if format == Format::Text && looks_binary(head) {
        bail!(
            "{} is not a text document — it looks like binary data",
            path.file_name().unwrap_or_default().to_string_lossy()
        );
    }
    let text = match format {
        Format::Text => None,
        Format::Docx => Some(from_docx(path)?),
        Format::Odt => Some(from_odt(path)?),
        Format::Epub => Some(from_epub(path)?),
        Format::Pdf => Some(from_pdf(path)?),
        Format::Rtf => Some(from_rtf(path)?),
        Format::Html => Some(from_html(path)?),
    };
    Ok((text, format))
}

/// Read an HTML/XHTML file, honouring the encoding it declares.
///
/// A saved web page is as likely to be EUC-KR as UTF-8, so the bytes go through
/// the same detector the plain text reader uses — with the page's own
/// `<meta charset>` believed first, because the page knows and we are guessing.
fn from_html(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
    let src = match meta_charset(&bytes).filter(|_| crate::text::bom_encoding(&bytes).is_none()) {
        Some(enc) => enc.decode(&bytes).0.into_owned(),
        None => crate::text::decode(&bytes).0,
    };
    Ok(strip_html(&src))
}

/// The encoding named by `<meta charset=…>` or `<meta http-equiv=… charset=…>`,
/// which by the HTML standard has to sit in the first 1024 bytes.
fn meta_charset(bytes: &[u8]) -> Option<&'static encoding_rs::Encoding> {
    let head = &bytes[..bytes.len().min(4096)];
    let text = String::from_utf8_lossy(head).to_ascii_lowercase();
    let at = text.find("charset")?;
    let rest = text[at + "charset".len()..].trim_start();
    let rest = rest.strip_prefix('=')?.trim_start();
    let rest = rest.trim_start_matches(['"', '\'']);
    let label: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    encoding_rs::Encoding::for_label(label.as_bytes())
}

/// The most one part of a document may unpack to.
///
/// EPUB, DOCX and ODT are zip archives, and a zip says nothing trustworthy
/// about how large it is: two megabytes of one repeated byte unpack to two
/// gigabytes, and a reader that hands an entry to `read_to_string` will ask
/// the machine for every one of them. Measured before this ceiling existed, a
/// 2 MB EPUB reached 3.6 GB in eight seconds and was still climbing.
///
/// A part of a *book* is prose. A hundred and twenty-eight megabytes of prose
/// is some twenty million words — far past any book, and far short of trouble.
pub(crate) const MAX_ENTRY_BYTES: u64 = 128 * 1024 * 1024;

/// The most a whole document may unpack to, across all its parts. An archive
/// of a thousand entries, each just under the single-part ceiling, would
/// otherwise walk past it a slice at a time.
const MAX_DOCUMENT_BYTES: usize = 256 * 1024 * 1024;

/// The most a plain file may be before the reader declines it.
///
/// Not a bomb — a file this size really is this size — but a four gigabyte log
/// read whole is the same frozen machine from the reader's side of it, and
/// saying so beats dying.
pub const MAX_FILE_BYTES: u64 = 512 * 1024 * 1024;

/// Refused for its size.
///
/// A type of its own rather than a message, because the callers above have to
/// be able to tell it apart. `from_epub` tries a chapter under several names
/// and reports "the archive does not hold it" when none work — which is a lie
/// when the part was found and turned away, and it sends the reader looking at
/// the book for a fault that is in its size.
#[derive(Debug)]
pub struct TooLarge {
    what: String,
    limit: u64,
}

impl std::fmt::Display for TooLarge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} is larger than this reader will open ({} MB). If it is really a \
             document this long, split it; if it came from the internet, be wary — \
             an archive this small should not hold this much.",
            self.what,
            self.limit / (1024 * 1024)
        )
    }
}

impl std::error::Error for TooLarge {}

/// How much text was too much, said in the units the reader thinks in.
fn too_large(what: &str, limit: u64) -> anyhow::Error {
    anyhow::Error::new(TooLarge {
        what: what.to_string(),
        limit,
    })
}

/// Was this refusal about size? Then no other name for the same part will do
/// any better, and no caller should turn it into a different complaint.
fn is_too_large(e: &anyhow::Error) -> bool {
    e.downcast_ref::<TooLarge>().is_some()
}

fn open_zip(path: &Path) -> Result<zip::ZipArchive<std::fs::File>> {
    let file = std::fs::File::open(path)?;
    zip::ZipArchive::new(file).with_context(|| format!("{} is not a zip archive", path.display()))
}

fn read_entry(zip: &mut zip::ZipArchive<std::fs::File>, name: &str) -> Result<String> {
    let entry = zip
        .by_name(name)
        .with_context(|| format!("missing {name} inside the document"))?;
    // Counted while reading, never taken from the header: the size an archive
    // declares is a number the person who built it chose.
    let mut buf = Vec::new();
    let mut capped = entry.take(MAX_ENTRY_BYTES + 1);
    capped.read_to_end(&mut buf)?;
    if buf.len() as u64 > MAX_ENTRY_BYTES {
        return Err(too_large(&format!("{name} inside this document"), MAX_ENTRY_BYTES));
    }
    String::from_utf8(buf)
        .map_err(|_| anyhow::anyhow!("{name} inside this document is not text"))
}

/// Read an entry an EPUB pointed at by URL rather than by file name.
///
/// The manifest holds a URL, the archive holds a name, and the two differ over
/// escaping (`chapter%201.xhtml`) and sometimes over case. Every miss here used
/// to drop a chapter without a word, so the fallbacks are worth the lookup.
fn read_linked_entry(zip: &mut zip::ZipArchive<std::fs::File>, href: &str) -> Result<String> {
    let decoded = percent_decode(href);
    for name in [href.to_string(), decoded.clone()] {
        match read_entry(zip, &name) {
            Ok(text) => return Ok(text),
            // Found, and too big. Trying the next spelling of the same name
            // would only lose the reason.
            Err(e) if is_too_large(&e) => return Err(e),
            Err(_) => {}
        }
    }
    // Last resort: match on the name alone, ignoring case and leading path.
    let target = decoded.rsplit('/').next().unwrap_or(&decoded).to_lowercase();
    let found = zip
        .file_names()
        .find(|n| {
            n.rsplit('/')
                .next()
                .is_some_and(|f| f.eq_ignore_ascii_case(&target))
        })
        .map(str::to_string);
    match found {
        Some(name) => read_entry(zip, &name),
        None => bail!("missing {href} inside the document"),
    }
}

/// Word keeps its text in `word/document.xml`: `<w:p>` is a paragraph and
/// `<w:t>` the runs of text inside it.
fn from_docx(path: &Path) -> Result<String> {
    let mut zip = open_zip(path)?;
    let xml = read_entry(&mut zip, "word/document.xml")?;
    let mut out = xml_paragraphs(&xml, "w:p", &["w:t"], &["w:br", "w:tab"]);
    // Footnotes and endnotes live in their own parts, which the reader never
    // opened — so a book that argued in its notes was read with the argument
    // taken out, and nothing said so. They are gathered at the end, where a
    // printed book puts them.
    for (part, title) in [
        ("word/footnotes.xml", "Footnotes"),
        ("word/endnotes.xml", "Endnotes"),
    ] {
        let Ok(xml) = read_entry(&mut zip, part) else {
            continue;
        };
        let notes = xml_paragraphs(&xml, "w:p", &["w:t"], &["w:br", "w:tab"]);
        append_notes(&mut out, title, &notes);
    }
    Ok(out)
}

/// Add a gathered run of notes to the end of the text, under a heading.
///
/// The heading is written so [`crate::text::Document`] reads it as one, which
/// puts the notes in the contents drawer and gives them a page of their own.
fn append_notes(out: &mut String, title: &str, notes: &str) {
    // Word writes a separator paragraph or two into these parts; a note is
    // text, and the separators are not.
    let body: Vec<&str> = notes
        .lines()
        .map(str::trim)
        .filter(|l| l.chars().any(|c| c.is_alphanumeric()))
        .collect();
    if body.is_empty() {
        return;
    }
    out.push_str("\n\n");
    out.push_str(title);
    out.push_str("\n\n");
    out.push_str(&body.join("\n\n"));
    out.push('\n');
}

/// OpenDocument keeps its text in `content.xml` as `<text:p>` / `<text:h>`.
fn from_odt(path: &Path) -> Result<String> {
    let mut zip = open_zip(path)?;
    let xml = read_entry(&mut zip, "content.xml")?;
    let mut out = xml_paragraphs(&xml, "text:p", &[], &["text:line-break"]);
    if out.trim().is_empty() {
        out = xml_paragraphs(&xml, "text:h", &[], &[]);
    }
    Ok(out)
}

/// EPUB: read the spine from the OPF so chapters come out in reading order,
/// then strip the tags from each document.
fn from_epub(path: &Path) -> Result<String> {
    let mut zip = open_zip(path)?;
    let container = read_entry(&mut zip, "META-INF/container.xml")
        .context("not an EPUB: no META-INF/container.xml")?;
    let opf_path = attribute_of(&container, "rootfile", "full-path")
        .context("EPUB has no rootfile")?;
    let opf = read_linked_entry(&mut zip, &opf_path)?;
    let opf_path = percent_decode(&opf_path);
    let base = opf_path
        .rsplit_once('/')
        .map(|(dir, _)| format!("{dir}/"))
        .unwrap_or_default();

    // manifest: id -> href, then spine gives the order of the ids.
    let mut manifest: Vec<(String, String)> = Vec::new();
    let mut spine: Vec<String> = Vec::new();
    let mut reader = Reader::from_str(&opf);
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Empty(e)) | Ok(Event::Start(e)) => {
                let local = local_name(e.name().as_ref());
                if local == "item" {
                    let id = attr(&e, "id");
                    let href = attr(&e, "href");
                    if let (Some(id), Some(href)) = (id, href) {
                        manifest.push((id, href));
                    }
                } else if local == "itemref" {
                    if let Some(idref) = attr(&e, "idref") {
                        spine.push(idref);
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    let mut out = String::new();
    let mut missing: Vec<String> = Vec::new();
    for id in &spine {
        let Some((_, href)) = manifest.iter().find(|(mid, _)| mid == id) else {
            continue;
        };
        // An href may be relative to the OPF, or already absolute in the zip.
        // An href may be relative to the OPF, or already absolute in the zip;
        // the second spelling is only worth trying when the first failed for a
        // reason another name could mend. Reading 128 MB twice to be told the
        // same thing is not one of those reasons.
        let doc = read_linked_entry(&mut zip, &format!("{base}{href}"))
            .or_else(|e| if is_too_large(&e) { Err(e) } else { read_linked_entry(&mut zip, href) });
        let doc = match doc {
            Ok(d) => d,
            Err(e) if is_too_large(&e) => return Err(e),
            Err(_) => {
                missing.push(href.clone());
                continue;
            }
        };
        let text = strip_html(&doc);
        if !text.trim().is_empty() {
            out.push_str(text.trim());
            out.push_str("\n\n");
        }
        // Each chapter is under the ceiling; a thousand of them need not be.
        if out.len() > MAX_DOCUMENT_BYTES {
            return Err(too_large("this EPUB", MAX_DOCUMENT_BYTES as u64));
        }
    }
    if out.trim().is_empty() {
        // Say which part could not be found: "no readable text" sends the
        // reader looking at the book when the fault is a broken link inside it.
        if !missing.is_empty() {
            bail!(
                "this EPUB lists {} chapter(s) its archive does not hold, starting with {}",
                missing.len(),
                missing[0]
            );
        }
        bail!("no readable text found in this EPUB");
    }
    Ok(out)
}

fn from_pdf(path: &Path) -> Result<String> {
    // pdf-extract writes its own diagnostics to stdout; the text is what we
    // want and a failure here is reported to the reader as an error.
    let text = pdf_extract::extract_text(path)
        .map_err(|e| anyhow::anyhow!("could not read the text of this PDF: {e}"))?;
    // A PDF stores where each line was printed, not where paragraphs begin, so
    // the extracted text breaks wherever the page did. Join those lines back
    // into paragraphs before handing them to the typesetter, which does its
    // own line breaking for the reader's column width.
    let text = tidy_paragraphs(&reflow(&text));
    if text.trim().is_empty() {
        bail!("this PDF has no extractable text (it may be a scan)");
    }
    Ok(text)
}

/// A small RTF reader.
///
/// RTF is a tree of groups. Most of a Word file is *not* text: font tables,
/// style sheets, revision data and any group opening with `\*` are
/// destinations the reader must skip whole, or the "book" fills up with
/// markup. Text only counts when it sits outside every skipped destination.
fn from_rtf(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
    // RTF is 7-bit ASCII with escapes, so a lossy read is safe here.
    Ok(rtf_to_text(&String::from_utf8_lossy(&bytes)))
}

/// The encoding a Windows code page number stands for.
fn codepage_encoding(cp: u32) -> Option<&'static encoding_rs::Encoding> {
    let label = match cp {
        932 => "shift_jis".to_string(),
        936 => "gbk".to_string(),
        949 => "euc-kr".to_string(),
        950 => "big5".to_string(),
        874 => "windows-874".to_string(),
        866 => "ibm866".to_string(),
        65001 => "utf-8".to_string(),
        10000 => "macintosh".to_string(),
        1250..=1258 => format!("windows-{cp}"),
        _ => return None,
    };
    encoding_rs::Encoding::for_label(label.as_bytes())
}

/// The code page an RTF file declares for its `\'hh` bytes.
///
/// Word writes `\ansicpg949` for a Korean document and `\ansicpg1252` for a
/// French one; reading either as the other turns every accented letter into
/// mojibake. Where the file says nothing, `\ansi` means the Windows Latin-1
/// page, which is what the RTF specification calls for.
fn rtf_encoding(src: &str) -> &'static encoding_rs::Encoding {
    if let Some(at) = src.find("\\ansicpg") {
        let digits: String = src[at + "\\ansicpg".len()..]
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        if let Some(enc) = digits.parse::<u32>().ok().and_then(codepage_encoding) {
            return enc;
        }
    }
    if src.contains("\\mac") && !src.contains("\\ansi") {
        return encoding_rs::MACINTOSH;
    }
    encoding_rs::WINDOWS_1252
}

/// Destinations gathered up and set at the end rather than dropped.
///
/// A footnote is not body text — running it into the sentence it hangs off
/// would be worse than losing it — but it is not furniture either. It used to
/// sit in [`RTF_SKIPPED`] and go the way of the font table.
const RTF_NOTES: &[&str] = &["footnote"];

/// Destinations whose contents are never body text.
const RTF_SKIPPED: &[&str] = &[
    "fonttbl",
    "filetbl",
    "colortbl",
    "stylesheet",
    "listtable",
    "listoverridetable",
    "revtbl",
    "rsidtbl",
    "generator",
    "info",
    "pict",
    "object",
    "themedata",
    "colorschememapping",
    "latentstyles",
    "datastore",
    "xmlnstbl",
    "mmathPr",
    "header",
    "footer",
    "headerl",
    "headerr",
    "footerl",
    "footerr",
    "shppict",
    "nonshppict",
    "fldinst",
];

/// Consume the `count` stand-in characters that follow a `\u` escape.
///
/// A stand-in is one character, but it may be written as `\'hh` or as any other
/// escape, which is three or more characters of source for one character of
/// text. Group braces are never part of the run and stop it.
fn skip_rtf_fallback(chars: &mut std::iter::Peekable<std::str::Chars>, count: usize) {
    for _ in 0..count {
        match chars.peek() {
            None | Some('{') | Some('}') => return,
            Some('\\') => {
                chars.next();
                match chars.peek().copied() {
                    Some('\'') => {
                        chars.next();
                        for _ in 0..2 {
                            if chars.peek().is_some_and(char::is_ascii_hexdigit) {
                                chars.next();
                            }
                        }
                    }
                    // A control word stands in for the character.
                    Some(c) if c.is_ascii_alphabetic() => {
                        while chars.peek().is_some_and(char::is_ascii_alphanumeric) {
                            chars.next();
                        }
                        if chars.peek() == Some(&' ') {
                            chars.next();
                        }
                    }
                    Some(_) => {
                        chars.next();
                    }
                    None => return,
                }
            }
            Some(_) => {
                chars.next();
            }
        }
    }
}

pub fn rtf_to_text(src: &str) -> String {
    let mut out = String::new();
    // Footnotes, gathered as they are met and set at the end.
    let mut notes = String::new();
    let enc = rtf_encoding(src);
    let mut chars = src.chars().peekable();
    let mut depth = 0usize;
    // Depth of the outermost group currently being skipped.
    let mut skip_at: Option<usize> = None;
    // ...and of the note currently being gathered.
    let mut note_at: Option<usize> = None;
    // Bytes gathered from \'hh escapes, decoded as a run.
    let mut hex: Vec<u8> = Vec::new();
    // How many stand-in characters follow each \u, per \ucN. Group scoped, so
    // a table that sets its own count cannot leak into the body text.
    let mut uc = 1usize;
    let mut uc_stack: Vec<usize> = Vec::new();

    /// Where text goes right now: nowhere, into a note, or onto the page.
    macro_rules! emit {
        ($text:expr) => {
            if skip_at.is_none() {
                if note_at.is_some() {
                    notes.push_str(&$text.to_string());
                } else {
                    out.push_str(&$text.to_string());
                }
            }
        };
    }

    macro_rules! flush_hex {
        () => {
            if !hex.is_empty() {
                if skip_at.is_none() {
                    let (cow, _, _) = enc.decode(&hex);
                    emit!(cow);
                }
                hex.clear();
            }
        };
    }

    while let Some(c) = chars.next() {
        match c {
            '{' => {
                flush_hex!();
                depth += 1;
                uc_stack.push(uc);
            }
            '}' => {
                flush_hex!();
                if let Some(d) = skip_at {
                    if depth <= d {
                        skip_at = None;
                    }
                }
                if let Some(d) = note_at {
                    if depth <= d {
                        // One note ends where the next begins; keep them apart.
                        if !notes.is_empty() && !notes.ends_with('\n') {
                            notes.push('\n');
                        }
                        note_at = None;
                    }
                }
                depth = depth.saturating_sub(1);
                if let Some(prev) = uc_stack.pop() {
                    uc = prev;
                }
            }
            '\\' => {
                let Some(&next) = chars.peek() else { break };
                if !next.is_ascii_alphabetic() {
                    chars.next();
                    match next {
                        // \* marks the whole group as a destination to skip.
                        '*' => skip_at = skip_at.or(Some(depth)),
                        '\'' => {
                            let mut h = String::new();
                            for _ in 0..2 {
                                if let Some(&d) = chars.peek() {
                                    if d.is_ascii_hexdigit() {
                                        h.push(d);
                                        chars.next();
                                    }
                                }
                            }
                            if let Ok(b) = u8::from_str_radix(&h, 16) {
                                hex.push(b);
                            }
                            continue; // keep gathering the byte run
                        }
                        '\\' | '{' | '}' => {
                            flush_hex!();
                            emit!(next);
                        }
                        '\n' | '\r' => {
                            flush_hex!();
                            emit!('\n');
                        }
                        '~' => {
                            flush_hex!();
                            emit!('\u{00a0}');
                        }
                        _ => flush_hex!(),
                    }
                    continue;
                }

                flush_hex!();
                let mut word = String::new();
                while let Some(&n) = chars.peek() {
                    if n.is_ascii_alphabetic() {
                        word.push(n);
                        chars.next();
                    } else {
                        break;
                    }
                }
                let mut arg = String::new();
                while let Some(&n) = chars.peek() {
                    if n.is_ascii_digit() || (arg.is_empty() && n == '-') {
                        arg.push(n);
                        chars.next();
                    } else {
                        break;
                    }
                }
                if chars.peek() == Some(&' ') {
                    chars.next(); // the space after a control word is a delimiter
                }

                if RTF_SKIPPED.contains(&word.as_str()) {
                    skip_at = skip_at.or(Some(depth));
                    continue;
                }
                if RTF_NOTES.contains(&word.as_str()) {
                    note_at = note_at.or(Some(depth));
                    continue;
                }
                if skip_at.is_some() {
                    continue;
                }
                match word.as_str() {
                    "par" | "line" | "sect" | "row" => emit!('\n'),
                    "cell" | "tab" => emit!('\t'),
                    "uc" => {
                        if let Ok(n) = arg.parse::<usize>() {
                            uc = n.min(64);
                        }
                    }
                    "u" => {
                        if let Ok(n) = arg.parse::<i32>() {
                            let code = if n < 0 { (n + 65536) as u32 } else { n as u32 };
                            if let Some(ch) = char::from_u32(code) {
                                emit!(ch);
                            }
                        }
                        // The stand-ins that follow a \u must be dropped, and
                        // there are exactly `uc` of them — one by default, but
                        // Word writes \uc2 for some documents and then the
                        // second one leaks into the book.
                        skip_rtf_fallback(&mut chars, uc);
                    }
                    _ => {}
                }
            }
            '\r' | '\n' => flush_hex!(),
            _ => {
                flush_hex!();
                emit!(c);
            }
        }
    }
    let mut text = tidy_paragraphs(&out);
    append_notes(&mut text, "Footnotes", &tidy_paragraphs(&notes));
    text
}

/// Pull the readable text out of HTML/XHTML, keeping block boundaries.
pub fn strip_html(src: &str) -> String {
    let mut reader = Reader::from_str(src);
    reader.config_mut().check_end_names = false;
    let mut buf = Vec::new();
    let mut out = String::new();
    let mut skip = 0usize;
    const BLOCKS: &[&str] = &[
        "p", "div", "br", "li", "tr", "h1", "h2", "h3", "h4", "h5", "h6", "section", "blockquote",
    ];
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = local_name(e.name().as_ref());
                if matches!(name.as_str(), "script" | "style" | "head") {
                    skip += 1;
                } else if BLOCKS.contains(&name.as_str()) {
                    out.push('\n');
                    if name.starts_with('h') && name.len() == 2 {
                        out.push('\n');
                    }
                }
            }
            Ok(Event::End(e)) => {
                let name = local_name(e.name().as_ref());
                if matches!(name.as_str(), "script" | "style" | "head") {
                    skip = skip.saturating_sub(1);
                } else if BLOCKS.contains(&name.as_str()) {
                    out.push('\n');
                }
            }
            Ok(Event::Empty(e)) => {
                if BLOCKS.contains(&local_name(e.name().as_ref()).as_str()) {
                    out.push('\n');
                }
            }
            Ok(Event::Text(t)) => {
                if skip == 0 {
                    out.push_str(t.as_ref());
                }
            }
            Ok(Event::CData(t)) => {
                if skip == 0 {
                    out.push_str(t.as_ref());
                }
            }
            // `&amp;` and `&#8217;` arrive as their own event, not as text.
            Ok(Event::GeneralRef(r)) => {
                if skip == 0 {
                    out.push_str(&entity_text(r.as_ref()));
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    tidy_paragraphs(&out)
}

fn local_name(raw: &str) -> String {
    let name = raw.to_lowercase();
    name.rsplit(':').next().unwrap_or(&name).to_string()
}

/// The named references that turn up in prose. The five XML ones are required;
/// the rest are the typography an author actually writes — quotes, dashes,
/// spaces and the handful of symbols a sentence carries.
const NAMED_ENTITIES: &[(&str, &str)] = &[
    ("amp", "&"),
    ("lt", "<"),
    ("gt", ">"),
    ("quot", "\""),
    ("apos", "'"),
    ("nbsp", "\u{00a0}"),
    ("ensp", "\u{2002}"),
    ("emsp", "\u{2003}"),
    ("thinsp", "\u{2009}"),
    ("shy", "\u{00ad}"),
    ("ndash", "\u{2013}"),
    ("mdash", "\u{2014}"),
    ("horbar", "\u{2015}"),
    ("lsquo", "\u{2018}"),
    ("rsquo", "\u{2019}"),
    ("sbquo", "\u{201a}"),
    ("ldquo", "\u{201c}"),
    ("rdquo", "\u{201d}"),
    ("bdquo", "\u{201e}"),
    ("laquo", "\u{00ab}"),
    ("raquo", "\u{00bb}"),
    ("lsaquo", "\u{2039}"),
    ("rsaquo", "\u{203a}"),
    ("hellip", "\u{2026}"),
    ("bull", "\u{2022}"),
    ("middot", "\u{00b7}"),
    ("dagger", "\u{2020}"),
    ("Dagger", "\u{2021}"),
    ("prime", "\u{2032}"),
    ("Prime", "\u{2033}"),
    ("copy", "\u{00a9}"),
    ("reg", "\u{00ae}"),
    ("trade", "\u{2122}"),
    ("sect", "\u{00a7}"),
    ("para", "\u{00b6}"),
    ("deg", "\u{00b0}"),
    ("plusmn", "\u{00b1}"),
    ("times", "\u{00d7}"),
    ("divide", "\u{00f7}"),
    ("minus", "\u{2212}"),
    ("frac12", "\u{00bd}"),
    ("frac14", "\u{00bc}"),
    ("frac34", "\u{00be}"),
    ("euro", "\u{20ac}"),
    ("pound", "\u{00a3}"),
    ("yen", "\u{00a5}"),
    ("cent", "\u{00a2}"),
    ("larr", "\u{2190}"),
    ("rarr", "\u{2192}"),
    ("harr", "\u{2194}"),
    ("ne", "\u{2260}"),
    ("le", "\u{2264}"),
    ("ge", "\u{2265}"),
];

/// The text an `&…;` reference stands for.
///
/// quick-xml reports references as their own event rather than folding them
/// into the surrounding text, so anything not resolved here would simply
/// vanish from the book — `R&amp;D` would be read as `RD`. A reference this
/// does not know is therefore written back out as it was, whole and visible.
fn entity_text(name: &str) -> String {
    // XML writes `&#x41;`, HTML allows `&#X41;` as well.
    let normalised = match name.strip_prefix("#X") {
        Some(rest) => format!("#x{rest}"),
        None => name.to_string(),
    };
    if let Some(c) = quick_xml::events::BytesRef::new(normalised.as_str())
        .resolve_char_ref()
        .ok()
        .flatten()
    {
        return c.to_string();
    }
    if let Some((_, v)) = NAMED_ENTITIES.iter().find(|(k, _)| *k == name) {
        return (*v).to_string();
    }
    format!("&{name};")
}

/// Could this be the name of a reference — `amp`, `#8217`, `#x2019`?
fn is_entity_name(s: &str) -> bool {
    let body = s.strip_prefix('#').unwrap_or(s);
    !body.is_empty() && body.chars().all(|c| c.is_ascii_alphanumeric())
}

/// Resolve every `&…;` in a run of text. Used for attribute values, where
/// quick-xml hands over the source as it stands.
fn unescape(src: &str) -> String {
    if !src.contains('&') {
        return src.to_string();
    }
    let mut out = String::with_capacity(src.len());
    let mut rest = src;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        let after = &rest[at + 1..];
        match after.find(';') {
            // A reference is a short run of name characters. A stray `&` in
            // prose is not one, however soon a semicolon happens to follow.
            Some(end) if end > 0 && end <= 32 && is_entity_name(&after[..end]) => {
                out.push_str(&entity_text(&after[..end]));
                rest = &after[end + 1..];
            }
            _ => {
                out.push('&');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

fn attr(e: &quick_xml::events::BytesStart, key: &str) -> Option<String> {
    e.attributes().flatten().find_map(|a| {
        let k = local_name(a.key.as_ref());
        if k.eq_ignore_ascii_case(key) {
            // An attribute is escaped like any other XML text: a href holding
            // `&amp;` must come back as `&` before the archive is asked for it.
            Some(unescape(a.value.as_ref()))
        } else {
            None
        }
    })
}

/// Turn `%20` and friends back into the bytes they stand for.
///
/// An EPUB manifest holds URLs, but the zip it points into holds file names, so
/// a chapter called `chapter 1.xhtml` is written `chapter%201.xhtml` and has to
/// be decoded before the archive is asked for it.
fn percent_decode(src: &str) -> String {
    let bytes = src.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0usize;
    while i < bytes.len() {
        // Both digits must really be hex: `from_str_radix` alone would take
        // `%+5` for a byte.
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && bytes[i + 1].is_ascii_hexdigit()
            && bytes[i + 2].is_ascii_hexdigit()
        {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
            if let Some(b) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// First value of `attribute` on the first `<tag …>` in `xml`.
fn attribute_of(xml: &str, tag: &str, attribute: &str) -> Option<String> {
    let mut reader = Reader::from_str(xml);
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Empty(e)) | Ok(Event::Start(e)) => {
                if local_name(e.name().as_ref()) == tag {
                    if let Some(v) = attr(&e, attribute) {
                        return Some(v);
                    }
                }
            }
            Ok(Event::Eof) | Err(_) => return None,
            _ => {}
        }
        buf.clear();
    }
}

/// Walk an XML document, treating `paragraph_tag` as a paragraph break and
/// gathering the text inside. `text_tags` limits which elements contribute
/// text (empty means all of them); `break_tags` insert a space.
fn xml_paragraphs(
    xml: &str,
    paragraph_tag: &str,
    text_tags: &[&str],
    break_tags: &[&str],
) -> String {
    let para = local_name(paragraph_tag);
    let wanted: Vec<String> = text_tags
        .iter()
        .map(|t| local_name(t))
        .collect();
    let breaks: Vec<String> = break_tags
        .iter()
        .map(|t| local_name(t))
        .collect();

    let mut reader = Reader::from_str(xml);
    reader.config_mut().check_end_names = false;
    let mut buf = Vec::new();
    let mut out = String::new();
    let mut current = String::new();
    let mut depth = 0usize; // inside a wanted text element
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = local_name(e.name().as_ref());
                if wanted.contains(&name) {
                    depth += 1;
                }
            }
            Ok(Event::Empty(e)) => {
                let name = local_name(e.name().as_ref());
                if breaks.contains(&name) {
                    current.push(' ');
                }
            }
            Ok(Event::End(e)) => {
                let name = local_name(e.name().as_ref());
                if wanted.contains(&name) {
                    depth = depth.saturating_sub(1);
                } else if name == para {
                    out.push_str(current.trim());
                    out.push_str("\n\n");
                    current.clear();
                }
            }
            Ok(Event::Text(t)) => {
                if wanted.is_empty() || depth > 0 {
                    current.push_str(t.as_ref());
                }
            }
            Ok(Event::GeneralRef(r)) => {
                if wanted.is_empty() || depth > 0 {
                    current.push_str(&entity_text(r.as_ref()));
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    if !current.trim().is_empty() {
        out.push_str(current.trim());
        out.push('\n');
    }
    tidy_paragraphs(&out)
}

/// Join lines that a page broke mid-sentence back into paragraphs.
///
/// A line ends a paragraph when it ends in sentence punctuation, when the next
/// line starts something new (a bullet, a number, a capitalised heading after
/// a short line), or when a blank line already separates them.
pub fn reflow(src: &str) -> String {
    let normalised = src.replace('\r', "\n");
    // Only the left side is trimmed. A trailing space is evidence: the line
    // broke between words. Without one, a CJK line probably broke inside a
    // word and the halves must be joined with nothing between them.
    let raw: Vec<&str> = normalised.split('\n').map(str::trim_start).collect();

    // Some extractors put a blank line after *every* line of the page. Where
    // that is the pattern, a single blank means nothing and only a longer gap
    // separates paragraphs — otherwise every line would become a paragraph.
    let text_lines = raw.iter().filter(|l| !l.trim().is_empty()).count();
    let blank_lines = raw.len() - text_lines;
    let blanks_are_noise = text_lines > 2 && blank_lines * 10 >= text_lines * 8;
    // Do this source's line endings carry the word-boundary signal? Only if a
    // fair share of lines really do end in a space. A file that never does
    // (hand-typed text, most HTML) gets the safe rule instead: Korean lines
    // are joined with a space, because they usually break between words.
    let spaced_ends = raw
        .iter()
        .filter(|l| l.ends_with(' ') || l.ends_with('\t'))
        .count();
    let trust_line_endings = text_lines > 2 && spaced_ends * 10 >= text_lines * 3;
    let lines: Vec<&str> = if blanks_are_noise {
        let mut kept: Vec<&str> = Vec::with_capacity(raw.len());
        let mut run = 0usize;
        for l in &raw {
            if l.trim().is_empty() {
                run += 1;
                // Two or more blanks in a row still mean a real break.
                if run == 2 {
                    kept.push("");
                }
            } else {
                run = 0;
                kept.push(l);
            }
        }
        kept
    } else {
        raw
    };
    // With the blank lines gone, the only thing left that marks the end of a
    // paragraph is the shape of the page: a line that stops well short of the
    // measure is the last line of its paragraph — which is how a typesetter
    // reads a page too.
    let mut widths: Vec<usize> = lines
        .iter()
        .map(|l| l.trim().chars().count())
        .filter(|n| *n > 0)
        .collect();
    widths.sort_unstable();
    let full_measure = widths.get(widths.len() * 3 / 4).copied().unwrap_or(0);
    let short_line = |t: &str| full_measure > 20 && t.chars().count() * 100 < full_measure * 82;

    let mut out = String::new();
    let mut para = String::new();
    let mut previous_broke_between_words = true;

    let ends_sentence = |t: &str| {
        t.chars()
            .last()
            .is_some_and(|c| ".!?。！？…:;\"')]”』」".contains(c))
    };
    let starts_block = |t: &str| {
        let first = t.chars().next();
        first.is_some_and(|c| "•-–—*·▪◦".contains(c))
            || first.is_some_and(|c| c.is_ascii_digit()) && t.contains('.')
    };

    let flush = |para: &mut String, out: &mut String| {
        if !para.trim().is_empty() {
            out.push_str(para.trim());
            out.push_str("\n\n");
        }
        para.clear();
    };

    for (i, raw_line) in lines.iter().enumerate() {
        let broke_between_words = raw_line.ends_with(' ') || raw_line.ends_with('\t');
        let line = raw_line.trim_end();
        if line.is_empty() {
            flush(&mut para, &mut out);
            continue;
        }
        if starts_block(line) {
            flush(&mut para, &mut out);
        }
        if !para.is_empty() {
            // A hyphen at a line break is a split word: put it together.
            if para.ends_with('-') {
                para.pop();
            } else {
                // Chinese and Japanese break inside a run of glyphs, so the
                // halves join directly. Korean breaks between words, so a
                // space belongs there — as it does at every latin line break.
                let glued = |c: char| {
                    unicode_width::UnicodeWidthChar::width(c).unwrap_or(1) > 1 && !is_hangul(c)
                };
                let cjk_join = para.chars().last().is_some_and(|c| {
                    unicode_width::UnicodeWidthChar::width(c).unwrap_or(1) > 1
                }) && line.chars().next().is_some_and(|c| {
                    unicode_width::UnicodeWidthChar::width(c).unwrap_or(1) > 1
                });
                let joins_without_space = if cjk_join && trust_line_endings {
                    // Korean breaks between words, but a justified line can
                    // still split one — where the source marks word ends with
                    // a space, its absence means the word was split.
                    !previous_broke_between_words
                } else {
                    // No such evidence: join only the scripts that never put
                    // spaces between words at all.
                    para.chars().last().is_some_and(glued)
                        && line.chars().next().is_some_and(glued)
                };
                if !joins_without_space {
                    para.push(' ');
                }
            }
        }
        // Extracted text often spaces words out to fake the original
        // tracking; one space is what prose needs.
        let mut last_space = para.ends_with(' ');
        for c in line.chars() {
            if c == ' ' || c == '\u{00a0}' || c == '\t' {
                if !last_space {
                    para.push(' ');
                }
                last_space = true;
            } else {
                para.push(c);
                last_space = false;
            }
        }

        previous_broke_between_words = broke_between_words;
        let next_blank = lines.get(i + 1).is_some_and(|l| l.trim().is_empty());
        let next_is_block = lines.get(i + 1).is_some_and(|l| starts_block(l.trim()));
        let ends_paragraph = if blanks_are_noise {
            // No blank lines to go by: trust the line's own length.
            short_line(line) || next_blank || next_is_block
        } else {
            ends_sentence(line) && (next_blank || next_is_block)
        };
        if ends_paragraph {
            flush(&mut para, &mut out);
        }
    }
    flush(&mut para, &mut out);
    out
}

/// Hangul syllables, jamo, and the compatibility jamo block.
fn is_hangul(c: char) -> bool {
    matches!(c, '\u{ac00}'..='\u{d7a3}' | '\u{1100}'..='\u{11ff}' | '\u{3130}'..='\u{318f}')
}

/// Collapse runs of blank lines to one, trim trailing spaces, and drop the
/// stray whitespace that extraction tends to leave behind.
fn tidy_paragraphs(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut blanks = 0usize;
    for line in src.replace('\r', "\n").split('\n') {
        let line = line.trim_end();
        if line.trim().is_empty() {
            blanks += 1;
            continue;
        }
        if !out.is_empty() {
            out.push_str(if blanks > 0 { "\n\n" } else { "\n" });
        }
        out.push_str(line);
        blanks = 0;
    }
    out.push('\n');
    out
}

#[cfg(test)]
mod bomb_tests {
    use super::*;
    use std::io::Write;

    /// Build an archive holding one entry far larger than it looks.
    fn bomb(path: &Path, entry: &str, bytes: usize) {
        let file = std::fs::File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options: zip::write::FileOptions<'_, ()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        zip.start_file(entry, options).unwrap();
        // One repeated byte: this is all a compression bomb ever is.
        let block = vec![b'A'; 1024 * 1024];
        let mut written = 0usize;
        while written < bytes {
            let n = block.len().min(bytes - written);
            zip.write_all(&block[..n]).unwrap();
            written += n;
        }
        zip.finish().unwrap();
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join("antilib-bomb");
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    /// A document that unpacks to more than the reader will hold must be
    /// refused, not read.
    ///
    /// Before the ceiling, a 2 MB EPUB reached 3.6 GB of working set in eight
    /// seconds and was still climbing when the measurement was stopped.
    #[test]
    fn an_archive_that_unpacks_to_too_much_is_refused() {
        let path = scratch("over.docx");
        let over = (MAX_ENTRY_BYTES + 8 * 1024 * 1024) as usize;
        bomb(&path, "word/document.xml", over);

        let on_disk = std::fs::metadata(&path).unwrap().len();
        assert!(
            on_disk < 4 * 1024 * 1024,
            "the archive itself should be small — that is the whole trick ({on_disk} bytes)"
        );

        let err = from_docx(&path).expect_err("a bomb was read as a document");
        assert!(
            is_too_large(&err),
            "refused, but not for its size — the reason a reader is given matters: {err}"
        );
        let _ = std::fs::remove_file(&path);
    }

    /// And an ordinary archive still opens. A ceiling that turns away real
    /// books has not fixed anything.
    #[test]
    fn an_ordinary_archive_still_opens() {
        let path = scratch("ordinary.docx");
        let file = std::fs::File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options: zip::write::FileOptions<'_, ()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        zip.start_file("word/document.xml", options).unwrap();
        zip.write_all(
            b"<w:document><w:body><w:p><w:t>A line of prose.</w:t></w:p></w:body></w:document>",
        )
        .unwrap();
        zip.finish().unwrap();

        let text = from_docx(&path).expect("an ordinary document was turned away");
        assert!(text.contains("A line of prose."), "{text:?}");
        let _ = std::fs::remove_file(&path);
    }

    /// The EPUB path must say *why* it refused.
    ///
    /// It tries each chapter under several spellings and reports "the archive
    /// does not hold it" when none work. A part that was found and turned away
    /// for its size then arrives as a missing chapter, and the reader is sent
    /// to look at the book for a fault that is in its size.
    #[test]
    fn an_epub_refused_for_size_says_so_and_not_that_a_chapter_is_missing() {
        let path = scratch("over.epub");
        let file = std::fs::File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let stored: zip::write::FileOptions<'_, ()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        zip.start_file("META-INF/container.xml", stored).unwrap();
        zip.write_all(
            br#"<container><rootfiles><rootfile full-path="content.opf"/></rootfiles></container>"#,
        )
        .unwrap();
        zip.start_file("content.opf", stored).unwrap();
        zip.write_all(
            br#"<package><manifest><item id="c1" href="big.xhtml"/></manifest>
                <spine><itemref idref="c1"/></spine></package>"#,
        )
        .unwrap();
        zip.start_file("big.xhtml", stored).unwrap();
        let block = vec![b'A'; 1024 * 1024];
        let mut written = 0usize;
        let over = (MAX_ENTRY_BYTES + 8 * 1024 * 1024) as usize;
        while written < over {
            let n = block.len().min(over - written);
            zip.write_all(&block[..n]).unwrap();
            written += n;
        }
        zip.finish().unwrap();

        let err = from_epub(&path).expect_err("a bomb was read as a book");
        assert!(
            is_too_large(&err),
            "the reader was told a chapter is missing when it is really too large: {err}"
        );
        let _ = std::fs::remove_file(&path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn detects_by_magic_bytes_before_extension() {
        assert_eq!(
            detect(&PathBuf::from("mislabelled.txt"), b"%PDF-1.7"),
            Format::Pdf
        );
        assert_eq!(
            detect(&PathBuf::from("notes.txt"), b"{\\rtf1\\ansi"),
            Format::Rtf
        );
        assert_eq!(detect(&PathBuf::from("book.txt"), b"hello wo"), Format::Text);
    }

    #[test]
    fn detects_zip_based_formats_by_extension() {
        let pk = b"PK\x03\x04....";
        assert_eq!(detect(&PathBuf::from("a.docx"), pk), Format::Docx);
        assert_eq!(detect(&PathBuf::from("a.odt"), pk), Format::Odt);
        assert_eq!(detect(&PathBuf::from("a.epub"), pk), Format::Epub);
        // A .docx that is not a zip is not a Word file.
        assert_eq!(detect(&PathBuf::from("a.docx"), b"plain te"), Format::Text);
    }

    #[test]
    fn word_paragraphs_become_blank_line_separated_text() {
        let xml = r#"<?xml version="1.0"?>
        <w:document xmlns:w="x"><w:body>
          <w:p><w:r><w:t>First </w:t></w:r><w:r><w:t>paragraph.</w:t></w:r></w:p>
          <w:p><w:r><w:t>Second paragraph.</w:t></w:r></w:p>
          <w:p/>
          <w:p><w:r><w:t>읽지 않은 책.</w:t></w:r></w:p>
        </w:body></w:document>"#;
        let out = xml_paragraphs(xml, "w:p", &["w:t"], &["w:br", "w:tab"]);
        assert_eq!(
            out,
            "First paragraph.\n\nSecond paragraph.\n\n읽지 않은 책.\n"
        );
    }

    #[test]
    fn word_ignores_text_outside_the_run_elements() {
        // Instructions and field codes sit outside <w:t> and must not appear.
        let xml = r#"<w:document><w:body>
          <w:p><w:instrText>PAGE \* MERGEFORMAT</w:instrText><w:r><w:t>Body.</w:t></w:r></w:p>
        </w:body></w:document>"#;
        assert_eq!(xml_paragraphs(xml, "w:p", &["w:t"], &[]), "Body.\n");
    }

    #[test]
    fn html_keeps_block_boundaries_and_drops_scripts() {
        let html = "<html><head><title>t</title></head><body>\
            <script>var x = 1;</script>\
            <h1>Title</h1><p>First line.</p><p>Second <b>line</b>.</p>\
            </body></html>";
        let out = strip_html(html);
        assert!(!out.contains("var x"), "script leaked: {out}");
        assert!(!out.contains("<"), "tags leaked: {out}");
        assert!(out.contains("Title"));
        assert!(out.contains("First line."));
        assert!(out.contains("Second line."), "{out}");
    }

    #[test]
    fn html_entities_become_the_characters_they_stand_for() {
        // quick-xml reports `&…;` as its own event; ignoring it silently ate
        // the character, so `R&amp;D` was read as `RD`.
        let out = strip_html(
            "<p>R&amp;D, team&#8217;s caf&#233; &mdash; 5 &lt; 6 &amp;&amp; 7 &gt; 6.</p>",
        );
        assert_eq!(
            out.trim(),
            "R&D, team\u{2019}s caf\u{e9} \u{2014} 5 < 6 && 7 > 6."
        );
    }

    #[test]
    fn an_unknown_entity_is_kept_rather_than_dropped() {
        let out = strip_html("<p>a &notanentity; b</p>");
        assert!(out.contains("&notanentity;"), "{out}");
    }

    #[test]
    fn hex_character_references_resolve_too() {
        assert_eq!(strip_html("<p>&#x2019;&#x2014;&#X41;</p>").trim(), "\u{2019}\u{2014}A");
    }

    #[test]
    fn an_attribute_value_is_unescaped_and_url_decoded() {
        // The href an EPUB writes is a URL holding escaped XML.
        assert_eq!(unescape("a&amp;b&#8217;c"), "a&b\u{2019}c");
        // An ampersand that is not opening a reference is left alone.
        assert_eq!(unescape("Tom & Jerry; and more"), "Tom & Jerry; and more");
        assert_eq!(percent_decode("chapter%201%2Ex.html"), "chapter 1.x.html");
        assert_eq!(percent_decode("100%+5 done"), "100%+5 done");
    }

    #[test]
    fn word_entities_survive_the_conversion() {
        let xml = r#"<w:document><w:body>
          <w:p><w:r><w:t>Smith &amp; Sons&#8217; ledger</w:t></w:r></w:p>
        </w:body></w:document>"#;
        assert_eq!(
            xml_paragraphs(xml, "w:p", &["w:t"], &[]),
            "Smith & Sons\u{2019} ledger\n"
        );
    }

    #[test]
    fn html_is_read_in_the_encoding_it_was_written_in() {
        let dir = std::env::temp_dir().join("antilib-import-tests");
        std::fs::create_dir_all(&dir).unwrap();

        // Declared EUC-KR, and EUC-KR on disk.
        let (euc, _, _) = encoding_rs::EUC_KR.encode(
            "<html><head><meta charset=\"euc-kr\"></head><body><p>한글 문서입니다.</p></body></html>",
        );
        let declared = dir.join("declared.html");
        std::fs::write(&declared, &euc).unwrap();
        assert!(from_html(&declared).unwrap().contains("한글 문서입니다."));

        // No declaration at all: the byte detector has to work it out.
        let (euc, _, _) = encoding_rs::EUC_KR.encode("<html><body><p>한글 문서입니다.</p></body></html>");
        let bare = dir.join("bare.html");
        std::fs::write(&bare, &euc).unwrap();
        assert!(from_html(&bare).unwrap().contains("한글 문서입니다."));

        let _ = std::fs::remove_file(&declared);
        let _ = std::fs::remove_file(&bare);
    }

    #[test]
    fn a_utf16_text_file_is_text_not_binary() {
        let mut bytes = vec![0xFFu8, 0xFE];
        for u in "읽지 않은 책.\n".encode_utf16() {
            bytes.extend_from_slice(&u.to_le_bytes());
        }
        assert!(
            !looks_binary(&bytes),
            "a byte order mark says this is text, whatever its NUL bytes suggest"
        );
    }

    #[test]
    fn epub_chapters_are_found_through_percent_encoded_links() {
        let dir = std::env::temp_dir().join("antilib-import-tests");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("encoded.epub");
        let file = std::fs::File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let o = zip::write::SimpleFileOptions::default();
        use std::io::Write;
        zip.start_file("META-INF/container.xml", o).unwrap();
        zip.write_all(
            br#"<container><rootfiles><rootfile full-path="OEBPS/content.opf"/></rootfiles></container>"#,
        )
        .unwrap();
        zip.start_file("OEBPS/content.opf", o).unwrap();
        zip.write_all(
            br#"<package><manifest><item id="c1" href="chapter%201.xhtml"/></manifest>
                <spine><itemref idref="c1"/></spine></package>"#,
        )
        .unwrap();
        zip.start_file("OEBPS/chapter 1.xhtml", o).unwrap();
        zip.write_all(b"<html><body><p>Chapter body text.</p></body></html>")
            .unwrap();
        zip.finish().unwrap();

        let text = from_epub(&path).expect("the space in the file name is written %20 in the link");
        assert!(text.contains("Chapter body text."), "{text}");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn an_epub_that_lists_a_chapter_it_does_not_hold_says_so() {
        let dir = std::env::temp_dir().join("antilib-import-tests");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("broken.epub");
        let file = std::fs::File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let o = zip::write::SimpleFileOptions::default();
        use std::io::Write;
        zip.start_file("META-INF/container.xml", o).unwrap();
        zip.write_all(
            br#"<container><rootfiles><rootfile full-path="content.opf"/></rootfiles></container>"#,
        )
        .unwrap();
        zip.start_file("content.opf", o).unwrap();
        zip.write_all(
            br#"<package><manifest><item id="c1" href="gone.xhtml"/></manifest>
                <spine><itemref idref="c1"/></spine></package>"#,
        )
        .unwrap();
        zip.finish().unwrap();

        let err = format!("{}", from_epub(&path).unwrap_err());
        assert!(err.contains("gone.xhtml"), "unhelpful message: {err}");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_zip_is_opened_as_whatever_it_actually_holds() {
        let dir = std::env::temp_dir().join("antilib-import-tests");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("mystery.zip");
        let file = std::fs::File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        use std::io::Write;
        zip.start_file("word/document.xml", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"<w:document><w:body><w:p><w:r><w:t>Body.</w:t></w:r></w:p></w:body></w:document>")
            .unwrap();
        zip.finish().unwrap();

        assert_eq!(detect(&path, b"PK\x03\x04...."), Format::Docx);
        let (text, format) = extract(&path).unwrap();
        assert_eq!(format, Format::Docx);
        assert_eq!(text.unwrap().trim(), "Body.");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn rtf_reads_the_code_page_the_document_declares() {
        // Latin-1: the bytes mean é and ï, not the Korean they used to become.
        let latin = concat!(r"{\rtf1\ansi\ansicpg1252 caf", r"\'e9", " na", r"\'ef", r"ve\par}");
        assert_eq!(rtf_to_text(latin).trim(), "café naïve");

        // The same escapes in a Korean document are a Hangul syllable.
        let (euc, _, _) = encoding_rs::EUC_KR.encode("책");
        let korean = format!(
            "{{\\rtf1\\ansi\\ansicpg949 \\'{:02x}\\'{:02x}\\par}}",
            euc[0], euc[1]
        );
        assert_eq!(rtf_to_text(&korean).trim(), "책");
    }

    #[test]
    fn rtf_drops_exactly_as_many_stand_ins_as_uc_declares() {
        // \uc1 is the common case: one '?' per \u.
        let one = concat!(r"{\rtf1\ansi ", r"\u51069?\u51648?", " ", r"\u52293?", r".\par}");
        assert_eq!(rtf_to_text(one).trim(), "읽지 책.");

        // \uc2 writes two, and the second used to leak into the book.
        let two = concat!(
            r"{\rtf1\ansi\uc2 ",
            r"\u51069?_\u51648?_",
            " ",
            r"\u52293?_",
            r".\par}"
        );
        assert_eq!(rtf_to_text(two).trim(), "읽지 책.");

        // A stand-in written as a byte escape counts as one character too.
        let hex = concat!(r"{\rtf1\ansi ", r"\u51069\'3f\u51648\'3f", r".\par}");
        assert_eq!(rtf_to_text(hex).trim(), "읽지.");
    }

    #[test]
    fn the_stand_in_count_is_scoped_to_the_group_that_set_it() {
        // The inner group drops four stand-ins per escape; the body
        // outside it is back to one. Without the group stack the rest of
        // the book would go on eating four characters per syllable.
        let rtf = concat!(
            r"{\rtf1\ansi {\uc4 \u51069?xxx}",
            r"\u51648?",
            r".\par}"
        );
        assert_eq!(rtf_to_text(rtf).trim(), "읽지.");
    }

    #[test]
    fn rtf_keeps_the_prose_and_drops_the_machinery() {
        let rtf = concat!(
            r"{\rtf1\ansi\deff0",
            r"{\fonttbl{\f0\froman Times New Roman;}{\f1\fswiss Arial;}}",
            r"{\colortbl;\red0\green0\blue0;}",
            r"{\stylesheet{\s0 Normal;}{\s1 heading 1;}}",
            r"{\*\generator Riched20 10.0;}",
            r"{\info{\author Someone}{\title Untitled}}",
            r"\f0\fs24 First line.\par Second line.\par}"
        );
        let out = rtf_to_text(rtf);
        assert!(out.contains("First line."), "{out}");
        assert!(out.contains("Second line."), "{out}");
        for leak in [
            "Times New Roman",
            "Arial",
            "Normal",
            "heading 1",
            "Riched20",
            "Someone",
            "Untitled",
            "\\",
        ] {
            assert!(!out.contains(leak), "{leak:?} leaked into the text:\n{out}");
        }
    }

    #[test]
    fn rtf_decodes_unicode_escapes_and_their_fallbacks() {
        // Word writes Hangul as \uNNNN followed by a '?' stand-in.
        let rtf = r"{\rtf1\ansi \u51069?\u51648? \uc1\u50506?\u51008? \u52293?.\par}";
        let out = rtf_to_text(rtf);
        assert_eq!(out.trim(), "읽지 않은 책.");
        assert!(!out.contains('?'), "fallback characters leaked: {out}");
    }

    #[test]
    fn rtf_skips_starred_destinations_whole() {
        let rtf = r"{\rtf1 before {\*\shppict{\pict\pngblip 89504e47}} after\par}";
        let out = rtf_to_text(rtf);
        assert!(out.contains("before"), "{out}");
        assert!(out.contains("after"), "{out}");
        assert!(!out.contains("89504e47"), "picture data leaked: {out}");
    }

    #[test]
    fn rtf_that_is_not_rtf_does_not_panic() {
        assert_eq!(rtf_to_text("").trim(), "");
        assert_eq!(rtf_to_text(r"\").trim(), "");
        assert_eq!(rtf_to_text(r"{{{{").trim(), "");
    }

    #[test]
    fn reflow_joins_lines_a_page_broke_mid_sentence() {
        let pdf_like = "The library is a device for confronting what one does\nnot yet know; the unread shelf is the useful\nhalf.\n\nA well set page does not\nhurry the eye.\n";
        let out = reflow(pdf_like);
        assert_eq!(
            out.trim(),
            "The library is a device for confronting what one does not yet know; the unread shelf is the useful half.\n\nA well set page does not hurry the eye."
        );
    }

    #[test]
    fn reflow_joins_korean_without_adding_spaces() {
        let out = reflow("읽지 않은 책이 쌓인 서가를\n안티라이브러리라 부른다.\n");
        assert_eq!(out.trim(), "읽지 않은 책이 쌓인 서가를 안티라이브러리라 부른다.");
        let out = reflow("문장은 그보다 느리게\n스며든다.\n");
        assert_eq!(out.trim(), "문장은 그보다 느리게 스며든다.");
    }

    #[test]
    fn reflow_survives_extractors_that_double_space_every_line() {
        // pdf-extract writes a blank line after every line of the page,
        // so a single blank means nothing; the short last line of each
        // paragraph is what separates them.
        let pdf_like = "The library is a device for confronting what one does

not yet know at all; the unread shelf is the half

that is useful.

A well set page does not hurry the eye, and the

measure stays narrow enough to read at speed

without effort.
";
        let out = reflow(pdf_like);
        let paragraphs: Vec<&str> = out.trim().split("

").collect();
        assert_eq!(paragraphs.len(), 2, "expected two paragraphs:
{out}");
        assert_eq!(
            paragraphs[0],
            "The library is a device for confronting what one does not yet know at all; the unread shelf is the half that is useful."
        );
    }

    #[test]
    fn reflow_collapses_the_padding_spaces_extractors_add() {
        // A single line, so only the space handling is under test.
        let out = reflow("읽지  않은   책이  쌓인  서가.
");
        assert_eq!(out.trim(), "읽지 않은 책이 쌓인 서가.");
    }

    #[test]
    fn reflow_rejoins_korean_lines_from_a_pdf() {
        // Four lines with a blank after each, the shape pdf-extract emits.
        let pdf_like = "읽지  않은  책이  쌓인  서가를  안티라이브러리라  부른다.  이미  읽은  책보다  아직  읽지  않\n\n은  책이  더  많은  것을  아는  일. \n\n책장을  넘기는  손은  느리고. \n\n문장은  그보다  느리게  스며든다. \n";
        let out = reflow(pdf_like);
        assert!(
            out.contains("아직 읽지 않은 책이 더 많은"),
            "the line break inside a word was not repaired:
{out}"
        );
        assert!(!out.contains("  "), "double spaces survived:
{out}");
    }

    #[test]
    fn reflow_ends_a_paragraph_on_a_short_line() {
        // Without blank lines to go by, a line that stops short of the
        // measure ends its paragraph — the way a page reads.
        let long = "The library is a device for confronting what one does not yet know at all";
        let pdf_like = format!("{long}

{long}

and so it ends.

{long}

{long}
");
        let out = reflow(&pdf_like);
        let paragraphs: Vec<&str> = out.trim().split("

").collect();
        assert_eq!(paragraphs.len(), 2, "expected two paragraphs:
{out}");
        assert!(paragraphs[0].ends_with("and so it ends."), "{out}");
    }

    #[test]
    fn reflow_leaves_ordinary_text_alone() {
        // One blank line between paragraphs is the normal shape and must
        // keep meaning what it says.
        let out = reflow("First paragraph.\n\nSecond paragraph.\n");
        assert_eq!(out.trim(), "First paragraph.\n\nSecond paragraph.");
    }

    #[test]
    fn reflow_glues_japanese_but_spaces_korean() {
        // Japanese breaks mid-run, so no space is invented.
        assert_eq!(reflow("吾輩は猫で\nある。\n").trim(), "吾輩は猫である。");
        // Korean breaks between words, so the space must come back.
        assert_eq!(
            reflow("서가를\n안티라이브러리라 부른다.\n").trim(),
            "서가를 안티라이브러리라 부른다."
        );
    }

    #[test]
    fn reflow_puts_hyphenated_words_back_together() {
        assert_eq!(reflow("confront-\ning the unknown\n").trim(), "confronting the unknown");
    }

    #[test]
    fn reflow_keeps_lists_apart() {
        let out = reflow("Items:\n- first item\n- second item\n");
        assert!(out.contains("- first item\n\n- second item"), "{out}");
    }

    #[test]
    fn tidy_collapses_blank_runs_but_keeps_paragraph_breaks() {
        let src = "one\n\n\n\ntwo\nstill two\n\n\nthree   \n";
        assert_eq!(tidy_paragraphs(src), "one\n\ntwo\nstill two\n\nthree\n");
    }

    #[test]
    fn binary_data_is_not_mistaken_for_a_book() {
        // An executable, a JPEG, a .docx that is really random bytes.
        assert!(looks_binary(b"MZ\x90\x00\x03\x00\x00\x00"));
        assert!(looks_binary(&[0xff, 0xd8, 0xff, 0xe0, 0x00, 0x10]));
        assert!(looks_binary(&(0u8..32).cycle().take(400).collect::<Vec<_>>()));
    }

    #[test]
    fn ordinary_text_is_not_called_binary() {
        assert!(!looks_binary("The library is a device.\n\n".as_bytes()));
        assert!(!looks_binary("읽지 않은 책이 쌓인 서가.\n".as_bytes()));
        let (euc, _, _) = encoding_rs::EUC_KR.encode("한글 문서입니다.\n");
        assert!(!looks_binary(&euc));
        assert!(!looks_binary(b""), "an empty file is empty, not binary");
    }

    #[test]
    fn a_zip_without_the_expected_part_reports_why() {
        // A .docx with no word/document.xml is an error, not a panic.
        let dir = std::env::temp_dir().join("antilib-import-tests");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("nodoc.docx");
        let file = std::fs::File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        zip.start_file("other.xml", zip::write::SimpleFileOptions::default())
            .unwrap();
        use std::io::Write;
        zip.write_all(b"<a/>").unwrap();
        zip.finish().unwrap();

        let err = extract(&path).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("word/document.xml"), "unhelpful message: {msg}");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn missing_file_reports_the_path() {
        let err = extract(&PathBuf::from("no-such-file.docx")).unwrap_err();
        assert!(format!("{err}").contains("no-such-file.docx"));
    }
}

#[cfg(test)]
mod note_tests {
    use super::*;

    #[test]
    fn an_rtf_footnote_is_kept_and_set_at_the_end() {
        // It used to sit in RTF_SKIPPED and go the way of the font table: a
        // book that argued in its notes was read with the argument removed,
        // and nothing said so.
        let bs = '\\';
        let src = format!(
            "{{{bs}rtf1{bs}ansi Body sentence.{{{bs}footnote {bs}pard But see Smith 1997.}} More body.{bs}par}}"
        );
        let out = rtf_to_text(&src);
        assert!(out.contains("Body sentence."), "{out:?}");
        assert!(out.contains("More body."), "{out:?}");
        assert!(out.contains("But see Smith 1997."), "the note was lost: {out:?}");
        // The note is at the end, not spliced into the sentence it hangs off.
        let body_at = out.find("More body.").unwrap();
        let note_at = out.find("But see Smith 1997.").unwrap();
        assert!(note_at > body_at, "the note ran into the body: {out:?}");
        assert!(out.contains("Footnotes"), "the notes got no heading: {out:?}");
    }

    #[test]
    fn a_document_with_no_notes_gains_no_heading() {
        let bs = '\\';
        let out = rtf_to_text(&format!("{{{bs}rtf1{bs}ansi Just body text.{bs}par}}"));
        assert!(!out.contains("Footnotes"), "{out:?}");
        assert_eq!(out.trim(), "Just body text.");
    }

    #[test]
    fn separators_in_a_note_part_are_not_mistaken_for_notes() {
        let mut out = String::from("Body.\n");
        append_notes(&mut out, "Footnotes", "\n\n   \n");
        assert_eq!(out, "Body.\n", "a heading appeared over nothing");
    }
}

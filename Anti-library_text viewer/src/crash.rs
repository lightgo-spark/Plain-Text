//! What the reader leaves behind when it dies.
//!
//! A desktop build has no console: `windows_subsystem = "windows"` means a
//! panic message goes to a stream nobody is reading, the window disappears, and
//! the reader is left with a program that closed itself and no way to say why.
//! This writes the message to a file next to the library, and tells the reader
//! where to find it the next time they open the program.

use std::io::Write;
use std::path::PathBuf;

/// Where the last crash was written, if the reader keeps its files anywhere.
pub fn log_path() -> Option<PathBuf> {
    let base = dirs::data_dir().or_else(dirs::home_dir)?;
    Some(base.join("anti-library").join("crash.log"))
}

/// Start writing panics to the crash log.
///
/// The hook runs in place of the default one, so the message is also still
/// printed — a terminal build is worth keeping useful.
pub fn install() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        write(info);
        previous(info);
    }));
}

fn write(info: &std::panic::PanicHookInfo<'_>) {
    let Some(path) = log_path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let at = info
        .location()
        .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
        .unwrap_or_else(|| "an unknown place".into());
    let what = info
        .payload()
        .downcast_ref::<&str>()
        .map(|s| (*s).to_string())
        .or_else(|| info.payload().downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "no message".into());
    let when = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    trim(&path);
    // Appended, not replaced: the crash that matters is often not the last one.
    let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) else {
        return;
    };
    let _ = writeln!(
        f,
        "{when} anti-library {} panicked at {at}: {what}",
        env!("CARGO_PKG_VERSION")
    );
}

/// How large the record may grow before its older half is dropped.
///
/// A program that crashes in a loop writes a line every time, and this file is
/// only ever appended to — left alone it grows without end on the machine of
/// the reader having the worst time with it. A quarter of a megabyte is some
/// thousands of crashes: far more history than anyone needs, and small enough
/// to send to somebody.
const MAX_LOG_BYTES: u64 = 256 * 1024;

/// Drop the older half of the record once it passes [`MAX_LOG_BYTES`].
///
/// Runs from inside the panic hook, so it does nothing that can panic itself:
/// a second panic there takes the process down with no message at all.
fn trim(path: &std::path::Path) {
    if std::fs::metadata(path).map(|m| m.len()).unwrap_or(0) <= MAX_LOG_BYTES {
        return;
    }
    let Ok(text) = std::fs::read_to_string(path) else {
        return;
    };
    // Half way, then forward to a character boundary, then to the start of the
    // next line — slicing a string anywhere else is itself a panic.
    let mut at = text.len() / 2;
    while at < text.len() && !text.is_char_boundary(at) {
        at += 1;
    }
    let at = text[at..].find('\n').map(|i| at + i + 1).unwrap_or(text.len());
    let kept = format!("(earlier crashes dropped to keep this file small)\n{}", &text[at..]);
    let _ = crate::library::write_atomic(path, kept.as_bytes());
}

/// The last crash the reader recorded, if there is one worth mentioning.
///
/// Read once at start-up. The file is left alone — it is the only account of
/// what happened, and the reader may want to send it on.
pub fn last() -> Option<String> {
    let path = log_path()?;
    let text = std::fs::read_to_string(&path).ok()?;
    let line = text.lines().rfind(|l| !l.trim().is_empty())?;
    Some(line.to_string())
}

/// Forget the recorded crashes, once the reader has been told.
pub fn clear() {
    if let Some(p) = log_path() {
        let _ = std::fs::remove_file(p);
    }
}

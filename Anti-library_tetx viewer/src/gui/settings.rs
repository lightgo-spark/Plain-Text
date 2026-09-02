//! Reader preferences, persisted beside the library.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// How the text is presented.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViewMode {
    /// Two leaves side by side, like an open book.
    Book,
    /// A single page, turned one at a time.
    Page,
    /// One continuous column that scrolls.
    Scroll,
}

impl ViewMode {
    pub const ALL: [ViewMode; 3] = [ViewMode::Book, ViewMode::Page, ViewMode::Scroll];

    pub fn name(self) -> &'static str {
        match self {
            ViewMode::Book => "Book",
            ViewMode::Page => "Page",
            ViewMode::Scroll => "Scroll",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            ViewMode::Book => "Two leaves side by side",
            ViewMode::Page => "One page at a time",
            ViewMode::Scroll => "One continuous column",
        }
    }

    pub fn columns(self) -> usize {
        match self {
            ViewMode::Book => 2,
            _ => 1,
        }
    }

    pub fn next(self) -> ViewMode {
        match self {
            ViewMode::Book => ViewMode::Page,
            ViewMode::Page => ViewMode::Scroll,
            ViewMode::Scroll => ViewMode::Book,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SkinChoice {
    Paper,
    Sepia,
    Night,
    Ink,
}

impl From<SkinChoice> for super::theme::Skin {
    fn from(c: SkinChoice) -> Self {
        match c {
            SkinChoice::Paper => super::theme::Skin::Paper,
            SkinChoice::Sepia => super::theme::Skin::Sepia,
            SkinChoice::Night => super::theme::Skin::Night,
            SkinChoice::Ink => super::theme::Skin::Ink,
        }
    }
}

impl From<super::theme::Skin> for SkinChoice {
    fn from(s: super::theme::Skin) -> Self {
        match s {
            super::theme::Skin::Paper => SkinChoice::Paper,
            super::theme::Skin::Sepia => SkinChoice::Sepia,
            super::theme::Skin::Night => SkinChoice::Night,
            super::theme::Skin::Ink => SkinChoice::Ink,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub skin: SkinChoice,
    /// Body type size in points.
    pub font_size: f32,
    /// Leading as a multiple of the type size.
    pub line_height: f32,
    /// Measure (column width) in characters — the classic 45–75 range.
    pub measure: f32,
    pub mode: ViewMode,
    /// Dim everything but the passage being read.
    pub focus: bool,
    pub justify: bool,
    pub drop_caps: bool,
    pub chapter_breaks: bool,
    /// Break words at their syllables so the column fills evenly.
    pub hyphenate: bool,
    pub page_animation: bool,
    pub last_book: Option<String>,
    /// Where new versions are announced, if anywhere.
    ///
    /// The reader does not reach the network by itself and is not going to
    /// start: it opens this page in a browser when asked, and that is the
    /// whole of it. Empty by default because this build has no release page —
    /// writing one in would be pretending there is somewhere to look.
    pub updates_url: Option<String>,
    /// Where these settings came from, and where [`Settings::save`] puts them
    /// back. `None` means nowhere — which is what a test wants, and what keeps
    /// one from writing over the settings of whoever is running it.
    #[serde(skip)]
    path: Option<PathBuf>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            skin: SkinChoice::Paper,
            font_size: 18.0,
            line_height: 1.65,
            measure: 62.0,
            mode: ViewMode::Book,
            focus: false,
            justify: true,
            drop_caps: true,
            chapter_breaks: true,
            hyphenate: true,
            page_animation: true,
            last_book: None,
            updates_url: None,
            path: None,
        }
    }
}

/// Is this something we are willing to hand to the browser?
///
/// Only http and https, and nothing that could be read as an argument. The
/// alternative is passing whatever is in a text file to the shell.
pub fn is_web_url(u: &str) -> bool {
    let lower = u.to_ascii_lowercase();
    (lower.starts_with("https://") || lower.starts_with("http://"))
        && u.len() > 10
        && u.len() < 2048
        && !u.contains(char::is_whitespace)
        && !u.contains('"')
}

pub const MIN_FONT: f32 = 12.0;
/// The largest body size the reader will set.
///
/// It was 34pt, which is a comfortable large-print book and nowhere near what
/// a reader with low vision needs; the usual ask is three to four times the
/// ordinary 12pt. The typesetter does not mind how big the type is — the
/// measure keeps the column sane at any size — so the ceiling was only ever
/// the slider's.
pub const MAX_FONT: f32 = 64.0;

impl Settings {
    fn default_path() -> Option<PathBuf> {
        let base = dirs::data_dir().or_else(dirs::home_dir)?;
        Some(base.join("anti-library").join("reader.json"))
    }

    pub fn load() -> Settings {
        match Self::default_path() {
            Some(p) => Self::load_from(p),
            None => Settings::default(),
        }
    }

    /// Read the settings from a named file, remembering it for [`save`].
    ///
    /// [`save`]: Settings::save
    pub fn load_from(path: PathBuf) -> Settings {
        let mut s: Settings = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        s.path = Some(path);
        s
    }

    /// Write the settings out through a temporary file.
    ///
    /// Settings are saved on every slider drag, so a plain overwrite would be
    /// truncating this file dozens of times a session — and one interruption
    /// there leaves the reader starting up with a half-written file.
    pub fn save(&self) {
        let Some(path) = self.path.clone() else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let Ok(json) = serde_json::to_string_pretty(self) else {
            return;
        };
        // Through the same atomic write the library uses: the bytes reach the
        // disk before the name is swapped. Saved on every slider drag, this is
        // the file most likely to be mid-write when a machine goes down, and a
        // half-written one is what greets the reader at the next start.
        let _ = crate::library::write_atomic(&path, json.as_bytes());
    }

    /// Clamp anything a hand-edited file (or an old version) could get wrong.
    pub fn sanitised(mut self) -> Settings {
        self.font_size = self.font_size.clamp(MIN_FONT, MAX_FONT);
        self.line_height = self.line_height.clamp(1.1, 2.4);
        self.measure = self.measure.clamp(38.0, 96.0);
        // This one is handed to the shell. A settings file is an ordinary text
        // file that anything on the machine can write, so a value that is not
        // plainly a web address does not get to be one.
        self.updates_url = self.updates_url.filter(|u| is_web_url(u));
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_within_the_allowed_range() {
        let d = Settings::default();
        let s = d.clone().sanitised();
        assert_eq!(d.font_size, s.font_size);
        assert_eq!(d.line_height, s.line_height);
        assert_eq!(d.measure, s.measure);
    }

    #[test]
    fn nonsense_values_are_clamped() {
        let s = Settings {
            font_size: 900.0,
            line_height: -3.0,
            measure: 5.0,
            ..Default::default()
        }
        .sanitised();
        assert_eq!(s.font_size, MAX_FONT);
        assert!(s.line_height >= 1.1);
        assert!(s.measure >= 38.0);
    }

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let s: Settings = serde_json::from_str(r#"{"font_size": 20.0}"#).unwrap();
        assert_eq!(s.font_size, 20.0);
        assert_eq!(s.measure, Settings::default().measure);
        assert!(s.justify);
    }

    #[test]
    fn a_file_from_before_view_modes_still_loads() {
        let s: Settings = serde_json::from_str(r#"{"font_size":21.0,"spread":false}"#).unwrap();
        assert_eq!(s.font_size, 21.0);
        assert_eq!(s.mode, ViewMode::Book, "unknown fields must not break it");
    }

    #[test]
    fn modes_cycle_and_know_their_column_count() {
        assert_eq!(ViewMode::Book.columns(), 2);
        assert_eq!(ViewMode::Page.columns(), 1);
        assert_eq!(ViewMode::Scroll.columns(), 1);
        let mut m = ViewMode::Book;
        for _ in 0..3 {
            m = m.next();
        }
        assert_eq!(m, ViewMode::Book);
    }

    /// The one setting that is handed to the shell.
    ///
    /// `reader.json` is an ordinary file in the user's profile, and anything
    /// running as that user can write it. What comes back out of it must not
    /// be able to become a command.
    #[test]
    fn only_a_plain_web_address_is_ever_opened() {
        for good in [
            "https://example.com/releases",
            "http://example.com/r",
            "https://example.com/a/b?c=d#e",
        ] {
            assert!(is_web_url(good), "{good} should be allowed");
        }
        for bad in [
            "file:///C:/Windows/System32/calc.exe",
            "javascript:alert(1)",
            r"C:\Windows\System32\calc.exe",
            "https://example.com/a b",
            "\"https://example.com\" & calc",
            "ftp://example.com/x",
            "https://",
            "",
            "example.com",
        ] {
            assert!(!is_web_url(bad), "{bad:?} should be refused");
        }
    }

    /// And a settings file carrying one of those is cleaned as it is read.
    #[test]
    fn a_settings_file_cannot_smuggle_one_in() {
        let s: Settings = serde_json::from_str(
            r#"{"updates_url":"file:///C:/Windows/System32/calc.exe"}"#,
        )
        .unwrap();
        assert!(s.updates_url.is_some(), "it is there before sanitising");
        assert_eq!(s.sanitised().updates_url, None, "and gone after");
    }

    #[test]
    fn skin_choice_round_trips() {
        for skin in super::super::theme::Skin::ALL {
            let c: SkinChoice = skin.into();
            let back: super::super::theme::Skin = c.into();
            assert_eq!(skin, back);
        }
    }
}

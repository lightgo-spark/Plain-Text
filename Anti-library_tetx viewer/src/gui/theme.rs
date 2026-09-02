//! Visual design: paper stocks, ink colours, type, spacing.
//!
//! The reader is built to look like a printed page floating on a desk, so the
//! palette names are printer's terms: `page` is the stock, `ink` the type,
//! `canvas` the desk it rests on.

use crate::library::Ink;
use egui::{Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke, Visuals};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Skin {
    Paper,
    Sepia,
    Night,
    Ink,
}

impl Skin {
    pub const ALL: [Skin; 4] = [Skin::Paper, Skin::Sepia, Skin::Night, Skin::Ink];

    pub fn name(self) -> &'static str {
        match self {
            Skin::Paper => "Paper",
            Skin::Sepia => "Sepia",
            Skin::Night => "Night",
            Skin::Ink => "Ink",
        }
    }

    pub fn next(self) -> Skin {
        match self {
            Skin::Paper => Skin::Sepia,
            Skin::Sepia => Skin::Night,
            Skin::Night => Skin::Ink,
            Skin::Ink => Skin::Paper,
        }
    }

    pub fn is_dark(self) -> bool {
        matches!(self, Skin::Night | Skin::Ink)
    }

    pub fn palette(self) -> Palette {
        match self {
            Skin::Paper => Palette {
                canvas: Color32::from_rgb(0xE4, 0xE1, 0xDA),
                page: Color32::from_rgb(0xFD, 0xFC, 0xF9),
                page_edge: Color32::from_rgb(0xE8, 0xE4, 0xDA),
                ink: Color32::from_rgb(0x24, 0x23, 0x20),
                ink_soft: Color32::from_rgb(0x6B, 0x66, 0x5E),
                ink_faint: Color32::from_rgb(0xA8, 0xA2, 0x98),
                accent: Color32::from_rgb(0x9C, 0x3C, 0x28),
                panel: Color32::from_rgb(0xF3, 0xF1, 0xEB),
                hairline: Color32::from_rgb(0xD8, 0xD3, 0xC8),
                highlight: Color32::from_rgb(0xF7, 0xE2, 0x9B),
                shadow: Color32::from_black_alpha(28),
            },
            Skin::Sepia => Palette {
                canvas: Color32::from_rgb(0xDE, 0xCE, 0xAE),
                page: Color32::from_rgb(0xF7, 0xEC, 0xD6),
                page_edge: Color32::from_rgb(0xE6, 0xD6, 0xB6),
                ink: Color32::from_rgb(0x3E, 0x2E, 0x1E),
                ink_soft: Color32::from_rgb(0x7A, 0x63, 0x45),
                ink_faint: Color32::from_rgb(0xB2, 0x99, 0x73),
                accent: Color32::from_rgb(0x8A, 0x4B, 0x1C),
                panel: Color32::from_rgb(0xEC, 0xDE, 0xC1),
                hairline: Color32::from_rgb(0xCF, 0xBB, 0x96),
                highlight: Color32::from_rgb(0xE3, 0xC0, 0x77),
                shadow: Color32::from_black_alpha(34),
            },
            Skin::Night => Palette {
                canvas: Color32::from_rgb(0x0F, 0x11, 0x15),
                page: Color32::from_rgb(0x1A, 0x1D, 0x23),
                page_edge: Color32::from_rgb(0x24, 0x28, 0x30),
                ink: Color32::from_rgb(0xD3, 0xD6, 0xDC),
                ink_soft: Color32::from_rgb(0x97, 0x9D, 0xA9),
                ink_faint: Color32::from_rgb(0x60, 0x67, 0x74),
                accent: Color32::from_rgb(0x86, 0xB6, 0xE0),
                panel: Color32::from_rgb(0x16, 0x19, 0x1E),
                hairline: Color32::from_rgb(0x2C, 0x31, 0x3A),
                highlight: Color32::from_rgb(0x3A, 0x4C, 0x63),
                shadow: Color32::from_black_alpha(90),
            },
            Skin::Ink => Palette {
                canvas: Color32::BLACK,
                // The sheet has to read as a sheet even here, so it sits a
                // shade above the desk and carries a visible edge.
                page: Color32::from_rgb(0x0D, 0x0D, 0x0D),
                page_edge: Color32::from_rgb(0x3A, 0x3A, 0x3A),
                ink: Color32::from_rgb(0xF2, 0xF2, 0xF2),
                ink_soft: Color32::from_rgb(0xB6, 0xB6, 0xB6),
                ink_faint: Color32::from_rgb(0x70, 0x70, 0x70),
                accent: Color32::from_rgb(0x7A, 0xE0, 0xD0),
                panel: Color32::from_rgb(0x0E, 0x0E, 0x0E),
                hairline: Color32::from_rgb(0x2A, 0x2A, 0x2A),
                highlight: Color32::from_rgb(0x4A, 0x4A, 0x1E),
                shadow: Color32::from_black_alpha(120),
            },
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub canvas: Color32,
    pub page: Color32,
    pub page_edge: Color32,
    pub ink: Color32,
    pub ink_soft: Color32,
    pub ink_faint: Color32,
    pub accent: Color32,
    pub panel: Color32,
    pub hairline: Color32,
    pub highlight: Color32,
    pub shadow: Color32,
}

/// Named font families. `Serif` sets the book text; `Ui` the chrome.
pub const SERIF: &str = "book-serif";
pub const SANS: &str = "book-sans";
pub const ICON: &str = "book-icon";

pub fn serif(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SERIF.into()))
}

pub fn sans(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SANS.into()))
}

pub fn icon(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(ICON.into()))
}

/// Candidate faces, best first. Missing files are skipped, so the reader still
/// starts on a machine that has none of them.
const SERIF_FACES: &[&str] = &[
    r"C:\Windows\Fonts\georgia.ttf",
    r"C:\Windows\Fonts\constan.ttf",
    r"C:\Windows\Fonts\times.ttf",
    "/System/Library/Fonts/Supplemental/Georgia.ttf",
    "/System/Library/Fonts/Supplemental/Times New Roman.ttf",
    "/System/Library/Fonts/NewYork.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSerif.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationSerif-Regular.ttf",
    "/usr/share/fonts/TTF/DejaVuSerif.ttf",
];
const SANS_FACES: &[&str] = &[
    r"C:\Windows\Fonts\segoeui.ttf",
    r"C:\Windows\Fonts\tahoma.ttf",
    "/System/Library/Fonts/SFNS.ttf",
    "/System/Library/Fonts/Helvetica.ttc",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
    "/usr/share/fonts/TTF/DejaVuSans.ttf",
];
/// Interface icons. Windows ships these; elsewhere the buttons fall back to
/// the short text labels in [`ICON_FALLBACK`].
const ICON_FACES: &[&str] = &[
    r"C:\Windows\Fonts\segmdl2.ttf",
    r"C:\Windows\Fonts\SegoeIcons.ttf",
];

/// Korean (and other CJK) coverage, appended to both families as a fallback.
const CJK_FACES: &[&str] = &[
    r"C:\Windows\Fonts\malgun.ttf",
    r"C:\Windows\Fonts\NanumGothic.ttf",
    r"C:\Windows\Fonts\gulim.ttc",
    "/System/Library/Fonts/AppleSDGothicNeo.ttc",
    "/System/Library/Fonts/Supplemental/AppleGothic.ttf",
    "/usr/share/fonts/truetype/nanum/NanumGothic.ttf",
    "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
];

/// Whether the icon face was found at start-up.
static ICONS_AVAILABLE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn icons_available() -> bool {
    ICONS_AVAILABLE.load(std::sync::atomic::Ordering::Relaxed)
}

/// Load the first face that is actually on this machine, and say which it was.
///
/// The name is worth keeping: the reader falls back silently through the list,
/// so without it there is no way to tell whether the page is set in Georgia or
/// in whatever egui ships with.
fn load_first(paths: &[&str], fonts: &mut FontDefinitions, key: &str) -> Option<String> {
    for p in paths {
        if let Ok(bytes) = std::fs::read(p) {
            fonts
                .font_data
                .insert(key.to_string(), Arc::new(FontData::from_owned(bytes)));
            return Some(
                std::path::Path::new(p)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| (*p).to_string()),
            );
        }
    }
    None
}

/// Install the book faces. Returns the names actually loaded, for the About box.
pub fn install_fonts(ctx: &egui::Context) -> Vec<String> {
    let mut fonts = FontDefinitions::default();
    let mut loaded = Vec::new();

    let serif_face = load_first(SERIF_FACES, &mut fonts, "serif");
    let icon_face = load_first(ICON_FACES, &mut fonts, "icons");
    let sans_face = load_first(SANS_FACES, &mut fonts, "sans");
    let cjk_face = load_first(CJK_FACES, &mut fonts, "cjk");
    let (has_icons, has_sans, has_cjk) = (
        icon_face.is_some(),
        sans_face.is_some(),
        cjk_face.is_some(),
    );

    let mut serif_family = Vec::new();
    if let Some(name) = &serif_face {
        serif_family.push("serif".to_owned());
        loaded.push(name.clone());
    }
    if let Some(name) = &cjk_face {
        serif_family.push("cjk".to_owned());
        loaded.push(name.clone());
    }
    serif_family.extend(fonts.families[&FontFamily::Proportional].clone());

    let mut sans_family = Vec::new();
    if let Some(name) = &sans_face {
        sans_family.push("sans".to_owned());
        loaded.push(name.clone());
    }
    if has_cjk {
        sans_family.push("cjk".to_owned());
    }
    sans_family.extend(fonts.families[&FontFamily::Proportional].clone());

    // Korean text must render in the default families too (tooltips, buttons).
    if has_cjk {
        fonts
            .families
            .entry(FontFamily::Proportional)
            .or_default()
            .push("cjk".to_owned());
    }
    fonts
        .families
        .insert(FontFamily::Name(SERIF.into()), serif_family);
    fonts
        .families
        .insert(FontFamily::Name(SANS.into()), sans_family);
    // The icon family falls back to the UI face, so a missing glyph shows as
    // ordinary text rather than a blank box.
    let mut icon_family = Vec::new();
    if let Some(name) = &icon_face {
        icon_family.push("icons".to_owned());
        loaded.push(name.clone());
    }
    if has_sans {
        icon_family.push("sans".to_owned());
    }
    icon_family.extend(fonts.families[&FontFamily::Proportional].clone());
    fonts
        .families
        .insert(FontFamily::Name(ICON.into()), icon_family);

    ctx.set_fonts(fonts);
    ICONS_AVAILABLE.store(has_icons, std::sync::atomic::Ordering::Relaxed);
    loaded
}

/// Apply the skin: colours, corner radii, spacing, and the type scale.
pub fn apply(ctx: &egui::Context, skin: Skin) {
    let p = skin.palette();
    let mut visuals = if skin.is_dark() {
        Visuals::dark()
    } else {
        Visuals::light()
    };

    visuals.panel_fill = p.canvas;
    visuals.window_fill = p.panel;
    visuals.extreme_bg_color = p.page;
    visuals.faint_bg_color = p.panel;
    visuals.override_text_color = Some(p.ink);
    visuals.hyperlink_color = p.accent;
    visuals.selection.bg_fill = p.highlight.gamma_multiply(0.7);
    visuals.selection.stroke = Stroke::new(1.0, p.ink);
    visuals.window_stroke = Stroke::new(1.0, p.hairline);
    visuals.window_corner_radius = CornerRadius::same(10);
    visuals.popup_shadow.color = p.shadow;
    visuals.window_shadow.color = p.shadow;

    for w in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        w.corner_radius = CornerRadius::same(7);
        w.bg_stroke = Stroke::new(1.0, p.hairline);
        w.fg_stroke = Stroke::new(1.0, p.ink);
    }
    visuals.widgets.noninteractive.bg_fill = p.panel;
    visuals.widgets.inactive.bg_fill = p.panel;
    visuals.widgets.inactive.weak_bg_fill = p.panel;
    visuals.widgets.hovered.bg_fill = blend(p.panel, p.accent, 0.12);
    visuals.widgets.hovered.weak_bg_fill = blend(p.panel, p.accent, 0.12);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, blend(p.hairline, p.accent, 0.5));
    visuals.widgets.active.bg_fill = blend(p.panel, p.accent, 0.2);
    visuals.widgets.active.weak_bg_fill = blend(p.panel, p.accent, 0.2);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, p.accent);
    ctx.set_visuals(visuals);

    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = egui::vec2(10.0, 8.0);
        style.spacing.button_padding = egui::vec2(12.0, 7.0);
        style.spacing.menu_margin = egui::Margin::same(8);
        style.spacing.slider_width = 160.0;
        style.spacing.interact_size.y = 28.0;
        style.text_styles = [
            (egui::TextStyle::Heading, sans(19.0)),
            (egui::TextStyle::Body, sans(14.5)),
            (egui::TextStyle::Button, sans(14.0)),
            (egui::TextStyle::Small, sans(12.0)),
            (egui::TextStyle::Monospace, egui::FontId::monospace(13.0)),
        ]
        .into();
    });
}

/// Highlighter colours: pale on paper, a deep wash at night.
///
/// They live here, beside the palette, because they are held to the same
/// numbers it is. A highlighter is painted *behind* the words, so the words on
/// it have to stay as readable as the words beside them — and the gate had a
/// hole exactly this shape: it checked the search wash (`Palette::highlight`)
/// and never these, the four colours the reader actually marks with. Measured
/// afterwards, the night set ran from 4.65:1 to 7.52:1 against the body ink,
/// which is to say Yellow, Mint and Sky were all below the AAA the rest of the
/// reader is held to. They are darker now; the gate checks all sixteen pairs.
pub fn ink_colour(ink: Ink, dark: bool) -> Color32 {
    match (ink, dark) {
        (Ink::Yellow, false) => Color32::from_rgb(0xFA, 0xE4, 0x9A),
        (Ink::Mint, false) => Color32::from_rgb(0xB8, 0xE6, 0xC4),
        (Ink::Sky, false) => Color32::from_rgb(0xB9, 0xD8, 0xF2),
        (Ink::Rose, false) => Color32::from_rgb(0xF6, 0xC6, 0xCE),
        (Ink::Yellow, true) => Color32::from_rgb(0x4A, 0x3E, 0x14),
        (Ink::Mint, true) => Color32::from_rgb(0x1C, 0x42, 0x30),
        (Ink::Sky, true) => Color32::from_rgb(0x20, 0x3A, 0x58),
        (Ink::Rose, true) => Color32::from_rgb(0x60, 0x2C, 0x3A),
    }
}

/// Mix `b` into `a`. `t` is the amount of `b`, 0..=1.
pub fn blend(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let f = |x: u8, y: u8| (x as f32 * (1.0 - t) + y as f32 * t).round() as u8;
    Color32::from_rgb(f(a.r(), b.r()), f(a.g(), b.g()), f(a.b(), b.b()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_skin_has_readable_contrast() {
        // Rough WCAG relative luminance; body text must stand well clear of the
        // page it sits on, or the "design" is unreadable.
        fn lum(c: Color32) -> f32 {
            let f = |v: u8| {
                let s = v as f32 / 255.0;
                if s <= 0.03928 {
                    s / 12.92
                } else {
                    ((s + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * f(c.r()) + 0.7152 * f(c.g()) + 0.0722 * f(c.b())
        }
        for skin in Skin::ALL {
            let p = skin.palette();
            let (a, b) = (lum(p.ink), lum(p.page));
            let ratio = (a.max(b) + 0.05) / (a.min(b) + 0.05);
            assert!(
                ratio >= 7.0,
                "{}: ink on page is only {ratio:.1}:1",
                skin.name()
            );
        }
    }

    #[test]
    fn the_page_is_distinguishable_from_the_desk_it_sits_on() {
        for skin in Skin::ALL {
            let p = skin.palette();
            let d = |a: Color32, b: Color32| {
                (a.r() as i32 - b.r() as i32).abs()
                    + (a.g() as i32 - b.g() as i32).abs()
                    + (a.b() as i32 - b.b() as i32).abs()
            };
            assert!(
                d(p.page, p.canvas) >= 12 || d(p.page_edge, p.canvas) >= 30,
                "{}: the sheet vanishes into the background",
                skin.name()
            );
        }
    }

    #[test]
    fn accents_are_visible_on_their_page() {
        fn lum(c: Color32) -> f32 {
            (0.2126 * c.r() as f32 + 0.7152 * c.g() as f32 + 0.0722 * c.b() as f32) / 255.0
        }
        for skin in Skin::ALL {
            let p = skin.palette();
            let d = (lum(p.accent) - lum(p.page)).abs();
            assert!(d > 0.15, "{}: accent too close to page", skin.name());
        }
    }

    #[test]
    fn skins_cycle_through_all_four() {
        let mut s = Skin::Paper;
        let mut seen = Vec::new();
        for _ in 0..4 {
            seen.push(s.name());
            s = s.next();
        }
        assert_eq!(s, Skin::Paper);
        assert_eq!(seen.len(), 4);
        assert_eq!(seen.iter().collect::<std::collections::HashSet<_>>().len(), 4);
    }

    #[test]
    fn blend_moves_between_the_two_colours() {
        let a = Color32::from_rgb(0, 0, 0);
        let b = Color32::from_rgb(100, 200, 50);
        assert_eq!(blend(a, b, 0.0), a);
        assert_eq!(blend(a, b, 1.0), b);
        assert_eq!(blend(a, b, 0.5), Color32::from_rgb(50, 100, 25));
        assert_eq!(blend(a, b, 5.0), b, "t must be clamped");
    }
}

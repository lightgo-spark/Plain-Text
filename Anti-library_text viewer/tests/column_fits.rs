//! Does the type stay inside the column it was set for?
//!
//! The typesetter breaks rows against a `measure` closure; the painter draws
//! them with the real font. `painted_width.rs` shows those two agree for a
//! string. This asks the question one level up, on whole documents at the type
//! sizes the reader actually offers — including 64pt, which exists for readers
//! who cannot use 18pt and is therefore the size least likely to have been
//! looked at.
//!
//! Needs a real face, so it says so and stops where the machine has none.

use anti_library::gui::layout::{self, Metrics, RowKind, Setup};
use anti_library::gui::theme;
use anti_library::text::Document;
use std::path::PathBuf;

fn context() -> Option<egui::Context> {
    let ctx = egui::Context::default();
    let faces = theme::install_fonts(&ctx);
    let _ = ctx.run_ui(egui::RawInput::default(), |_| {});
    if faces.is_empty() {
        eprintln!("no faces on this machine; skipping");
        return None;
    }
    Some(ctx)
}

/// What the reader's painter would put on the screen for `text`.
fn painted(ctx: &egui::Context, font: &egui::FontId, text: &str) -> f32 {
    ctx.fonts_mut(|f| {
        f.layout_no_wrap(text.to_string(), font.clone(), egui::Color32::WHITE)
            .rect
            .width()
    })
}

/// What the typesetter measures, character by character, as `ReaderApp::pump`
/// does it.
fn measured(ctx: &egui::Context, font: &egui::FontId, text: &str) -> f32 {
    ctx.fonts_mut(|f| {
        text.chars()
            .map(|c| {
                if anti_library::text::is_combining(c) {
                    0.0
                } else {
                    f.glyph_width(font, c)
                }
            })
            .sum()
    })
}

fn sample() -> Document {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("sample.txt");
    Document::load(&path).expect("sample.txt must be readable")
}

/// The reader's own arithmetic, from `ReaderApp::relayout`.
fn setup_for(font_size: f32, col_width: f32, col_height: f32) -> Setup {
    Setup {
        width: col_width,
        height: col_height,
        metrics: Metrics {
            body_height: font_size * 1.65,
            heading_height: font_size * 1.65 * 1.9,
            blank_height: font_size * 1.65 * 0.5,
            indent: font_size * 1.4,
            drop_cap_rows: 3,
            drop_cap_scale: 2.9,
            drop_cap_gap: font_size * 0.34,
        },
        justify: true,
        drop_caps: true,
        chapter_breaks: true,
        hyphenate: true,
    }
}

#[test]
fn no_row_is_painted_wider_than_the_column_it_was_set_for() {
    let Some(ctx) = context() else { return };
    let doc = sample();
    let mut complaints = Vec::new();

    // The type sizes the slider offers, against the column a small window
    // gives at each of them.
    for font_size in [12.0f32, 18.0, 28.0, 40.0, 52.0, 64.0] {
        for col_width in [140.0f32, 238.0, 420.0, 760.0] {
            let body = theme::serif(font_size);
            let head = theme::serif(font_size * 1.45);
            let bw = |t: &str| measured(&ctx, &body, t);
            let hw = |t: &str| measured(&ctx, &head, t);
            let l = layout::typeset(&doc, &setup_for(font_size, col_width, 900.0), &bw, &hw);

            for r in &l.rows {
                if r.kind == RowKind::Blank || r.text.is_empty() {
                    continue;
                }
                let font = if r.kind == RowKind::Heading {
                    head.clone()
                } else {
                    body.clone()
                };
                // A single grapheme wider than the whole column has nowhere
                // else to go; anything else must fit.
                if r.text.chars().count() <= 1 {
                    continue;
                }
                let ink = painted(&ctx, &font, &r.text) + r.indent;
                if ink > col_width + 0.5 {
                    complaints.push(format!(
                        "{font_size}pt in a {col_width}pt column: {:?} row {:?} paints {:.1}pt \
(indent {:.1})",
                        r.kind, r.text, ink, r.indent
                    ));
                }
            }
        }
    }
    assert!(
        complaints.is_empty(),
        "{} row(s) run past the margin:\n{}",
        complaints.len(),
        complaints
            .iter()
            .take(12)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

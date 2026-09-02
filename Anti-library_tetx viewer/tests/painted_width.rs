//! Does the width the typesetter measures match the width egui paints?
//!
//! The whole layout is built on adding up glyph advances, and the selection,
//! the highlighter and the search wash all read the positions that sum
//! produces. If the painter disagrees, every one of them is drawn beside the
//! text rather than on it — which is not something a test on the layout alone
//! can ever see, because both sides of it would be our own arithmetic.
//!
//! Needs a real face, so it says so and stops where the machine has none.

use anti_library::gui::theme;

fn context() -> Option<(egui::Context, Vec<String>)> {
    let ctx = egui::Context::default();
    let faces = theme::install_fonts(&ctx);
    // egui builds its atlas during a frame; there is nothing to measure before.
    let _ = ctx.run_ui(egui::RawInput::default(), |_| {});
    if faces.is_empty() {
        return None;
    }
    Some((ctx, faces))
}

fn summed(ctx: &egui::Context, font: &egui::FontId, s: &str) -> f32 {
    ctx.fonts_mut(|f| {
        s.chars()
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

fn painted(ctx: &egui::Context, font: &egui::FontId, s: &str) -> f32 {
    ctx.fonts_mut(|f| {
        f.layout_no_wrap(s.to_string(), font.clone(), egui::Color32::WHITE)
            .rect
            .width()
    })
}

#[test]
fn the_measured_width_is_the_painted_width() {
    let Some((ctx, faces)) = context() else {
        eprintln!("no book faces on this machine — nothing to compare against");
        return;
    };
    eprintln!("measuring against {faces:?}");
    for size in [12.0f32, 18.0, 34.0] {
        let font = theme::serif(size);
        for s in [
            "The quick brown fox jumps over the lazy dog",
            "Waterfall AVATAR To Yo LTa VA Ta",
            "office fluffy affluent difficult",
            "읽지 않은 책이 쌓인 서가를 안티라이브러리라 부른다",
            "한글 and latin mixed 함께",
            "AVAWAY",
            "Grüße aus Köln — café, naïve, Straße",
            // Written out rather than composed, so the rule is actually
            // exercised on a machine that has an ordinary latin face. The
            // reader itself composes these when it reads a file; this is the
            // measurement being held to what the painter does with them.
            "e\u{0301}cole and cafe\u{0301}",
            "a\u{0300}a\u{0301}a\u{0302}a\u{0303}",
            // Marks with no composed form. Normalising cannot help these, so
            // the zero-width rule is all that keeps the sum honest.
            "\u{0e01}\u{0e31}\u{0e19}",
            "\u{0939}\u{093f}\u{0928}\u{094d}\u{0926}\u{0940}",
            "\u{05d0}\u{05b8}\u{05d1}",
        ] {
            // A character the loaded faces do not carry is measured at nothing
            // and painted as a replacement box that is very much something.
            // That gap is font coverage, not arithmetic, and the reader shows
            // such a script as boxes either way — so it is skipped here and
            // recorded as a limitation rather than asserted away.
            if !ctx.fonts_mut(|f| s.chars().all(|c| f.has_glyph(&font, c))) {
                eprintln!("  skipped {s:?}: this machine has no face for it");
                continue;
            }
            let (a, b) = (summed(&ctx, &font, s), painted(&ctx, &font, s));
            // Half a point at any size: rounding, not disagreement.
            assert!(
                (a - b).abs() <= 0.5,
                "at {size}pt {s:?}: measured {a:.2}pt, painted {b:.2}pt"
            );
        }
    }
}

#[test]
fn a_document_written_in_decomposed_form_reads_as_ordinary_text() {
    use anti_library::text::Document;
    use std::path::PathBuf;

    // macOS writes Korean this way: the syllable stored as its jamo.
    let nfd = "\u{1112}\u{1161}\u{11ab}\u{1100}\u{1173}\u{11af}";
    let d = Document::from_string(
        format!("{nfd} 문서입니다.\n"),
        &PathBuf::from("t.txt"),
        "UTF-8",
    );
    assert_eq!(d.paragraphs[0].text, "한글 문서입니다.");
    assert_eq!(
        d.search("한글").len(),
        1,
        "a word typed on a Windows keyboard must find the same word from a Mac"
    );
    // And latin the same way.
    let d = Document::from_string(
        "e\u{0301}cole and cafe\u{0301}\n".into(),
        &PathBuf::from("t.txt"),
        "UTF-8",
    );
    assert_eq!(d.paragraphs[0].text, "école and café");
    assert_eq!(d.search("école").len(), 1);
    assert_eq!(d.search("CAFÉ").len(), 1);
}

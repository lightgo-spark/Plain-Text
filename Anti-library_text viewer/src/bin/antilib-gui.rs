//! Anti-library — the desktop reader.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use anti_library::gui::{icon, ReaderApp};
use std::path::PathBuf;

fn main() -> eframe::Result<()> {
    // A windowed build has no console, so a panic would otherwise take the
    // window away and say nothing anywhere the reader can look.
    anti_library::crash::install();

    // Not filtered by `is_file` any more. A file that is named and not there
    // is something the reader has to be told about — dropping it here made a
    // double-clicked document with a broken association look like a program
    // that simply ignored it. Switches are still ignored: this build has no
    // console to answer them on, and the version is on the start screen.
    let book = std::env::args()
        .nth(1)
        .filter(|a| !a.starts_with('-'))
        .map(PathBuf::from);

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1180.0, 820.0])
        .with_min_inner_size([680.0, 520.0])
        .with_title("Anti-library");
    // The window, taskbar and Alt-Tab all read this.
    if let Some(icon) = icon::load() {
        viewport = viewport.with_icon(egui::IconData {
            rgba: icon.rgba,
            width: icon.width,
            height: icon.height,
        });
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "anti-library",
        options,
        Box::new(|cc| Ok(Box::new(ReaderApp::new(&cc.egui_ctx, book)))),
    )
}

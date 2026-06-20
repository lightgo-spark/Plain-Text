fn main() {
    let mut res = winres::WindowsResource::new();

    // ── Icon ──
    res.set_icon("icon.ico");

    // ── Application manifest (security & compatibility) ──
    res.set_manifest_file("app.manifest");

    // ── Version info (Windows file properties) ──
    res.set("FileDescription", "Writing - Text Editor");
    res.set("ProductName", "Writing");
    res.set("CompanyName", "lightgo");
    res.set("LegalCopyright", "\u{00A9} 2026 lightgo. All rights reserved.");
    res.set("OriginalFilename", "simple_notepad.exe");
    res.set("FileVersion", "3.0.0.0");
    res.set("ProductVersion", "3.0.0.0");
    res.set("InternalName", "simple_notepad");
    res.set("Comments", "A lightweight text editor built with Rust and egui");

    res.compile().unwrap();
}

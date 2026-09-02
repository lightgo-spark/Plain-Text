//! Stamps the Windows resources onto the executables: the icon Explorer and
//! the taskbar show, plus the version strings in the file's properties.

fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    println!("cargo:rerun-if-changed=build.rs");

    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico")
            .set("ProductName", "Anti-library")
            .set("FileDescription", "Anti-library — read documents like a book")
            .set("LegalCopyright", "");
        if let Err(e) = res.compile() {
            // A missing resource compiler must not stop the build; the reader
            // just runs without a custom icon.
            println!("cargo:warning=could not embed the icon: {e}");
        }
    }
}

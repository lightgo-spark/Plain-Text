//! The application icon, built into the binary.
//!
//! Windows takes the icon for the taskbar and Explorer from the executable's
//! resources (see `build.rs`); the window itself needs the same picture as raw
//! pixels, which is what this decodes.

/// The icon file is embedded so the reader shows its own picture wherever it
/// is copied to, with no file to lose.
pub const ICON_BYTES: &[u8] = include_bytes!("../../assets/icon.ico");

/// Decoded icon: RGBA pixels plus the size they form.
pub struct Icon {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Decode the embedded icon, preferring the largest image the file holds —
/// the window manager scales down far better than it scales up.
pub fn load() -> Option<Icon> {
    decode(ICON_BYTES)
}

fn decode(bytes: &[u8]) -> Option<Icon> {
    let image = image::load_from_memory_with_format(bytes, image::ImageFormat::Ico).ok()?;
    let rgba = image.to_rgba8();
    let (width, height) = rgba.dimensions();
    Some(Icon {
        rgba: rgba.into_raw(),
        width,
        height,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_embedded_icon_decodes() {
        let icon = load().expect("the icon must decode, it ships inside the binary");
        assert!(icon.width >= 16 && icon.height >= 16);
        assert_eq!(
            icon.rgba.len() as u32,
            icon.width * icon.height * 4,
            "RGBA buffer does not match its dimensions"
        );
    }

    #[test]
    fn the_icon_is_not_blank() {
        // A fully transparent or single-colour icon would mean the wrong file
        // was embedded.
        let icon = load().unwrap();
        let opaque = icon.rgba.chunks_exact(4).filter(|p| p[3] > 8).count();
        assert!(
            opaque * 10 > (icon.width * icon.height) as usize,
            "icon is almost entirely transparent"
        );
        let first = &icon.rgba[..4];
        assert!(
            icon.rgba.chunks_exact(4).any(|p| p != first),
            "icon is a single flat colour"
        );
    }

    #[test]
    fn a_file_that_is_not_an_icon_is_refused() {
        assert!(decode(b"not an icon at all").is_none());
        assert!(decode(&[]).is_none());
    }
}

# Lightnote v3.0

Advanced notepad application built with Rust + egui

## Key Features

- **Multi-Tab Editing** - Edit multiple files simultaneously (Ctrl+T for new tab, middle-click to close)
- **Tab Renaming** - Double-click an Untitled tab or right-click → Rename to change the name
- **Tab Colors** - Right-click → Tab Color to assign one of 8 preset colors (Red, Orange, Yellow, Green, Blue, Purple, Pink, Cyan)
- **Syntax Highlighting** - Auto-detection for various programming languages
- **Line Numbers** - Toggle line number display
- **Find/Replace** - Ctrl+F find, Ctrl+H replace (regex, case-sensitive, whole word matching)
- **Bookmarks** - Per-line bookmark support
- **Multiple Encodings** - UTF-8, UTF-16 LE/BE, Shift-JIS, EUC-KR
- **Line Ending Conversion** - Switch between LF, CRLF, CR
- **Read-Only Mode** - Per-tab read-only toggle
- **Auto Save** - Automatic save every 2 minutes
- **Session Restore** - Restores previous tabs and settings on restart
- **Themes** - Light, Dark, Solarized, Dracula
- **Font Selection** - Malgun Gothic, Consolas, Courier, Segoe UI, Cascadia, D2Coding
- **Word Frequency** - Text word frequency statistics
- **Encoding Tools** - Base64, URL, Hex encode/decode
- **File Info** - File size, creation date, modification date
- **External Change Detection** - Automatic notification when files are modified externally

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| Ctrl+T | New Tab |
| Ctrl+O | Open File |
| Ctrl+S | Save |
| Ctrl+Shift+S | Save As |
| Ctrl+F | Find |
| Ctrl+H | Replace |
| Ctrl+G | Go to Line |
| Ctrl+W | Close Tab |

## Build

```bash
cargo build --release
```

Output: `target/release/notepad_rs.exe` (~8.5MB)

## Requirements

- Windows 10/11
- Rust 1.70+


## License

Copyright 2026 lightgo. All rights reserved.
Contact

lightgo1230@gmail.com
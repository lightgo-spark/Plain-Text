# Markdown Editor

A lightweight, native Markdown editor for Windows built with Rust and egui.  
No Electron, no web runtime — just a single `.exe` under 8MB.

![Rust](https://img.shields.io/badge/Rust-2021-orange)
![Platform](https://img.shields.io/badge/Platform-Windows-blue)
![License](https://img.shields.io/badge/License-Proprietary-red)

## Why?

Most Markdown editors are either Electron-based (300MB+ RAM) or web apps that need a browser.  
I wanted something that opens instantly, doesn't eat memory, and just works offline.

So I wrote one.

## Features

  Editor
- Split view (editor + live preview side by side)
- Editor-only / Preview-only modes
- Line numbers with scroll sync
- Adjustable font size (Ctrl+Plus / Ctrl+Minus, 8~32px)
- Word wrap toggle

  Markdown
- CommonMark rendering with syntax highlighting
- Bold, italic, strikethrough, inline code
- Code blocks, tables, blockquotes, lists
- Links, horizontal rules, headings
- Insert menu for quick Markdown snippets

  Writing Tools
- Find & Replace with case-sensitive option (Ctrl+F / Ctrl+H)
- Live word / character / line count in status bar
- Estimated read time
- Word goal with progress bar
- Document statistics (paragraphs, headings, code blocks, links)

  File I/O
- New / Open / Save / Save As
- Supports `.md`, `.markdown`, `.txt`
- Unsaved changes indicator in title bar

  UI
- Dark / Light theme toggle
- Zen mode for distraction-free writing (F11)
- Menu bar + toolbar + status bar
- Custom window icon

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| Ctrl+N | New file |
| Ctrl+O | Open file |
| Ctrl+S | Save |
| Ctrl+Shift+S | Save As |
| Ctrl+F | Find |
| Ctrl+H | Find & Replace |
| Ctrl+Plus | Zoom in |
| Ctrl+Minus | Zoom out |
| F11 | Zen mode |
| Escape | Close find bar |

## Build

Requires Rust 1.70+ and a C++ build toolchain (MSVC on Windows).

```bash
git clone <repo>
cd markdown_editor_rs
cargo build --release
```

The binary is at `target/release/markdown_editor.exe`.

To set a custom application icon, place an `icon.ico` in the project root before building.

## Project Structure

```
markdown_editor_rs/
  Cargo.toml      # dependencies & build config
  build.rs        # Windows resource compiler (icon embedding)
  icon.ico        # application icon
  src/
    main.rs       # single-file application (~2000 lines)
```

## Dependencies

| Crate | Purpose |
|-------|---------|
| eframe / egui | GUI framework |
| egui_commonmark | Markdown rendering |
| rfd | Native file dialogs |
| image | Icon loading |
| winres | Windows exe icon embedding |

All dependencies are MIT or Apache-2.0 licensed.

## System Requirements

- Windows 10/11
- ~8MB disk space
- ~30MB RAM at runtime

## License

Copyright 2026 lightgo. All rights reserved.

## Contact

lightgo1230@gmail.com

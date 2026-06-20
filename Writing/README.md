# Writing

A lightweight, distraction-free text editor for Windows built with Rust and [egui](https://github.com/emilk/egui).

![Rust](https://img.shields.io/badge/Rust-2021-orange) ![Platform](https://img.shields.io/badge/Platform-Windows-blue) ![License](https://img.shields.io/badge/License-MIT-green)

## Features

### Core
- Multi-tab editing with drag & drop file support
- File formats: TXT, Markdown, HTML export
- Auto-recovery (saves to temp every 30s)
- Recent files list (up to 10)
- Find & Replace (case-insensitive)

### Writing Experience
- **9 Themes**: Cream, Dark, Forest, Ocean, Sepia, Midnight, Solarized, Nord, Lavender
- **7 Fonts**: Malgun Gothic, Segoe UI, Consolas, Arial, Times New Roman, Calibri, Verdana
- **Zen Mode**: Full-screen distraction-free writing (Ctrl+Shift+Enter)
- **Focus Mode**: Line or paragraph highlighting (Ctrl+G)
- **Typewriter Mode**: Cursor stays centered (Ctrl+J)
- Adjustable font size (12-36pt) and line spacing (1.2x-2.0x)
- Line numbers toggle
- Smart typography (em-dash, ellipsis, smart quotes)

### Analysis & Tools
- Statistics panel: words, characters, sentences, paragraphs, pages, reading time
- Document outline (Markdown headings)
- Word frequency analysis (top 50)
- Writing goal tracker with progress bar
- Session tracking (words written, WPM)
- History snapshots (auto every 15s, up to 100)

### Markdown
- Live preview with inline formatting (bold, italic, code, links)
- Headings (H1-H6), blockquotes, lists, code blocks, horizontal rules
- Syntax highlighting for Markdown elements

### AI Assistant
- Local AI via [Ollama](https://ollama.com) (no external API calls)
- Proofread, Summarize, Expand, Rewrite, Translate (KO/EN), Continue Writing
- Custom prompt support
- Streaming response display

## Keyboard Shortcuts

| Shortcut | Action |
|---|---|
| Ctrl+N | New Window |
| Ctrl+Shift+N | New Tab |
| Ctrl+O | Open File |
| Ctrl+S | Save |
| Ctrl+Shift+S | Save As |
| Ctrl+W | Close Tab |
| Ctrl+F | Find |
| Ctrl+H | Find & Replace |
| Ctrl+D | Cycle Theme |
| Ctrl+G | Cycle Focus Mode |
| Ctrl+J | Typewriter Mode |
| Ctrl+Shift+Enter | Zen Mode |
| Ctrl+B | Status Bar |
| Ctrl+I | Statistics Panel |
| Ctrl+U | Document Outline |
| Ctrl+M | Word Frequency |
| Ctrl+K | Writing Goal |
| Ctrl+T | Smart Typography |
| Ctrl+Shift+A | AI Assistant |
| Ctrl+Shift+P | Markdown Preview |
| Ctrl+Shift+G | Syntax Highlighting |
| Ctrl+Shift+B | History Snapshots |
| Ctrl+P | Go to Line |
| Ctrl+E | Export HTML |
| Ctrl+R | Recent Files |
| Ctrl+L | Cycle Line Spacing |
| Ctrl+Shift+L | Line Numbers |
| Ctrl+=/- | Font Size |
| Ctrl+Tab | Next Tab |
| F11 | Fullscreen |

## Build

Requires [Rust](https://rustup.rs/) toolchain.

```bash
cargo build --release
```

## Dependencies

| Crate | Purpose | License |
|---|---|---|
| eframe | GUI framework (egui) | MIT/Apache-2.0 |
| rfd | Native file dialogs | MIT |
| ureq | HTTP client (Ollama) | MIT |
| serde_json | JSON parsing | MIT/Apache-2.0 |
| image | Icon loading | MIT |
| winres | Windows resource embedding | MIT |

## License

MIT License. See [LICENSE](LICENSE) for details.
lightgo1230@gmail.com

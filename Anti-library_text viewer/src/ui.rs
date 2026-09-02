//! Rendering. The reader is drawn as a bound book: a header running title, one
//! or two text columns separated by a spine, and a footer with the progress.

use crate::app::{App, Mode, Theme};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, LineGauge, List, ListItem, ListState, Paragraph};

pub struct Palette {
    pub bg: Color,
    pub fg: Color,
    pub dim: Color,
    pub accent: Color,
    pub highlight_bg: Color,
    pub highlight_fg: Color,
}

pub fn palette(theme: Theme) -> Palette {
    match theme {
        Theme::Paper => Palette {
            bg: Color::Rgb(248, 246, 240),
            fg: Color::Rgb(38, 36, 33),
            dim: Color::Rgb(140, 134, 124),
            accent: Color::Rgb(140, 60, 40),
            highlight_bg: Color::Rgb(246, 220, 130),
            highlight_fg: Color::Rgb(38, 36, 33),
        },
        Theme::Sepia => Palette {
            bg: Color::Rgb(244, 232, 208),
            fg: Color::Rgb(70, 52, 34),
            dim: Color::Rgb(158, 132, 100),
            accent: Color::Rgb(126, 74, 30),
            highlight_bg: Color::Rgb(214, 176, 110),
            highlight_fg: Color::Rgb(40, 28, 16),
        },
        Theme::Night => Palette {
            bg: Color::Rgb(20, 22, 27),
            fg: Color::Rgb(206, 208, 214),
            dim: Color::Rgb(110, 116, 128),
            accent: Color::Rgb(126, 176, 220),
            highlight_bg: Color::Rgb(70, 92, 120),
            highlight_fg: Color::Rgb(240, 244, 250),
        },
        Theme::Ink => Palette {
            bg: Color::Black,
            fg: Color::White,
            dim: Color::DarkGray,
            accent: Color::Cyan,
            highlight_bg: Color::Yellow,
            highlight_fg: Color::Black,
        },
    }
}

pub fn draw(f: &mut Frame, app: &mut App) {
    let p = palette(app.theme);
    let area = f.area();
    f.render_widget(Block::default().style(Style::default().bg(p.bg).fg(p.fg)), area);

    let chunks = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(3),
        Constraint::Length(2),
    ])
    .split(area);

    draw_header(f, app, &p, chunks[0]);
    draw_pages(f, app, &p, chunks[1]);
    draw_footer(f, app, &p, chunks[2]);

    match app.mode {
        Mode::Contents => draw_list(f, app, &p, "Contents", contents_items(app)),
        Mode::Bookmarks => draw_list(f, app, &p, "Bookmarks", bookmark_items(app)),
        Mode::Help => draw_help(f, app, &p),
        Mode::Search => draw_search(f, app, &p),
        Mode::Reading => {}
    }
}

fn draw_header(f: &mut Frame, app: &App, p: &Palette, area: Rect) {
    let chapter = app
        .doc
        .chapters
        .iter()
        .rfind(|c| c.offset <= app.current_offset())
        .map(|c| c.title.clone())
        .unwrap_or_default();
    let left = Span::styled(
        app.doc.title.clone(),
        Style::default().fg(p.fg).add_modifier(Modifier::BOLD),
    );
    let right = Span::styled(chapter, Style::default().fg(p.dim).italic());
    let line = Line::from(vec![left, Span::raw("   "), right]);
    f.render_widget(
        Paragraph::new(line).block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(p.dim)),
        ),
        area,
    );
}

fn draw_pages(f: &mut Frame, app: &mut App, p: &Palette, area: Rect) {
    let inner = area.inner(Margin {
        horizontal: 2,
        vertical: 0,
    });
    let two = app.two_page && inner.width as usize >= crate::app::MIN_SPREAD_WIDTH;
    let cols = if two {
        Layout::horizontal([
            Constraint::Min(1),
            Constraint::Length(crate::app::SPINE_WIDTH as u16),
            Constraint::Min(1),
        ])
        .split(inner)
    } else {
        Layout::horizontal([Constraint::Min(1)]).split(inner)
    };

    // Wrap to the width the layout actually handed us, not to a guess: a line
    // one cell too wide silently loses its last glyph.
    let avail = if two {
        cols[0].width.min(cols[2].width)
    } else {
        cols[0].width
    } as usize;
    app.relayout_cols(if two { 2 } else { 1 }, avail, inner.height as usize);

    let hits = app.visible_matches();
    let rows = app.rows;
    for c in 0..app.cols {
        let start = app.top + c * rows;
        let mut text: Vec<Line> = Vec::new();
        for i in start..start + rows {
            match app.lines.get(i) {
                Some(l) => text.push(render_line(l, hits, p)),
                None => text.push(Line::raw("")),
            }
        }
        let target = if app.cols == 2 && c == 1 { cols[2] } else { cols[0] };
        f.render_widget(Paragraph::new(text), target);
    }

    if app.cols == 2 {
        let spine: Vec<Line> = (0..cols[1].height)
            .map(|_| Line::styled("\u{2502}", Style::default().fg(p.dim)))
            .collect();
        f.render_widget(Paragraph::new(spine).alignment(Alignment::Center), cols[1]);
    }
}

/// Paint one wrapped line, washing the parts of it a search matched.
///
/// The matches arrive as character ranges in the document, so the line only has
/// to work out which part of itself each one covers. It used to search its own
/// text again instead, which meant the paint and the count in the footer were
/// two different searches — and any line whose lower case was a different
/// number of bytes (a line holding `İ`) was quietly left unmarked.
fn render_line<'a>(
    l: &'a anti_library::text::Line,
    hits: &[anti_library::text::Match],
    p: &Palette,
) -> Line<'a> {
    let base = if l.is_heading {
        Style::default().fg(p.accent).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(p.fg)
    };
    let chars: Vec<char> = l.text.chars().collect();
    let (a, b) = (l.offset, l.offset + chars.len());
    let here = anti_library::text::matches_in(hits, a, b);
    if here.is_empty() || chars.is_empty() {
        return Line::styled(l.text.clone(), base);
    }
    let wash = Style::default().bg(p.highlight_bg).fg(p.highlight_fg);
    let mut spans = Vec::new();
    let mut cursor = 0usize; // character index within the line
    for m in here {
        let from = m.start.saturating_sub(a).min(chars.len());
        let to = m.end.saturating_sub(a).min(chars.len());
        if to <= from || from < cursor {
            continue;
        }
        if from > cursor {
            spans.push(Span::styled(
                chars[cursor..from].iter().collect::<String>(),
                base,
            ));
        }
        spans.push(Span::styled(
            chars[from..to].iter().collect::<String>(),
            wash,
        ));
        cursor = to;
    }
    if cursor < chars.len() {
        spans.push(Span::styled(
            chars[cursor..].iter().collect::<String>(),
            base,
        ));
    }
    Line::from(spans)
}

fn draw_footer(f: &mut Frame, app: &App, p: &Palette, area: Rect) {
    let rows = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).split(area);

    let ratio = app.progress();
    let gauge = LineGauge::default()
        .filled_style(Style::default().fg(p.accent))
        .unfilled_style(Style::default().fg(p.dim))
        .ratio(ratio)
        .label(Span::styled(
            format!(
                "{:>3}%  p.{}/{}",
                (ratio * 100.0).round() as u32,
                app.page_number(),
                app.total_pages()
            ),
            Style::default().fg(p.dim),
        ));
    f.render_widget(gauge, rows[0]);

    let text = match &app.status {
        Some(s) => s.clone(),
        None => "←/→ page   ↑/↓ scroll   / search   b bookmark   c contents   ? help   q quit"
            .into(),
    };
    f.render_widget(
        Paragraph::new(Span::styled(text, Style::default().fg(p.dim))),
        rows[1],
    );
}

fn contents_items(app: &App) -> Vec<String> {
    if app.doc.chapters.is_empty() {
        return vec!["(no chapter headings found)".into()];
    }
    app.doc
        .chapters
        .iter()
        .map(|c| {
            let pct = if app.doc.chars == 0 {
                0
            } else {
                c.offset * 100 / app.doc.chars
            };
            format!("{:>3}%  {}", pct, c.title)
        })
        .collect()
}

fn bookmark_items(app: &App) -> Vec<String> {
    let marks = app.bookmarks();
    if marks.is_empty() {
        return vec!["(no bookmarks — press b while reading)".into()];
    }
    marks
        .iter()
        .map(|b| {
            let pct = if app.doc.chars == 0 {
                0
            } else {
                b.offset * 100 / app.doc.chars
            };
            format!("{:>3}%  {}", pct, b.label)
        })
        .collect()
}

fn popup(area: Rect, width_pct: u16, height_pct: u16) -> Rect {
    let v = Layout::vertical([
        Constraint::Percentage((100 - height_pct) / 2),
        Constraint::Percentage(height_pct),
        Constraint::Percentage((100 - height_pct) / 2),
    ])
    .split(area);
    Layout::horizontal([
        Constraint::Percentage((100 - width_pct) / 2),
        Constraint::Percentage(width_pct),
        Constraint::Percentage((100 - width_pct) / 2),
    ])
    .split(v[1])[1]
}

fn draw_list(f: &mut Frame, app: &App, p: &Palette, title: &str, items: Vec<String>) {
    let area = popup(f.area(), 66, 66);
    f.render_widget(Clear, area);
    let list = List::new(
        items
            .into_iter()
            .map(|s| ListItem::new(s).style(Style::default().fg(p.fg)))
            .collect::<Vec<_>>(),
    )
    .block(
        Block::default()
            .title(format!(" {title} "))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(p.accent))
            .style(Style::default().bg(p.bg)),
    )
    .highlight_style(Style::default().bg(p.highlight_bg).fg(p.highlight_fg))
    .highlight_symbol("▸ ");
    let mut state = ListState::default();
    if app.list_len() > 0 {
        state.select(Some(app.list_cursor));
    }
    f.render_stateful_widget(list, area, &mut state);
}

fn draw_search(f: &mut Frame, app: &App, p: &Palette) {
    let area = popup(f.area(), 60, 20);
    let area = Rect {
        height: 3.min(area.height),
        ..area
    };
    f.render_widget(Clear, area);
    f.render_widget(
        Paragraph::new(format!("/{}", app.query)).block(
            Block::default()
                .title(" Search ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(p.accent))
                .style(Style::default().bg(p.bg).fg(p.fg)),
        ),
        area,
    );
}

const HELP: &[(&str, &str)] = &[
    ("→ l Space PgDn", "turn to the next page"),
    // Not `b`. `b` sets a bookmark (three lines down), and this help used to
    // print it here as `b*` — an asterisk it never explained — so a reader who
    // took the help at its word and pressed `b` to go back dropped a bookmark
    // instead. The key that turns back is Backspace.
    ("← h PgUp Bksp", "turn back a page"),
    ("↓ ↑ j k", "scroll a line"),
    ("g / G", "first page / last page"),
    ("0-9 then %", "jump to a percentage"),
    ("/", "search, Enter to run"),
    ("n / N", "next / previous match"),
    ("b", "add or remove a bookmark here"),
    ("m", "open the bookmark list (d deletes)"),
    ("c", "open the table of contents"),
    ("t", "cycle theme: Paper Sepia Night Ink"),
    ("s", "toggle the two-page spread"),
    ("i", "toggle paragraph indent"),
    ("+ / -", "widen or narrow the text column"),
    ("? ", "this help"),
    ("q Esc", "save progress and quit"),
];

fn draw_help(f: &mut Frame, app: &App, p: &Palette) {
    let area = popup(f.area(), 62, 80);
    f.render_widget(Clear, area);
    let mut lines: Vec<Line> = HELP
        .iter()
        .map(|(k, d)| {
            Line::from(vec![
                Span::styled(
                    format!("{:<16}", k),
                    Style::default().fg(p.accent).add_modifier(Modifier::BOLD),
                ),
                Span::styled(d.to_string(), Style::default().fg(p.fg)),
            ])
        })
        .collect();
    lines.push(Line::raw(""));
    lines.push(Line::styled(
        format!(
            "{} · {} words · {} chars · {}",
            app.doc.encoding,
            app.doc.words,
            app.doc.chars,
            app.theme.name()
        ),
        Style::default().fg(p.dim),
    ));
    f.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .title(" Anti-library ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(p.accent))
                .style(Style::default().bg(p.bg)),
        ),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use anti_library::library::Library;
    use anti_library::text::Document;
    use ratatui::backend::TestBackend;
    use unicode_width::UnicodeWidthStr;
    use std::path::PathBuf;

    fn render(app: &mut App, w: u16, h: u16) -> String {
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        term.draw(|f| draw(f, app)).unwrap();
        let buf = term.backend().buffer().clone();
        (0..buf.area.height)
            .map(|y| {
                // A wide glyph occupies two cells; the second one is empty and
                // must be skipped so the text reads back as it was written.
                (0..buf.area.width)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .filter(|s| !s.is_empty())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("
")
    }

    /// The test backend writes a wide glyph as the glyph plus a padding cell,
    /// so a line reads back with a space after every wide character.
    fn spread(text: &str) -> String {
        let mut out = String::new();
        for g in text.chars() {
            out.push(g);
            if UnicodeWidthStr::width(g.to_string().as_str()) > 1 {
                out.push(' ');
            }
        }
        out
    }

    #[test]
    fn no_glyph_is_clipped_at_the_column_edge() {
        let para = "읽지 않은 책이 쌓인 서가를 안티라이브러리라 부른다. \
이미 읽은 책보다 아직 읽지 않은 책이 더 많은 것을 아는 일, 그것이 서재의 쓸모다.";
        for w in [80u16, 100, 120, 121, 140] {
            let mut app = book(&format!("{para}\n"));
            let out = render(&mut app, w, 20);
            for line in app.lines.iter().filter(|l| !l.blank) {
                assert!(
                    out.contains(&spread(line.text.trim())),
                    "line clipped at terminal width {w}: {:?}\n{out}",
                    line.text
                );
            }
        }
    }

    fn book(text: &str) -> App {
        let doc =
            Document::from_string(text.to_string(), &PathBuf::from("Moby.txt"), "UTF-8");
        App::new(doc, Library::default())
    }

    #[test]
    fn draws_title_text_and_progress() {
        let mut app = book("Chapter 1

Call me Ishmael. Some years ago never mind how long.
");
        let out = render(&mut app, 80, 16);
        assert!(out.contains("Moby"), "title missing:
{out}");
        assert!(out.contains("Ishmael"), "body missing:
{out}");
        assert!(out.contains('%'), "progress missing:
{out}");
    }

    #[test]
    fn wide_terminal_shows_a_spine_between_two_pages() {
        let mut app = book(&(0..80).map(|i| format!("line {i}
")).collect::<String>());
        let out = render(&mut app, 140, 20);
        assert_eq!(app.cols, 2);
        assert!(out.contains('│'), "spine missing:
{out}");
    }

    #[test]
    fn overlays_render_without_panicking() {
        let mut app = book("Chapter 1

body text here

Chapter 2

more
");
        for mode in [Mode::Contents, Mode::Bookmarks, Mode::Help, Mode::Search] {
            app.open(mode);
            app.mode = mode;
            let out = render(&mut app, 90, 24);
            assert!(!out.trim().is_empty());
        }
    }

    #[test]
    fn contents_overlay_lists_the_chapters() {
        let mut app = book("Chapter 1

aaa

Chapter 2

bbb
");
        app.open(Mode::Contents);
        let out = render(&mut app, 90, 24);
        assert!(out.contains("Chapter 2"), "toc missing:
{out}");
    }

    #[test]
    fn tiny_terminal_does_not_panic() {
        let mut app = book("hello world
");
        for (w, h) in [(20u16, 6u16), (8, 4), (200, 60)] {
            let _ = render(&mut app, w, h);
        }
    }

    #[test]
    fn highlighting_survives_case_changing_characters() {
        let mut app = book("İstanbul and istanbul appear here
");
        app.query = "istanbul".into();
        app.submit_search();
        let out = render(&mut app, 80, 12);
        assert!(out.contains("stanbul"), "text lost:
{out}");
    }

    /// End to end on the real file shipped with the repo: load it from disk,
    /// page through the whole book, render every page.
    #[test]
    fn renders_the_shipped_sample_from_disk() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("sample.txt");
        let doc = Document::load(&path).expect("sample.txt must be readable");
        assert_eq!(doc.encoding, "UTF-8");
        assert!(doc.chapters.len() >= 3, "chapters: {:?}", doc.chapters.len());
        let mut app = App::new(doc, Library::default());
        let first = render(&mut app, 120, 30);
        // Body text off the shipped file, not just a heading: a reader that
        // draws the chapter title and nothing under it would pass on the title
        // alone. Wide-glyph rendering is covered by
        // `no_glyph_is_clipped_at_the_column_edge`, which builds its own
        // Hangul document rather than depending on what the sample happens to
        // be written in.
        assert!(
            first.contains("not an ornament"),
            "body text missing:
{first}"
        );
        let mut seen = 0;
        while app.top < app.max_top() {
            app.next_page();
            let out = render(&mut app, 120, 30);
            assert!(!out.trim().is_empty());
            seen += 1;
            assert!(seen < 500, "paging did not terminate");
        }
        assert!(app.progress() >= 1.0);
    }

    /// The help used to print `b*` on the line for turning back a page. `b`
    /// sets a bookmark — three lines further down, in this same table — so a
    /// reader who took the help at its word dropped one instead of going back,
    /// and the asterisk that was meant to explain it explained nothing.
    #[test]
    fn the_help_never_offers_a_key_that_does_something_else() {
        // What these keys really do, from `handle_reading` in the binary.
        // Written out by hand on purpose: the point is to be able to disagree
        // with the table when the table is wrong.
        let bound: &[(&str, &str)] = &[
            ("b", "bookmark"),
            ("m", "bookmark"),
            ("c", "contents"),
            ("t", "theme"),
            ("n", "match"),
            ("s", "spread"),
            ("i", "indent"),
        ];
        for (keys, what) in HELP {
            for token in keys.split_whitespace() {
                let token = token.trim_end_matches('*');
                if let Some((_, does)) = bound.iter().find(|(k, _)| *k == token) {
                    assert!(
                        what.contains(does),
                        "the help offers `{token}` to {what}, but it works the {does}"
                    );
                }
            }
        }
    }

    #[test]
    fn every_theme_renders() {
        let mut app = book("some text
");
        for _ in 0..4 {
            let _ = render(&mut app, 80, 12);
            app.theme = app.theme.next();
        }
    }
}

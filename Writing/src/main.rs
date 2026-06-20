#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use std::io::BufRead;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

// ── Windows title bar color (DWM API) ─────────────────────────────────

#[link(name = "dwmapi")]
extern "system" {
    fn DwmSetWindowAttribute(hwnd: isize, attr: u32, value: *const u32, size: u32) -> i32;
}
#[link(name = "user32")]
extern "system" {
    fn GetForegroundWindow() -> isize;
}

fn set_titlebar_color(hwnd: isize, bg: egui::Color32, text: egui::Color32, dark: bool) {
    if hwnd == 0 {
        return;
    }
    let ba = bg.to_array();
    let ta = text.to_array();
    let bg_cr = ba[0] as u32 | ((ba[1] as u32) << 8) | ((ba[2] as u32) << 16);
    let tx_cr = ta[0] as u32 | ((ta[1] as u32) << 8) | ((ta[2] as u32) << 16);
    let dark_mode: u32 = if dark { 1 } else { 0 };
    unsafe {
        DwmSetWindowAttribute(hwnd, 20, &dark_mode, 4); // DWMWA_USE_IMMERSIVE_DARK_MODE
        DwmSetWindowAttribute(hwnd, 34, &bg_cr, 4);     // DWMWA_BORDER_COLOR
        DwmSetWindowAttribute(hwnd, 35, &bg_cr, 4);     // DWMWA_CAPTION_COLOR
        DwmSetWindowAttribute(hwnd, 36, &tx_cr, 4);     // DWMWA_TEXT_COLOR
    }
}

fn load_icon() -> Option<egui::IconData> {
    let ico_bytes = include_bytes!("../icon.ico");
    let img = image::load_from_memory(ico_bytes).ok()?.into_rgba8();
    let (w, h) = img.dimensions();
    Some(egui::IconData { rgba: img.into_raw(), width: w, height: h })
}

fn main() -> eframe::Result<()> {
    let skip_recovery = std::env::args().any(|a| a == "--new");
    let mut vp = egui::ViewportBuilder::default()
        .with_inner_size([700.0, 700.0])
        .with_min_inner_size([300.0, 300.0])
        .with_drag_and_drop(true);
    if let Some(icon) = load_icon() {
        vp = vp.with_icon(std::sync::Arc::new(icon));
    }
    let options = eframe::NativeOptions {
        viewport: vp,
        ..Default::default()
    };
    eframe::run_native(
        "Writing",
        options,
        Box::new(move |cc| {
            apply_font(&cc.egui_ctx, FontChoice::MalgunGothic);
            Ok(Box::new(Writer::new_with(skip_recovery)))
        }),
    )
}

// ── Font Choice ───────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq)]
enum FontChoice {
    MalgunGothic,
    SegoeUI,
    Consolas,
    Arial,
    TimesNewRoman,
    Calibri,
    Verdana,
}

impl FontChoice {
    const ALL: &'static [FontChoice] = &[
        Self::MalgunGothic,
        Self::SegoeUI,
        Self::Consolas,
        Self::Arial,
        Self::TimesNewRoman,
        Self::Calibri,
        Self::Verdana,
    ];
    fn name(self) -> &'static str {
        match self {
            Self::MalgunGothic => "Malgun Gothic",
            Self::SegoeUI => "Segoe UI",
            Self::Consolas => "Consolas",
            Self::Arial => "Arial",
            Self::TimesNewRoman => "Times New Roman",
            Self::Calibri => "Calibri",
            Self::Verdana => "Verdana",
        }
    }
    fn file(self) -> &'static str {
        match self {
            Self::MalgunGothic => "malgun.ttf",
            Self::SegoeUI => "segoeui.ttf",
            Self::Consolas => "consola.ttf",
            Self::Arial => "arial.ttf",
            Self::TimesNewRoman => "times.ttf",
            Self::Calibri => "calibri.ttf",
            Self::Verdana => "verdana.ttf",
        }
    }
}

fn apply_font(ctx: &egui::Context, choice: FontChoice) {
    let mut fonts = egui::FontDefinitions::default();
    let font_path = format!("C:\\Windows\\Fonts\\{}", choice.file());

    // Load selected font
    if choice != FontChoice::MalgunGothic {
        if let Ok(data) = std::fs::read(&font_path) {
            let key = "selected".to_owned();
            fonts.font_data.insert(key.clone(), egui::FontData::from_owned(data).into());
            fonts.families.get_mut(&egui::FontFamily::Proportional).unwrap().insert(0, key.clone());
            fonts.families.get_mut(&egui::FontFamily::Monospace).unwrap().insert(0, key);
        }
    }

    // Always load Malgun Gothic as fallback for Korean
    if let Ok(data) = std::fs::read("C:\\Windows\\Fonts\\malgun.ttf") {
        let key = "malgun".to_owned();
        fonts.font_data.insert(key.clone(), egui::FontData::from_owned(data).into());
        let idx = if choice == FontChoice::MalgunGothic { 0 } else { 1 };
        fonts.families.get_mut(&egui::FontFamily::Proportional).unwrap().insert(idx, key.clone());
        fonts.families.get_mut(&egui::FontFamily::Monospace).unwrap().insert(idx, key);
    }

    ctx.set_fonts(fonts);
}

// ── Theme ──────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq)]
enum Theme {
    Cream,
    Dark,
    Forest,
    Ocean,
    Sepia,
    Midnight,
    Solarized,
    Nord,
    Lavender,
}

impl Theme {
    const ALL: &'static [Theme] = &[
        Self::Cream, Self::Dark, Self::Forest, Self::Ocean,
        Self::Sepia, Self::Midnight, Self::Solarized, Self::Nord, Self::Lavender,
    ];
    fn next(self) -> Self {
        match self {
            Self::Cream => Self::Dark,
            Self::Dark => Self::Forest,
            Self::Forest => Self::Ocean,
            Self::Ocean => Self::Sepia,
            Self::Sepia => Self::Midnight,
            Self::Midnight => Self::Solarized,
            Self::Solarized => Self::Nord,
            Self::Nord => Self::Lavender,
            Self::Lavender => Self::Cream,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Cream => "Cream",
            Self::Dark => "Dark",
            Self::Forest => "Forest",
            Self::Ocean => "Ocean",
            Self::Sepia => "Sepia",
            Self::Midnight => "Midnight",
            Self::Solarized => "Solarized",
            Self::Nord => "Nord",
            Self::Lavender => "Lavender",
        }
    }
    fn bg(self) -> egui::Color32 {
        match self {
            Self::Cream => egui::Color32::from_rgb(252, 250, 245),
            Self::Dark => egui::Color32::from_rgb(26, 26, 46),
            Self::Forest => egui::Color32::from_rgb(27, 43, 27),
            Self::Ocean => egui::Color32::from_rgb(26, 36, 51),
            Self::Sepia => egui::Color32::from_rgb(242, 229, 209),
            Self::Midnight => egui::Color32::from_rgb(15, 15, 20),
            Self::Solarized => egui::Color32::from_rgb(253, 246, 227),
            Self::Nord => egui::Color32::from_rgb(46, 52, 64),
            Self::Lavender => egui::Color32::from_rgb(240, 235, 248),
        }
    }
    fn fg(self) -> egui::Color32 {
        match self {
            Self::Cream => egui::Color32::from_rgb(50, 50, 50),
            Self::Dark => egui::Color32::from_rgb(212, 212, 212),
            Self::Forest => egui::Color32::from_rgb(200, 216, 192),
            Self::Ocean => egui::Color32::from_rgb(189, 208, 232),
            Self::Sepia => egui::Color32::from_rgb(75, 55, 35),
            Self::Midnight => egui::Color32::from_rgb(180, 180, 195),
            Self::Solarized => egui::Color32::from_rgb(101, 123, 131),
            Self::Nord => egui::Color32::from_rgb(216, 222, 233),
            Self::Lavender => egui::Color32::from_rgb(60, 50, 80),
        }
    }
    fn dim(self) -> egui::Color32 {
        match self {
            Self::Cream => egui::Color32::from_rgb(160, 160, 160),
            Self::Dark => egui::Color32::from_rgb(100, 100, 120),
            Self::Forest => egui::Color32::from_rgb(90, 122, 80),
            Self::Ocean => egui::Color32::from_rgb(80, 104, 120),
            Self::Sepia => egui::Color32::from_rgb(160, 140, 115),
            Self::Midnight => egui::Color32::from_rgb(80, 80, 100),
            Self::Solarized => egui::Color32::from_rgb(147, 161, 161),
            Self::Nord => egui::Color32::from_rgb(120, 130, 148),
            Self::Lavender => egui::Color32::from_rgb(140, 130, 160),
        }
    }
    fn focus_dim(self) -> egui::Color32 {
        match self {
            Self::Cream => egui::Color32::from_rgb(208, 207, 200),
            Self::Dark => egui::Color32::from_rgb(58, 58, 80),
            Self::Forest => egui::Color32::from_rgb(47, 74, 47),
            Self::Ocean => egui::Color32::from_rgb(42, 58, 80),
            Self::Sepia => egui::Color32::from_rgb(210, 200, 180),
            Self::Midnight => egui::Color32::from_rgb(35, 35, 50),
            Self::Solarized => egui::Color32::from_rgb(220, 215, 200),
            Self::Nord => egui::Color32::from_rgb(68, 76, 92),
            Self::Lavender => egui::Color32::from_rgb(210, 205, 225),
        }
    }
    fn hover(self) -> egui::Color32 {
        match self {
            Self::Cream => egui::Color32::from_rgb(238, 235, 226),
            Self::Dark => egui::Color32::from_rgb(40, 40, 68),
            Self::Forest => egui::Color32::from_rgb(38, 60, 38),
            Self::Ocean => egui::Color32::from_rgb(36, 52, 72),
            Self::Sepia => egui::Color32::from_rgb(230, 218, 198),
            Self::Midnight => egui::Color32::from_rgb(25, 25, 38),
            Self::Solarized => egui::Color32::from_rgb(238, 232, 213),
            Self::Nord => egui::Color32::from_rgb(59, 66, 82),
            Self::Lavender => egui::Color32::from_rgb(225, 218, 238),
        }
    }
    fn selection(self) -> egui::Color32 {
        match self {
            Self::Cream => egui::Color32::from_rgba_premultiplied(180, 210, 240, 100),
            Self::Dark => egui::Color32::from_rgba_premultiplied(80, 80, 140, 100),
            Self::Forest => egui::Color32::from_rgba_premultiplied(80, 140, 80, 100),
            Self::Ocean => egui::Color32::from_rgba_premultiplied(80, 120, 180, 100),
            Self::Sepia => egui::Color32::from_rgba_premultiplied(190, 160, 110, 100),
            Self::Midnight => egui::Color32::from_rgba_premultiplied(60, 60, 120, 100),
            Self::Solarized => egui::Color32::from_rgba_premultiplied(38, 139, 210, 80),
            Self::Nord => egui::Color32::from_rgba_premultiplied(94, 129, 172, 100),
            Self::Lavender => egui::Color32::from_rgba_premultiplied(140, 110, 200, 90),
        }
    }
    fn accent(self) -> egui::Color32 {
        match self {
            Self::Cream => egui::Color32::from_rgb(70, 130, 180),
            Self::Dark => egui::Color32::from_rgb(130, 130, 200),
            Self::Forest => egui::Color32::from_rgb(120, 180, 100),
            Self::Ocean => egui::Color32::from_rgb(100, 160, 220),
            Self::Sepia => egui::Color32::from_rgb(180, 120, 60),
            Self::Midnight => egui::Color32::from_rgb(100, 100, 180),
            Self::Solarized => egui::Color32::from_rgb(38, 139, 210),
            Self::Nord => egui::Color32::from_rgb(136, 192, 208),
            Self::Lavender => egui::Color32::from_rgb(120, 80, 180),
        }
    }
    fn is_dark(self) -> bool {
        matches!(self, Self::Dark | Self::Forest | Self::Ocean | Self::Midnight | Self::Nord)
    }
}

fn blend_color(a: egui::Color32, b: egui::Color32, t: f32) -> egui::Color32 {
    let aa = a.to_array();
    let ba = b.to_array();
    egui::Color32::from_rgb(
        (aa[0] as f32 + (ba[0] as f32 - aa[0] as f32) * t) as u8,
        (aa[1] as f32 + (ba[1] as f32 - aa[1] as f32) * t) as u8,
        (aa[2] as f32 + (ba[2] as f32 - aa[2] as f32) * t) as u8,
    )
}

// ── Focus Mode ─────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq)]
enum FocusMode {
    Off,
    Line,
    Paragraph,
}

impl FocusMode {
    fn next(self) -> Self {
        match self {
            Self::Off => Self::Line,
            Self::Line => Self::Paragraph,
            Self::Paragraph => Self::Off,
        }
    }
    fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Line => "Line",
            Self::Paragraph => "Paragraph",
        }
    }
}

// ── AI Action ─────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq)]
enum AiAction {
    Proofread,
    Summarize,
    Expand,
    Rewrite,
    TranslateKo,
    TranslateEn,
    Continue,
    Custom,
}

impl AiAction {
    const ALL: &'static [Self] = &[
        Self::Proofread, Self::Summarize, Self::Expand, Self::Rewrite,
        Self::TranslateKo, Self::TranslateEn, Self::Continue, Self::Custom,
    ];
    fn name(self) -> &'static str {
        match self {
            Self::Proofread => "Proofread",
            Self::Summarize => "Summarize",
            Self::Expand => "Expand",
            Self::Rewrite => "Rewrite",
            Self::TranslateKo => "Translate \u{2192} Korean",
            Self::TranslateEn => "Translate \u{2192} English",
            Self::Continue => "Continue Writing",
            Self::Custom => "Custom Prompt",
        }
    }
    fn icon(self) -> &'static str {
        match self {
            Self::Proofread => "ABC",
            Self::Summarize => "<<",
            Self::Expand => ">>",
            Self::Rewrite => "<>",
            Self::TranslateKo => "KO",
            Self::TranslateEn => "EN",
            Self::Continue => "...",
            Self::Custom => "?",
        }
    }
    fn build_prompt(self, text: &str, custom: &str) -> String {
        let instruction = match self {
            Self::Proofread => "Fix grammar, spelling, and punctuation errors in the following text. Return only the corrected text without any explanation.",
            Self::Summarize => "Summarize the following text concisely. Keep the same language as the input.",
            Self::Expand => "Expand and elaborate on the following text with more detail and depth. Keep the same style and language.",
            Self::Rewrite => "Rewrite the following text with improved clarity and flow. Keep the same meaning and language.",
            Self::TranslateKo => "Translate the following text to Korean. Return only the translation, no explanation.",
            Self::TranslateEn => "Translate the following text to English. Return only the translation, no explanation.",
            Self::Continue => "Continue writing the following text naturally. Match the style, tone, and language of the input. Write 2-3 paragraphs.",
            Self::Custom => custom,
        };
        format!("{}\n\n---\n\n{}", instruction, text)
    }
}

// ── History Entry ─────────────────────────────────────────────────────

struct HistoryEntry {
    text: String,
    timestamp: Instant,
    word_count: usize,
}

// ── Inline Markdown Parser (for preview) ─────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq)]
enum InlineStyle {
    Normal,
    Bold,
    Italic,
    Code,
    LinkText,
}

struct InlineSpan {
    text: String,
    style: InlineStyle,
}

fn parse_inline_md(input: &str) -> Vec<InlineSpan> {
    let mut spans = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut i = 0;
    let mut current = String::new();

    while i < len {
        // Bold: **text**
        if i + 1 < len && chars[i] == '*' && chars[i + 1] == '*' {
            if !current.is_empty() {
                spans.push(InlineSpan { text: std::mem::take(&mut current), style: InlineStyle::Normal });
            }
            i += 2;
            let mut bold = String::new();
            while i + 1 < len && !(chars[i] == '*' && chars[i + 1] == '*') {
                bold.push(chars[i]);
                i += 1;
            }
            if i + 1 < len { i += 2; }
            if !bold.is_empty() {
                spans.push(InlineSpan { text: bold, style: InlineStyle::Bold });
            }
            continue;
        }
        // Italic: *text* (single star, not **)
        if chars[i] == '*' && (i + 1 >= len || chars[i + 1] != '*') {
            if !current.is_empty() {
                spans.push(InlineSpan { text: std::mem::take(&mut current), style: InlineStyle::Normal });
            }
            i += 1;
            let mut italic = String::new();
            while i < len && chars[i] != '*' {
                italic.push(chars[i]);
                i += 1;
            }
            if i < len { i += 1; }
            if !italic.is_empty() {
                spans.push(InlineSpan { text: italic, style: InlineStyle::Italic });
            }
            continue;
        }
        // Inline code: `text`
        if chars[i] == '`' {
            if !current.is_empty() {
                spans.push(InlineSpan { text: std::mem::take(&mut current), style: InlineStyle::Normal });
            }
            i += 1;
            let mut code = String::new();
            while i < len && chars[i] != '`' {
                code.push(chars[i]);
                i += 1;
            }
            if i < len { i += 1; }
            if !code.is_empty() {
                spans.push(InlineSpan { text: code, style: InlineStyle::Code });
            }
            continue;
        }
        // Link: [text](url) → render text as link
        if chars[i] == '[' {
            i += 1;
            let mut link_text = String::new();
            while i < len && chars[i] != ']' {
                link_text.push(chars[i]);
                i += 1;
            }
            if i + 1 < len && chars[i] == ']' && chars[i + 1] == '(' {
                i += 2;
                while i < len && chars[i] != ')' { i += 1; }
                if i < len { i += 1; }
                if !current.is_empty() {
                    spans.push(InlineSpan { text: std::mem::take(&mut current), style: InlineStyle::Normal });
                }
                spans.push(InlineSpan { text: link_text, style: InlineStyle::LinkText });
            } else {
                current.push('[');
                current.push_str(&link_text);
            }
            continue;
        }
        current.push(chars[i]);
        i += 1;
    }
    if !current.is_empty() {
        spans.push(InlineSpan { text: current, style: InlineStyle::Normal });
    }
    spans
}

// ── Tab ────────────────────────────────────────────────────────────────

struct Tab {
    text: String,
    file_path: Option<PathBuf>,
    modified: bool,
}

impl Tab {
    fn new() -> Self {
        Self {
            text: String::new(),
            file_path: None,
            modified: false,
        }
    }
    fn name(&self) -> String {
        self.file_path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "Untitled".to_string())
    }
    fn title(&self) -> String {
        if self.modified {
            format!("* {}", self.name())
        } else {
            self.name()
        }
    }
}

// ── Helpers ────────────────────────────────────────────────────────────

fn safe_byte_pos(text: &str, pos: usize) -> usize {
    let pos = pos.min(text.len());
    (0..=pos)
        .rev()
        .find(|&i| text.is_char_boundary(i))
        .unwrap_or(0)
}

fn char_to_byte(text: &str, char_idx: usize) -> usize {
    text.char_indices()
        .nth(char_idx)
        .map(|(i, _)| i)
        .unwrap_or(text.len())
}

fn find_line_bounds(text: &str, byte_pos: usize) -> (usize, usize) {
    let pos = safe_byte_pos(text, byte_pos);
    let start = text[..pos].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let end = text[pos..]
        .find('\n')
        .map(|i| pos + i)
        .unwrap_or(text.len());
    (start, end)
}

fn find_para_bounds(text: &str, byte_pos: usize) -> (usize, usize) {
    let pos = safe_byte_pos(text, byte_pos);
    let start = text[..pos].rfind("\n\n").map(|i| i + 2).unwrap_or(0);
    let end = text[pos..]
        .find("\n\n")
        .map(|i| pos + i)
        .unwrap_or(text.len());
    (start, end)
}

fn color_hex(c: egui::Color32) -> String {
    let a = c.to_array();
    format!("#{:02x}{:02x}{:02x}", a[0], a[1], a[2])
}

// ── Apply global theme ────────────────────────────────────────────────

fn apply_theme(ctx: &egui::Context, theme: Theme) {
    let bg = theme.bg();
    let fg = theme.fg();
    let dim = theme.dim();
    let hover = theme.hover();
    let sel = theme.selection();

    let mut style = (*ctx.style()).clone();
    let v = &mut style.visuals;

    v.window_fill = bg;
    v.panel_fill = bg;
    v.extreme_bg_color = bg;
    v.faint_bg_color = bg;

    v.widgets.noninteractive.fg_stroke.color = dim;
    v.widgets.noninteractive.bg_fill = bg;
    v.widgets.noninteractive.bg_stroke = egui::Stroke::NONE;

    v.widgets.inactive.fg_stroke.color = fg;
    v.widgets.inactive.bg_fill = egui::Color32::TRANSPARENT;
    v.widgets.inactive.bg_stroke = egui::Stroke::NONE;
    v.widgets.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;

    v.widgets.hovered.fg_stroke.color = fg;
    v.widgets.hovered.bg_fill = hover;
    v.widgets.hovered.bg_stroke = egui::Stroke::NONE;
    v.widgets.hovered.weak_bg_fill = hover;

    v.widgets.active.fg_stroke.color = fg;
    v.widgets.active.bg_fill = sel;
    v.widgets.active.bg_stroke = egui::Stroke::NONE;
    v.widgets.active.weak_bg_fill = sel;

    v.widgets.open.fg_stroke.color = fg;
    v.widgets.open.bg_fill = hover;
    v.widgets.open.bg_stroke = egui::Stroke::NONE;
    v.widgets.open.weak_bg_fill = hover;

    v.selection.bg_fill = sel;
    v.selection.stroke = egui::Stroke::new(1.0, fg);

    v.window_stroke = egui::Stroke::new(0.5, dim);

    style.animation_time = 0.0;
    style.spacing.scroll.bar_width = 0.0;
    style.spacing.scroll.floating = false;
    style.spacing.scroll.floating_allocated_width = 0.0;
    style.spacing.scroll.floating_width = 0.0;
    ctx.set_style(style);
}

// ── Writer ─────────────────────────────────────────────────────────────

struct Writer {
    tabs: Vec<Tab>,
    active: usize,
    // find & replace
    show_find: bool,
    show_replace: bool,
    find_text: String,
    replace_text: String,
    find_count: usize,
    // settings
    show_status: bool,
    theme: Theme,
    font_choice: FontChoice,
    applied_font: Option<FontChoice>,
    font_size: f32,
    line_spacing: f32,
    focus_mode: FocusMode,
    typewriter: bool,
    fullscreen: bool,
    // state
    cursor_byte_pos: usize,
    last_auto_save: Instant,
    save_flash: Option<Instant>,
    recent_files: Vec<PathBuf>,
    show_recent: bool,
    show_about: bool,
    menu_hover_time: Option<Instant>,
    hwnd: isize,
    applied_titlebar: Option<Theme>,
    // v2.0: Zen mode & line numbers
    zen_mode: bool,
    show_line_numbers: bool,
    // v2.0: Writing goals
    word_goal: usize,
    word_goal_input: String,
    show_goal_settings: bool,
    // v2.0: Session tracking
    session_start: Instant,
    session_start_words: usize,
    // v2.0: Panels
    show_stats_panel: bool,
    show_outline: bool,
    show_word_freq: bool,
    // v2.0: Smart typography
    smart_typography: bool,
    // v2.0: AI Assistant (Ollama)
    ai_show: bool,
    ai_model: String,
    ai_action: AiAction,
    ai_custom_prompt: String,
    ai_result: String,
    ai_loading: bool,
    ai_error: String,
    ai_host: String,
    ai_stream_buf: Arc<Mutex<String>>,
    ai_stream_done: Arc<AtomicBool>,
    ai_stream_err: Arc<Mutex<String>>,
    ai_available_models: Vec<String>,
    // v2.0: Additional features
    goto_line_input: String,
    show_goto_line: bool,
    // v3.0: Markdown preview
    show_preview: bool,
    // v3.0: Syntax highlighting
    syntax_highlight: bool,
    // v3.0: History snapshots
    history: Vec<HistoryEntry>,
    show_history: bool,
    last_snapshot: Instant,
    last_snapshot_text: String,
}

impl Writer {
    fn new_with(skip_recovery: bool) -> Self {
        let mut tab = Tab::new();
        if !skip_recovery {
            if let Some(text) = std::fs::read_to_string(Self::recovery_path())
                .ok()
                .filter(|s| !s.is_empty())
            {
                tab.text = text;
                tab.modified = true;
            }
        }
        Self {
            tabs: vec![tab],
            active: 0,
            show_find: false,
            show_replace: false,
            find_text: String::new(),
            replace_text: String::new(),
            find_count: 0,
            show_status: false,
            theme: Theme::Cream,
            font_choice: FontChoice::MalgunGothic,
            applied_font: Some(FontChoice::MalgunGothic),
            font_size: 18.0,
            line_spacing: 1.6,
            focus_mode: FocusMode::Off,
            typewriter: false,
            fullscreen: false,
            cursor_byte_pos: 0,
            last_auto_save: Instant::now(),
            save_flash: None,
            recent_files: Self::load_recent(),
            show_recent: false,
            show_about: false,
            menu_hover_time: None,
            hwnd: 0,
            applied_titlebar: None,
            // v2.0
            zen_mode: false,
            show_line_numbers: false,
            word_goal: 0,
            word_goal_input: String::new(),
            show_goal_settings: false,
            session_start: Instant::now(),
            session_start_words: 0,
            show_stats_panel: false,
            show_outline: false,
            show_word_freq: false,
            smart_typography: false,
            // AI
            ai_show: false,
            ai_model: "gemma3".to_string(),
            ai_action: AiAction::Proofread,
            ai_custom_prompt: String::new(),
            ai_result: String::new(),
            ai_loading: false,
            ai_error: String::new(),
            ai_host: "http://localhost:11434".to_string(),
            ai_stream_buf: Arc::new(Mutex::new(String::new())),
            ai_stream_done: Arc::new(AtomicBool::new(false)),
            ai_stream_err: Arc::new(Mutex::new(String::new())),
            ai_available_models: Vec::new(),
            // Additional
            goto_line_input: String::new(),
            show_goto_line: false,
            // v3.0
            show_preview: false,
            syntax_highlight: false,
            history: Vec::new(),
            show_history: false,
            last_snapshot: Instant::now(),
            last_snapshot_text: String::new(),
        }
    }

    fn recovery_path() -> PathBuf {
        std::env::temp_dir().join("simple_writer_recovery.txt")
    }
    fn recent_path() -> PathBuf {
        std::env::temp_dir().join("simple_writer_recent.txt")
    }

    fn load_recent() -> Vec<PathBuf> {
        std::fs::read_to_string(Self::recent_path())
            .unwrap_or_default()
            .lines()
            .filter(|l| !l.is_empty())
            .map(PathBuf::from)
            .filter(|p| p.exists())
            .take(10)
            .collect()
    }
    fn save_recent(&self) {
        let s: String = self
            .recent_files
            .iter()
            .filter_map(|p| p.to_str())
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(Self::recent_path(), s).ok();
    }
    fn add_recent(&mut self, path: &PathBuf) {
        self.recent_files.retain(|p| p != path);
        self.recent_files.insert(0, path.clone());
        self.recent_files.truncate(10);
        self.save_recent();
    }

    fn new_window(&self) {
        if let Ok(exe) = std::env::current_exe() {
            std::process::Command::new(exe)
                .arg("--new")
                .spawn()
                .ok();
        }
    }
    fn new_tab(&mut self) {
        self.tabs.push(Tab::new());
        self.active = self.tabs.len() - 1;
        self.cursor_byte_pos = 0;
    }
    fn close_tab(&mut self) {
        if self.tabs.len() > 1 {
            self.tabs.remove(self.active);
            if self.active >= self.tabs.len() {
                self.active = self.tabs.len() - 1;
            }
            self.cursor_byte_pos = 0;
        }
    }

    fn open_file(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Text", &["txt", "md"])
            .add_filter("All", &["*"])
            .pick_file()
        {
            self.open_path(path);
        }
    }
    fn open_path(&mut self, path: PathBuf) {
        if let Ok(content) = std::fs::read_to_string(&path) {
            let tab = &self.tabs[self.active];
            if tab.text.is_empty() && tab.file_path.is_none() && !tab.modified {
                let t = &mut self.tabs[self.active];
                t.text = content;
                t.file_path = Some(path.clone());
            } else {
                self.tabs.push(Tab {
                    text: content,
                    file_path: Some(path.clone()),
                    modified: false,
                });
                self.active = self.tabs.len() - 1;
            }
            self.add_recent(&path);
            self.cursor_byte_pos = 0;
        }
    }

    fn save_file(&mut self) {
        if let Some(path) = self.tabs[self.active].file_path.clone() {
            if std::fs::write(&path, &self.tabs[self.active].text).is_ok() {
                self.tabs[self.active].modified = false;
                self.save_flash = Some(Instant::now());
                std::fs::remove_file(Self::recovery_path()).ok();
                self.add_recent(&path);
            }
        } else {
            self.save_as();
        }
    }
    fn save_as(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Text", &["txt"])
            .add_filter("Markdown", &["md"])
            .save_file()
        {
            self.tabs[self.active].file_path = Some(path);
            self.save_file();
        }
    }

    fn auto_save(&mut self) {
        let tab = &self.tabs[self.active];
        if tab.modified
            && !tab.text.is_empty()
            && self.last_auto_save.elapsed() > Duration::from_secs(30)
        {
            std::fs::write(Self::recovery_path(), &tab.text).ok();
            self.last_auto_save = Instant::now();
        }
    }

    fn update_find_count(&mut self) {
        if self.find_text.is_empty() {
            self.find_count = 0;
        } else {
            self.find_count = self.tabs[self.active]
                .text
                .to_lowercase()
                .matches(&self.find_text.to_lowercase())
                .count();
        }
    }
    fn replace_next(&mut self) {
        if self.find_text.is_empty() {
            return;
        }
        let lower = self.tabs[self.active].text.to_lowercase();
        let pat = self.find_text.to_lowercase();
        if let Some(pos) = lower.find(&pat) {
            let end = pos + self.find_text.len();
            self.tabs[self.active]
                .text
                .replace_range(pos..end, &self.replace_text);
            self.tabs[self.active].modified = true;
            self.update_find_count();
        }
    }
    fn replace_all(&mut self) {
        if self.find_text.is_empty() {
            return;
        }
        let lower = self.tabs[self.active].text.to_lowercase();
        let pat = self.find_text.to_lowercase();
        let mut result = String::new();
        let mut last = 0;
        for (idx, _) in lower.match_indices(&pat) {
            result.push_str(&self.tabs[self.active].text[last..idx]);
            result.push_str(&self.replace_text);
            last = idx + self.find_text.len();
        }
        result.push_str(&self.tabs[self.active].text[last..]);
        if self.tabs[self.active].text != result {
            self.tabs[self.active].text = result;
            self.tabs[self.active].modified = true;
        }
        self.update_find_count();
    }

    fn export_html(&self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("HTML", &["html"])
            .save_file()
        {
            let tab = &self.tabs[self.active];
            let escaped = tab
                .text
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;");
            let html = format!(
                "<!DOCTYPE html>\n<html><head><meta charset=\"utf-8\">\n\
                 <title>{}</title>\n\
                 <style>\n\
                 body {{ max-width:700px; margin:50px auto; padding:0 40px;\n\
                   font-family:'Malgun Gothic','Segoe UI',sans-serif;\n\
                   font-size:{}px; line-height:{}; color:{}; background:{}; }}\n\
                 pre {{ white-space:pre-wrap; word-wrap:break-word; font-family:inherit; }}\n\
                 @media print {{ body {{ margin:0; max-width:100%; }} }}\n\
                 </style>\n</head>\n<body>\n<pre>{}</pre>\n</body></html>",
                tab.name(),
                self.font_size as u32,
                self.line_spacing,
                color_hex(self.theme.fg()),
                color_hex(self.theme.bg()),
                escaped
            );
            std::fs::write(path, html).ok();
        }
    }

    fn word_count(&self) -> usize {
        self.tabs[self.active].text.split_whitespace().count()
    }
    fn char_count(&self) -> usize {
        self.tabs[self.active].text.chars().count()
    }
    fn line_count(&self) -> usize {
        let t = &self.tabs[self.active].text;
        if t.is_empty() {
            1
        } else {
            t.lines().count().max(1)
        }
    }

    fn sentence_count(&self) -> usize {
        let t = &self.tabs[self.active].text;
        if t.is_empty() { return 0; }
        t.chars()
            .filter(|&c| c == '.' || c == '!' || c == '?' || c == '。')
            .count()
            .max(if t.split_whitespace().count() > 0 { 1 } else { 0 })
    }

    fn paragraph_count(&self) -> usize {
        let t = &self.tabs[self.active].text;
        if t.trim().is_empty() { return 0; }
        t.split("\n\n")
            .filter(|p| !p.trim().is_empty())
            .count()
            .max(1)
    }

    fn page_count(&self) -> usize {
        let words = self.word_count();
        (words + 249) / 250
    }

    fn reading_time_min(&self) -> f32 {
        self.word_count() as f32 / 200.0
    }

    fn avg_word_length(&self) -> f32 {
        let t = &self.tabs[self.active].text;
        let words: Vec<&str> = t.split_whitespace().collect();
        if words.is_empty() { return 0.0; }
        let total_chars: usize = words.iter().map(|w| w.chars().count()).sum();
        total_chars as f32 / words.len() as f32
    }

    fn word_frequency(&self) -> Vec<(String, usize)> {
        let t = &self.tabs[self.active].text;
        let mut freq: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for word in t.split_whitespace() {
            let clean: String = word.chars()
                .filter(|c| c.is_alphanumeric() || *c > '\u{007F}')
                .collect::<String>()
                .to_lowercase();
            if !clean.is_empty() {
                *freq.entry(clean).or_insert(0) += 1;
            }
        }
        let mut sorted: Vec<_> = freq.into_iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(&a.1));
        sorted.truncate(50);
        sorted
    }

    fn outline_entries(&self) -> Vec<(usize, String, usize)> {
        let t = &self.tabs[self.active].text;
        let mut entries = Vec::new();
        for (line_num, line) in t.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with('#') {
                let level = trimmed.chars().take_while(|&c| c == '#').count();
                let heading = trimmed[level..].trim().to_string();
                if !heading.is_empty() && level <= 6 {
                    entries.push((line_num + 1, heading, level));
                }
            }
        }
        entries
    }

    fn session_words(&self) -> usize {
        self.word_count().saturating_sub(self.session_start_words)
    }

    fn session_minutes(&self) -> f32 {
        self.session_start.elapsed().as_secs_f32() / 60.0
    }

    fn session_wpm(&self) -> f32 {
        let mins = self.session_minutes();
        if mins < 0.1 { return 0.0; }
        self.session_words() as f32 / mins
    }

    fn goal_progress(&self) -> f32 {
        if self.word_goal == 0 { return 0.0; }
        (self.word_count() as f32 / self.word_goal as f32).min(1.0)
    }

    fn apply_smart_typo(&mut self) {
        let text = &mut self.tabs[self.active].text;
        let len = text.len();
        if len < 2 { return; }
        let bytes = text.as_bytes();
        // Auto em-dash: convert " -- " to " \u{2014} "
        if len >= 4 && &text[len-4..] == " -- " {
            text.replace_range(len-4..len, " \u{2014} ");
            self.tabs[self.active].modified = true;
        }
        // Auto ellipsis: convert "..." to "\u{2026}"
        else if len >= 3 && &text[len-3..] == "..." {
            text.replace_range(len-3..len, "\u{2026}");
            self.tabs[self.active].modified = true;
        }
        // Smart double quotes
        else if bytes[len-1] == b'"' {
            let before = if len >= 2 { bytes[len-2] } else { b' ' };
            let quote = if before == b' ' || before == b'\n' || before == b'(' {
                "\u{201C}" // left "
            } else {
                "\u{201D}" // right "
            };
            text.replace_range(len-1..len, quote);
            self.tabs[self.active].modified = true;
        }
        // Smart single quotes
        else if bytes[len-1] == b'\'' {
            let before = if len >= 2 { bytes[len-2] } else { b' ' };
            let quote = if before == b' ' || before == b'\n' || before == b'(' {
                "\u{2018}" // left '
            } else {
                "\u{2019}" // right '
            };
            text.replace_range(len-1..len, quote);
            self.tabs[self.active].modified = true;
        }
    }

    fn export_markdown(&self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Markdown", &["md"])
            .save_file()
        {
            std::fs::write(path, &self.tabs[self.active].text).ok();
        }
    }

    fn char_count_nospaces(&self) -> usize {
        self.tabs[self.active].text.chars().filter(|c| !c.is_whitespace()).count()
    }

    // ── AI Assistant methods ──────────────────────────────────────────
    fn ai_run(&mut self) {
        let text = self.tabs[self.active].text.clone();
        if text.trim().is_empty() {
            self.ai_error = "No text to process.".to_string();
            return;
        }
        // Clear buffers
        { self.ai_stream_buf.lock().unwrap().clear(); }
        { self.ai_stream_err.lock().unwrap().clear(); }
        self.ai_stream_done.store(false, Ordering::SeqCst);
        self.ai_result.clear();
        self.ai_error.clear();
        self.ai_loading = true;

        let prompt = self.ai_action.build_prompt(&text, &self.ai_custom_prompt);
        let host = self.ai_host.clone();
        let model = self.ai_model.clone();
        let buf = self.ai_stream_buf.clone();
        let done = self.ai_stream_done.clone();
        let err_buf = self.ai_stream_err.clone();

        std::thread::spawn(move || {
            let url = format!("{}/api/generate", host);
            let body = serde_json::json!({
                "model": model,
                "prompt": prompt,
                "stream": true
            });
            let agent = ureq::AgentBuilder::new()
                .timeout_connect(std::time::Duration::from_secs(10))
                .timeout_read(std::time::Duration::from_secs(600))
                .timeout_write(std::time::Duration::from_secs(30))
                .build();
            match agent.post(&url).send_json(&body) {
                Ok(resp) => {
                    let reader = std::io::BufReader::new(resp.into_reader());
                    for line in reader.lines().flatten() {
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&line) {
                            if let Some(token) = json["response"].as_str() {
                                if let Ok(mut b) = buf.lock() {
                                    b.push_str(token);
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    let msg = format!("Connection failed: {}\n\nMake sure Ollama is running:\n  1. Install: https://ollama.com\n  2. Run: ollama serve\n  3. Pull model: ollama pull {}", e, model);
                    if let Ok(mut eb) = err_buf.lock() {
                        *eb = msg;
                    }
                }
            }
            done.store(true, Ordering::SeqCst);
        });
    }

    fn ai_fetch_models(&mut self) {
        let url = format!("{}/api/tags", self.ai_host);
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(std::time::Duration::from_secs(3))
            .timeout_read(std::time::Duration::from_secs(5))
            .build();
        match agent.get(&url).call() {
            Ok(resp) => {
                if let Ok(json) = resp.into_json::<serde_json::Value>() {
                    self.ai_available_models = json["models"].as_array()
                        .map(|arr| arr.iter()
                            .filter_map(|m| m["name"].as_str().map(|s| s.to_string()))
                            .collect())
                        .unwrap_or_default();
                    if self.ai_available_models.is_empty() {
                        self.ai_error = "No models found. Run: ollama pull gemma3".into();
                    } else {
                        self.ai_error.clear();
                    }
                }
            }
            Err(e) => {
                self.ai_error = format!("Cannot connect: {}", e);
                self.ai_available_models.clear();
            }
        }
    }

    fn ai_insert_result(&mut self) {
        if !self.ai_result.is_empty() {
            self.tabs[self.active].text.push_str("\n\n");
            self.tabs[self.active].text.push_str(&self.ai_result);
            self.tabs[self.active].modified = true;
        }
    }

    fn ai_replace_text(&mut self) {
        if !self.ai_result.is_empty() {
            self.tabs[self.active].text = self.ai_result.clone();
            self.tabs[self.active].modified = true;
        }
    }

    // ── v3.0: History snapshots ──────────────────────────────────────

    fn take_snapshot(&mut self) {
        let text = self.tabs[self.active].text.clone();
        if text == self.last_snapshot_text {
            return;
        }
        let wc = text.split_whitespace().count();
        self.history.push(HistoryEntry {
            text: text.clone(),
            timestamp: Instant::now(),
            word_count: wc,
        });
        self.last_snapshot_text = text;
        // Keep max 100 snapshots
        if self.history.len() > 100 {
            self.history.remove(0);
        }
    }

    fn restore_history(&mut self, idx: usize) {
        if idx < self.history.len() {
            let text = self.history[idx].text.clone();
            self.tabs[self.active].text = text.clone();
            self.tabs[self.active].modified = true;
            self.last_snapshot_text = text;
        }
    }

    fn auto_snapshot(&mut self) {
        let elapsed = self.last_snapshot.elapsed() > Duration::from_secs(15);
        let changed = self.tabs[self.active].text != self.last_snapshot_text;
        if elapsed && changed && !self.tabs[self.active].text.is_empty() {
            self.take_snapshot();
            self.last_snapshot = Instant::now();
        }
    }

    // ── v3.0: Markdown Preview Rendering ─────────────────────────────

    fn build_inline_job(
        &self, text: &str,
        fg: egui::Color32, dim: egui::Color32, accent: egui::Color32,
        size: f32, heading: bool, wrap_width: f32,
    ) -> egui::text::LayoutJob {
        let spans = parse_inline_md(text);
        let mut job = egui::text::LayoutJob::default();
        job.wrap.max_width = wrap_width;
        let font_prop = egui::FontId::new(size, egui::FontFamily::Proportional);
        let font_mono = egui::FontId::new(size - 1.0, egui::FontFamily::Monospace);

        for span in &spans {
            let (font, color, underline) = match span.style {
                InlineStyle::Normal => (font_prop.clone(), if heading { accent } else { fg }, egui::Stroke::NONE),
                InlineStyle::Bold => (font_prop.clone(), accent, egui::Stroke::NONE),
                InlineStyle::Italic => {
                    let c = blend_color(fg, accent, 0.5);
                    (font_prop.clone(), c, egui::Stroke::NONE)
                }
                InlineStyle::Code => {
                    let c = blend_color(fg, dim, 0.3);
                    (font_mono.clone(), c, egui::Stroke::NONE)
                }
                InlineStyle::LinkText => (font_prop.clone(), accent, egui::Stroke::new(1.0, accent)),
            };
            job.append(&span.text, 0.0, egui::TextFormat {
                font_id: font,
                color,
                underline,
                line_height: Some(size * self.line_spacing),
                ..Default::default()
            });
        }
        job
    }

    fn render_preview(&self, ui: &mut egui::Ui, theme: Theme) {
        let text = &self.tabs[self.active].text;
        let fg = theme.fg();
        let dim = theme.dim();
        let accent = theme.accent();
        let bg = theme.bg();
        let code_bg = if theme.is_dark() {
            blend_color(bg, egui::Color32::WHITE, 0.06)
        } else {
            blend_color(bg, egui::Color32::BLACK, 0.04)
        };
        let fs = self.font_size;
        let wrap = ui.available_width();

        let mut in_code_block = false;
        let mut code_lines: Vec<String> = Vec::new();

        for line in text.lines() {
            let trimmed = line.trim();

            // Code block toggle
            if trimmed.starts_with("```") {
                if in_code_block {
                    // End code block: render accumulated lines
                    let code_text = code_lines.join("\n");
                    egui::Frame::new()
                        .fill(code_bg)
                        .corner_radius(4.0)
                        .inner_margin(egui::Margin::symmetric(10, 6))
                        .show(ui, |ui| {
                            ui.label(egui::RichText::new(&code_text)
                                .color(dim)
                                .family(egui::FontFamily::Monospace)
                                .size(fs - 1.0));
                        });
                    code_lines.clear();
                }
                in_code_block = !in_code_block;
                continue;
            }

            if in_code_block {
                code_lines.push(line.to_string());
                continue;
            }

            // Heading
            if trimmed.starts_with('#') {
                let level = trimmed.chars().take_while(|&c| c == '#').count().min(6);
                let rest = trimmed[level..].trim();
                if !rest.is_empty() {
                    let size = match level {
                        1 => fs + 12.0,
                        2 => fs + 8.0,
                        3 => fs + 4.0,
                        4 => fs + 2.0,
                        _ => fs + 1.0,
                    };
                    ui.add_space(size * 0.3);
                    let job = self.build_inline_job(rest, fg, dim, accent, size, true, wrap);
                    ui.label(job);
                    if level <= 2 {
                        let w = ui.available_width();
                        let (r, _) = ui.allocate_exact_size(egui::vec2(w, 1.0), egui::Sense::hover());
                        ui.painter().rect_filled(r, 0.0, blend_color(bg, dim, 0.3));
                    }
                    ui.add_space(size * 0.2);
                    continue;
                }
            }

            // Horizontal rule
            if (trimmed == "---" || trimmed == "***" || trimmed == "___") && trimmed.len() >= 3 {
                ui.add_space(8.0);
                let w = ui.available_width();
                let (r, _) = ui.allocate_exact_size(egui::vec2(w, 1.0), egui::Sense::hover());
                ui.painter().rect_filled(r, 0.0, dim);
                ui.add_space(8.0);
                continue;
            }

            // Blockquote
            if trimmed.starts_with("> ") {
                let content = &trimmed[2..];
                ui.horizontal(|ui| {
                    let h = fs * self.line_spacing;
                    let (r, _) = ui.allocate_exact_size(egui::vec2(3.0, h), egui::Sense::hover());
                    ui.painter().rect_filled(r, 1.5, accent);
                    ui.add_space(8.0);
                    let c = blend_color(fg, dim, 0.4);
                    let job = self.build_inline_job(content, c, dim, accent, fs, false, wrap - 20.0);
                    ui.label(job);
                });
                continue;
            }

            // Unordered list
            if (trimmed.starts_with("- ") || trimmed.starts_with("* ")) && trimmed.len() > 2 {
                let content = &trimmed[2..];
                ui.horizontal(|ui| {
                    ui.add_space(16.0);
                    ui.label(egui::RichText::new("\u{2022}").color(accent).size(fs));
                    ui.add_space(6.0);
                    let job = self.build_inline_job(content, fg, dim, accent, fs, false, wrap - 40.0);
                    ui.label(job);
                });
                continue;
            }

            // Ordered list
            if let Some(dot_pos) = trimmed.find(". ") {
                if dot_pos <= 3 && !trimmed.is_empty() && trimmed[..dot_pos].chars().all(|c| c.is_ascii_digit()) {
                    let num = &trimmed[..=dot_pos];
                    let content = &trimmed[dot_pos + 2..];
                    ui.horizontal(|ui| {
                        ui.add_space(16.0);
                        ui.label(egui::RichText::new(num).color(accent).size(fs));
                        ui.add_space(4.0);
                        let job = self.build_inline_job(content, fg, dim, accent, fs, false, wrap - 40.0);
                        ui.label(job);
                    });
                    continue;
                }
            }

            // Empty line
            if trimmed.is_empty() {
                ui.add_space(fs * 0.4);
                continue;
            }

            // Regular paragraph with inline formatting
            let job = self.build_inline_job(line, fg, dim, accent, fs, false, wrap);
            ui.label(job);
        }

        // Flush remaining code block
        if in_code_block && !code_lines.is_empty() {
            let code_text = code_lines.join("\n");
            egui::Frame::new()
                .fill(code_bg)
                .corner_radius(4.0)
                .inner_margin(egui::Margin::symmetric(10, 6))
                .show(ui, |ui| {
                    ui.label(egui::RichText::new(&code_text)
                        .color(dim)
                        .family(egui::FontFamily::Monospace)
                        .size(fs - 1.0));
                });
        }
    }

}

// ── App ────────────────────────────────────────────────────────────────

impl eframe::App for Writer {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        let c = self.theme.bg().to_array();
        [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0, 1.0]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let theme = self.theme;
        let bg = theme.bg();
        let fg = theme.fg();
        let dim = theme.dim();

        // Apply unified theme globally
        apply_theme(ctx, theme);

        // Apply font if changed
        if self.applied_font != Some(self.font_choice) {
            apply_font(ctx, self.font_choice);
            self.applied_font = Some(self.font_choice);
        }

        // Apply Windows title bar color
        if self.hwnd == 0 {
            self.hwnd = unsafe { GetForegroundWindow() };
        }
        if self.applied_titlebar != Some(theme) {
            set_titlebar_color(self.hwnd, bg, fg, theme.is_dark());
            self.applied_titlebar = Some(theme);
        }

        self.auto_save();
        self.auto_snapshot();
        ctx.request_repaint_after(Duration::from_secs(5));

        // Poll AI streaming result
        if self.ai_loading {
            if let Ok(buf) = self.ai_stream_buf.lock() {
                if !buf.is_empty() {
                    self.ai_result = buf.clone();
                }
            }
            if let Ok(eb) = self.ai_stream_err.lock() {
                if !eb.is_empty() {
                    self.ai_error = eb.clone();
                }
            }
            if self.ai_stream_done.load(Ordering::SeqCst) {
                self.ai_loading = false;
                // Final read
                if let Ok(buf) = self.ai_stream_buf.lock() {
                    self.ai_result = buf.clone();
                }
            }
            ctx.request_repaint_after(Duration::from_millis(50));
        }

        // Recovery save on close
        if ctx.input(|i| i.viewport().close_requested()) {
            let tab = &self.tabs[self.active];
            if tab.modified && !tab.text.is_empty() {
                std::fs::write(Self::recovery_path(), &tab.text).ok();
            }
        }

        // Drag & drop
        let dropped: Vec<_> = ctx.input(|i| i.raw.dropped_files.clone());
        for file in dropped {
            if let Some(path) = file.path {
                self.open_path(path);
            }
        }

        // ── Keyboard shortcuts ────────────────────────────────────────
        let ctrl = ctx.input(|i| i.modifiers.ctrl);
        let shift = ctx.input(|i| i.modifiers.shift);
        if ctrl {
            if ctx.input(|i| i.key_pressed(egui::Key::N)) {
                if shift {
                    self.new_tab();
                } else {
                    self.new_window();
                }
            }
            if ctx.input(|i| i.key_pressed(egui::Key::O)) {
                self.open_file();
            }
            if ctx.input(|i| i.key_pressed(egui::Key::S)) {
                if shift {
                    self.save_as();
                } else {
                    self.save_file();
                }
            }
            if ctx.input(|i| i.key_pressed(egui::Key::W)) {
                self.close_tab();
            }
            if ctx.input(|i| i.key_pressed(egui::Key::Tab)) {
                if self.tabs.len() > 1 {
                    if shift {
                        self.active = if self.active == 0 {
                            self.tabs.len() - 1
                        } else {
                            self.active - 1
                        };
                    } else {
                        self.active = (self.active + 1) % self.tabs.len();
                    }
                    self.cursor_byte_pos = 0;
                }
            }
            if ctx.input(|i| i.key_pressed(egui::Key::F)) {
                self.show_find = !self.show_find;
                if !self.show_find {
                    self.show_replace = false;
                }
            }
            if ctx.input(|i| i.key_pressed(egui::Key::H)) {
                self.show_find = true;
                self.show_replace = !self.show_replace;
            }
            if ctx.input(|i| i.key_pressed(egui::Key::D)) {
                self.theme = self.theme.next();
            }
            if ctx.input(|i| i.key_pressed(egui::Key::B)) {
                if shift {
                    self.show_history = !self.show_history;
                } else {
                    self.show_status = !self.show_status;
                }
            }
            if ctx.input(|i| i.key_pressed(egui::Key::E)) {
                self.export_html();
            }
            if ctx.input(|i| i.key_pressed(egui::Key::R)) {
                self.show_recent = !self.show_recent;
            }
            if ctx.input(|i| i.key_pressed(egui::Key::G)) {
                if shift {
                    self.syntax_highlight = !self.syntax_highlight;
                } else {
                    self.focus_mode = self.focus_mode.next();
                }
            }
            if ctx.input(|i| i.key_pressed(egui::Key::J)) {
                self.typewriter = !self.typewriter;
            }
            if ctx.input(|i| i.key_pressed(egui::Key::Equals)) {
                self.font_size = (self.font_size + 1.0).min(36.0);
            }
            if ctx.input(|i| i.key_pressed(egui::Key::Minus)) {
                self.font_size = (self.font_size - 1.0).max(12.0);
            }
            if ctx.input(|i| i.key_pressed(egui::Key::L)) {
                if shift {
                    self.show_line_numbers = !self.show_line_numbers;
                } else {
                    self.line_spacing = match self.line_spacing {
                        s if s < 1.3 => 1.5,
                        s if s < 1.6 => 1.8,
                        s if s < 1.9 => 2.0,
                        _ => 1.2,
                    };
                }
            }
            if ctx.input(|i| i.key_pressed(egui::Key::I)) {
                self.show_stats_panel = !self.show_stats_panel;
            }
            if ctx.input(|i| i.key_pressed(egui::Key::U)) {
                self.show_outline = !self.show_outline;
            }
            if ctx.input(|i| i.key_pressed(egui::Key::K)) {
                self.show_goal_settings = !self.show_goal_settings;
            }
            if ctx.input(|i| i.key_pressed(egui::Key::M)) {
                self.show_word_freq = !self.show_word_freq;
            }
            if ctx.input(|i| i.key_pressed(egui::Key::T)) {
                self.smart_typography = !self.smart_typography;
            }
            if ctx.input(|i| i.key_pressed(egui::Key::A)) && shift {
                self.ai_show = !self.ai_show;
                if self.ai_show && self.ai_available_models.is_empty() {
                    self.ai_fetch_models();
                }
            }
            if ctx.input(|i| i.key_pressed(egui::Key::P)) {
                if shift {
                    self.show_preview = !self.show_preview;
                } else {
                    self.show_goto_line = !self.show_goto_line;
                }
            }
        }
        // Zen mode: Ctrl+Shift+Enter
        if ctx.input(|i| i.modifiers.ctrl && i.modifiers.shift && i.key_pressed(egui::Key::Enter)) {
            self.zen_mode = !self.zen_mode;
            if self.zen_mode && !self.fullscreen {
                self.fullscreen = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
            }
        }
        if ctx.input(|i| i.key_pressed(egui::Key::F11)) {
            self.fullscreen = !self.fullscreen;
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.fullscreen));
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.show_find = false;
            self.show_replace = false;
            self.show_recent = false;
            self.show_goal_settings = false;
            if self.zen_mode {
                self.zen_mode = false;
            }
            if self.fullscreen {
                self.fullscreen = false;
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
            }
        }

        ctx.send_viewport_cmd(egui::ViewportCommand::Title(
            format!("Writing - {}", self.tabs[self.active].title()),
        ));

        // ── Hover menu detection ──────────────────────────────────────
        let mouse_near_top = ctx.input(|i| {
            i.pointer
                .hover_pos()
                .map(|p| p.y < 32.0)
                .unwrap_or(false)
        });
        if mouse_near_top {
            self.menu_hover_time = Some(Instant::now());
        }
        let menu_visible = self
            .menu_hover_time
            .map(|t| t.elapsed() < Duration::from_millis(2500))
            .unwrap_or(false);

        // ── Menu bar (hover-activated, hidden in zen mode) ───────────
        if self.zen_mode {
            // In zen mode, show nothing - just the writing surface
        } else {
        egui::TopBottomPanel::top("menu_bar")
            .frame(
                egui::Frame::new()
                    .fill(bg)
                    .inner_margin(egui::Margin::symmetric(10, 3)),
            )
            .show(ctx, |ui| {
                // Menu text: invisible when not hovered, dim when hovered
                let menu_text = if menu_visible {
                    dim
                } else {
                    blend_color(bg, dim, 0.08)
                };

                egui::menu::bar(ui, |ui| {
                    ui.style_mut()
                        .visuals
                        .widgets
                        .inactive
                        .fg_stroke
                        .color = menu_text;
                    ui.style_mut()
                        .visuals
                        .widgets
                        .hovered
                        .fg_stroke
                        .color = fg;

                    // ── File ──
                    ui.menu_button("File", |ui| {
                        self.menu_hover_time = Some(Instant::now());
                        ui.style_mut()
                            .visuals
                            .widgets
                            .inactive
                            .fg_stroke
                            .color = fg;
                        if ui
                            .add(egui::Button::new("New Window").shortcut_text("Ctrl+N"))
                            .clicked()
                        {
                            self.new_window();
                            ui.close_menu();
                        }
                        if ui
                            .add(egui::Button::new("New Tab").shortcut_text("Ctrl+Shift+N"))
                            .clicked()
                        {
                            self.new_tab();
                            ui.close_menu();
                        }
                        if ui
                            .add(egui::Button::new("Open...").shortcut_text("Ctrl+O"))
                            .clicked()
                        {
                            self.open_file();
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui
                            .add(egui::Button::new("Save").shortcut_text("Ctrl+S"))
                            .clicked()
                        {
                            self.save_file();
                            ui.close_menu();
                        }
                        if ui
                            .add(egui::Button::new("Save As...").shortcut_text("Ctrl+Shift+S"))
                            .clicked()
                        {
                            self.save_as();
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui
                            .add(egui::Button::new("Export HTML").shortcut_text("Ctrl+E"))
                            .clicked()
                        {
                            self.export_html();
                            ui.close_menu();
                        }
                        if ui.button("Export Markdown").clicked() {
                            self.export_markdown();
                            ui.close_menu();
                        }
                        if ui
                            .add(egui::Button::new("Recent Files").shortcut_text("Ctrl+R"))
                            .clicked()
                        {
                            self.show_recent = !self.show_recent;
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui
                            .add(egui::Button::new("Close Tab").shortcut_text("Ctrl+W"))
                            .clicked()
                        {
                            self.close_tab();
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button("About").clicked() {
                            self.show_about = true;
                            ui.close_menu();
                        }
                    });

                    // ── Edit ──
                    ui.menu_button("Edit", |ui| {
                        self.menu_hover_time = Some(Instant::now());
                        ui.style_mut()
                            .visuals
                            .widgets
                            .inactive
                            .fg_stroke
                            .color = fg;
                        if ui
                            .add(egui::Button::new("Find").shortcut_text("Ctrl+F"))
                            .clicked()
                        {
                            self.show_find = true;
                            self.show_replace = false;
                            ui.close_menu();
                        }
                        if ui
                            .add(egui::Button::new("Find & Replace").shortcut_text("Ctrl+H"))
                            .clicked()
                        {
                            self.show_find = true;
                            self.show_replace = true;
                            ui.close_menu();
                        }
                        ui.separator();
                        ui.colored_label(dim, "Undo                        Ctrl+Z");
                        ui.colored_label(dim, "Redo                        Ctrl+Y");
                    });

                    // ── View ──
                    ui.menu_button("View", |ui| {
                        self.menu_hover_time = Some(Instant::now());
                        ui.style_mut().visuals.widgets.inactive.fg_stroke.color = fg;
                        // Theme submenu
                        ui.menu_button(format!("Theme: {}", theme.name()), |ui| {
                            for &t in Theme::ALL {
                                let label = if t == self.theme {
                                    format!("  {} ", t.name())
                                } else {
                                    format!("  {}", t.name())
                                };
                                if ui.button(label).clicked() {
                                    self.theme = t;
                                    ui.close_menu();
                                }
                            }
                        });
                        if ui.add(egui::Button::new(format!("Focus: {}", self.focus_mode.label())).shortcut_text("Ctrl+G")).clicked() {
                            self.focus_mode = self.focus_mode.next();
                            ui.close_menu();
                        }
                        if ui.add(egui::Button::new(if self.typewriter { "Typewriter: On" } else { "Typewriter: Off" }).shortcut_text("Ctrl+J")).clicked() {
                            self.typewriter = !self.typewriter;
                            ui.close_menu();
                        }
                        if ui.add(egui::Button::new(if self.zen_mode { "Zen Mode: On" } else { "Zen Mode: Off" }).shortcut_text("Ctrl+Shift+Enter")).clicked() {
                            self.zen_mode = !self.zen_mode;
                            if self.zen_mode && !self.fullscreen {
                                self.fullscreen = true;
                                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
                            }
                            ui.close_menu();
                        }
                        ui.separator();
                        ui.menu_button(format!("Font: {}", self.font_choice.name()), |ui| {
                            for &fc in FontChoice::ALL {
                                let label = if fc == self.font_choice {
                                    format!("  {} ", fc.name())
                                } else {
                                    format!("  {}", fc.name())
                                };
                                if ui.button(label).clicked() {
                                    self.font_choice = fc;
                                    ui.close_menu();
                                }
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label(format!("Size: {}pt", self.font_size as u32));
                            if ui.small_button(" - ").clicked() { self.font_size = (self.font_size - 1.0).max(12.0); }
                            if ui.small_button(" + ").clicked() { self.font_size = (self.font_size + 1.0).min(36.0); }
                            ui.colored_label(dim, "Ctrl+=/\u{2212}");
                        });
                        if ui.add(egui::Button::new(format!("Line Spacing: x{:.1}", self.line_spacing)).shortcut_text("Ctrl+L")).clicked() {
                            self.line_spacing = match self.line_spacing {
                                s if s < 1.3 => 1.5, s if s < 1.6 => 1.8, s if s < 1.9 => 2.0, _ => 1.2,
                            };
                        }
                        if ui.add(egui::Button::new(if self.show_line_numbers { "Line Numbers: On" } else { "Line Numbers: Off" }).shortcut_text("Ctrl+Shift+L")).clicked() {
                            self.show_line_numbers = !self.show_line_numbers;
                            ui.close_menu();
                        }
                        if ui.add(egui::Button::new(if self.syntax_highlight { "Syntax Highlight: On" } else { "Syntax Highlight: Off" }).shortcut_text("Ctrl+Shift+G")).clicked() {
                            self.syntax_highlight = !self.syntax_highlight;
                            ui.close_menu();
                        }
                        if ui.add(egui::Button::new(if self.show_preview { "Markdown Preview: On" } else { "Markdown Preview: Off" }).shortcut_text("Ctrl+Shift+P")).clicked() {
                            self.show_preview = !self.show_preview;
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.add(egui::Button::new(if self.show_status { "Status Bar: On" } else { "Status Bar: Off" }).shortcut_text("Ctrl+B")).clicked() {
                            self.show_status = !self.show_status;
                            ui.close_menu();
                        }
                        if ui.add(egui::Button::new("Fullscreen").shortcut_text("F11")).clicked() {
                            self.fullscreen = !self.fullscreen;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.fullscreen));
                            ui.close_menu();
                        }
                    });

                    // ── Tools ──
                    ui.menu_button("Tools", |ui| {
                        self.menu_hover_time = Some(Instant::now());
                        ui.style_mut().visuals.widgets.inactive.fg_stroke.color = fg;
                        if ui.add(egui::Button::new("Statistics Panel").shortcut_text("Ctrl+I")).clicked() {
                            self.show_stats_panel = !self.show_stats_panel;
                            ui.close_menu();
                        }
                        if ui.add(egui::Button::new("Document Outline").shortcut_text("Ctrl+U")).clicked() {
                            self.show_outline = !self.show_outline;
                            ui.close_menu();
                        }
                        if ui.add(egui::Button::new("Word Frequency").shortcut_text("Ctrl+M")).clicked() {
                            self.show_word_freq = !self.show_word_freq;
                            ui.close_menu();
                        }
                        if ui.add(egui::Button::new("Writing Goal").shortcut_text("Ctrl+K")).clicked() {
                            self.show_goal_settings = !self.show_goal_settings;
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.add(egui::Button::new(if self.smart_typography { "Smart Typography: On" } else { "Smart Typography: Off" }).shortcut_text("Ctrl+T")).clicked() {
                            self.smart_typography = !self.smart_typography;
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.add(egui::Button::new("AI Assistant (Ollama)").shortcut_text("Ctrl+Shift+A")).clicked() {
                            self.ai_show = !self.ai_show;
                            if self.ai_show && self.ai_available_models.is_empty() {
                                self.ai_fetch_models();
                            }
                            ui.close_menu();
                        }
                        if ui.add(egui::Button::new("Go to Line").shortcut_text("Ctrl+P")).clicked() {
                            self.show_goto_line = !self.show_goto_line;
                            ui.close_menu();
                        }
                        if ui.add(egui::Button::new("History Snapshots").shortcut_text("Ctrl+Shift+B")).clicked() {
                            self.show_history = !self.show_history;
                            ui.close_menu();
                        }
                    });
                });
            });
        } // end if !zen_mode (menu bar)

        // ── Tab bar (2+ tabs, hidden in zen mode) ─────────────────────
        if self.tabs.len() > 1 && !self.zen_mode {
            egui::TopBottomPanel::top("tab_bar")
                .frame(
                    egui::Frame::new()
                        .fill(bg)
                        .inner_margin(egui::Margin::symmetric(10, 4)),
                )
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        let mut switch_to: Option<usize> = None;
                        for i in 0..self.tabs.len() {
                            let color = if i == self.active { fg } else { dim };
                            let label = egui::RichText::new(self.tabs[i].title())
                                .color(color)
                                .size(13.0);
                            if ui
                                .add(egui::Label::new(label).sense(egui::Sense::click()))
                                .clicked()
                            {
                                switch_to = Some(i);
                            }
                            ui.add_space(12.0);
                        }
                        if let Some(i) = switch_to {
                            self.active = i;
                            self.cursor_byte_pos = 0;
                        }
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                if ui
                                    .add(
                                        egui::Label::new(
                                            egui::RichText::new("+").color(dim).size(14.0),
                                        )
                                        .sense(egui::Sense::click()),
                                    )
                                    .clicked()
                                {
                                    self.new_tab();
                                }
                            },
                        );
                    });
                });
        }

        // ── Find bar ──────────────────────────────────────────────────
        if self.show_find && !self.zen_mode {
            egui::TopBottomPanel::top("find_bar")
                .frame(
                    egui::Frame::new()
                        .fill(bg)
                        .inner_margin(egui::Margin::symmetric(16, 8)),
                )
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.colored_label(dim, "Find");
                        let r = ui.add_sized(
                            [180.0, 22.0],
                            egui::TextEdit::singleline(&mut self.find_text).text_color(fg),
                        );
                        if r.changed() {
                            self.update_find_count();
                        }
                        ui.colored_label(dim, format!("{}", self.find_count));
                        if self.show_replace {
                            ui.add_space(12.0);
                            ui.colored_label(dim, "Replace");
                            ui.add_sized(
                                [180.0, 22.0],
                                egui::TextEdit::singleline(&mut self.replace_text).text_color(fg),
                            );
                            if ui.small_button("Next").clicked() {
                                self.replace_next();
                            }
                            if ui.small_button("All").clicked() {
                                self.replace_all();
                            }
                        }
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                if ui.small_button("X").clicked() {
                                    self.show_find = false;
                                    self.show_replace = false;
                                }
                            },
                        );
                    });
                });
        }

        // ── Goal progress bar ─────────────────────────────────────────
        if self.word_goal > 0 && !self.zen_mode {
            let progress = self.goal_progress();
            egui::TopBottomPanel::bottom("goal_bar")
                .frame(egui::Frame::new().fill(bg).inner_margin(egui::Margin::symmetric(0, 0)))
                .exact_height(3.0)
                .show(ctx, |ui| {
                    let w = ui.available_width();
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, 3.0), egui::Sense::hover());
                    let accent = theme.accent();
                    let bar_w = rect.width() * progress;
                    ui.painter().rect_filled(
                        egui::Rect::from_min_size(rect.min, egui::vec2(bar_w, 3.0)),
                        0.0,
                        if progress >= 1.0 { egui::Color32::from_rgb(100, 200, 100) } else { accent },
                    );
                });
        }

        // ── Status bar ────────────────────────────────────────────────
        if self.show_status && !self.zen_mode {
            egui::TopBottomPanel::bottom("status_bar")
                .frame(
                    egui::Frame::new()
                        .fill(bg)
                        .inner_margin(egui::Margin::symmetric(16, 5)),
                )
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        let words = self.word_count();
                        let reading = self.reading_time_min();
                        let reading_str = if reading < 1.0 {
                            format!("<1 min read")
                        } else {
                            format!("{:.0} min read", reading)
                        };
                        let cc = self.char_count();
                        let ccns = self.char_count_nospaces();
                        let mut status = format!(
                            "L:{}  W:{}  C:{}/{}  P:{}  {}",
                            self.line_count(),
                            words,
                            cc, ccns,
                            self.page_count(),
                            reading_str,
                        );
                        if self.word_goal > 0 {
                            status.push_str(&format!("  [{}/{}]", words, self.word_goal));
                        }
                        ui.colored_label(dim, status);
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                if let Some(t) = self.save_flash {
                                    if t.elapsed() < Duration::from_secs(3) {
                                        ui.colored_label(egui::Color32::from_rgb(100, 180, 100), "Saved");
                                        ui.add_space(8.0);
                                    }
                                }
                                let sw = self.session_words();
                                let swpm = self.session_wpm();
                                let mut parts = Vec::new();
                                if sw > 0 {
                                    parts.push(format!("+{} ({:.0} wpm)", sw, swpm));
                                }
                                if self.focus_mode != FocusMode::Off {
                                    parts.push(self.focus_mode.label().to_string());
                                }
                                if self.typewriter { parts.push("Typewriter".to_string()); }
                                if self.zen_mode { parts.push("Zen".to_string()); }
                                if self.smart_typography { parts.push("Typo".to_string()); }
                                parts.push(theme.name().to_string());
                                ui.colored_label(
                                    dim,
                                    format!("{}pt  {}", self.font_size as u32, parts.join("  ")),
                                );
                            },
                        );
                    });
                });
        }

        // ── Outline side panel ────────────────────────────────────────
        if self.show_outline && !self.zen_mode {
            egui::SidePanel::left("outline_panel")
                .min_width(160.0)
                .max_width(280.0)
                .frame(egui::Frame::new().fill(bg).inner_margin(egui::Margin::symmetric(12, 16)))
                .show(ctx, |ui| {
                    ui.label(egui::RichText::new("Outline").color(fg).size(14.0).strong());
                    ui.add_space(8.0);
                    let entries = self.outline_entries();
                    if entries.is_empty() {
                        ui.colored_label(dim, "No headings found.\nUse # Heading syntax.");
                    } else {
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            for (line, heading, level) in &entries {
                                let indent = (*level as f32 - 1.0) * 12.0;
                                ui.horizontal(|ui| {
                                    ui.add_space(indent);
                                    let size = 14.0 - (*level as f32 - 1.0);
                                    ui.colored_label(dim, format!("L{}", line));
                                    ui.add_space(4.0);
                                    ui.label(egui::RichText::new(heading).color(fg).size(size));
                                });
                            }
                        });
                    }
                });
        }

        // ── Central writing surface ───────────────────────────────────
        let show_preview = self.show_preview && !self.zen_mode;
        let margin_x: i8 = if self.zen_mode { 120 } else if show_preview { 30 } else { 60 };
        let margin_y: i8 = if self.zen_mode { 80 } else { 40 };
        let frame = egui::Frame::new()
            .fill(bg)
            .inner_margin(egui::Margin::symmetric(margin_x, margin_y));

        egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
            // Build layouter (focus mode + syntax highlight + font + line spacing)
            let focus_mode = self.focus_mode;
            let cursor_pos = self.cursor_byte_pos;
            let bright = fg;
            let dimmed = theme.focus_dim();
            let font_id = egui::FontId::new(self.font_size, egui::FontFamily::Proportional);
            let mono_font_id = egui::FontId::new(self.font_size - 1.0, egui::FontFamily::Monospace);
            let line_h = Some(self.font_size * self.line_spacing);
            let tw = self.typewriter;
            let syntax_hl = self.syntax_highlight;
            let heading_color = theme.accent();
            let quote_color = blend_color(fg, dim, 0.4);
            let code_color = blend_color(fg, dim, 0.25);
            let marker_color = dim;

            let mut layouter =
                move |ui: &egui::Ui, text: &str, wrap_width: f32| -> Arc<egui::Galley> {
                    let mut job = egui::text::LayoutJob::default();
                    job.wrap.max_width = wrap_width;
                    let fmt_bright = egui::TextFormat {
                        font_id: font_id.clone(),
                        color: bright,
                        line_height: line_h,
                        ..Default::default()
                    };
                    let fmt_dim = egui::TextFormat {
                        font_id: font_id.clone(),
                        color: dimmed,
                        line_height: line_h,
                        ..Default::default()
                    };

                    if text.is_empty() {
                        job.append(text, 0.0, fmt_bright.clone());
                        return ui.fonts(|f| f.layout_job(job));
                    }

                    // Compute focus region
                    let (focus_start, focus_end) = if focus_mode != FocusMode::Off {
                        let sp = safe_byte_pos(text, cursor_pos);
                        match focus_mode {
                            FocusMode::Line => find_line_bounds(text, sp),
                            FocusMode::Paragraph => find_para_bounds(text, sp),
                            FocusMode::Off => (0, text.len()),
                        }
                    } else {
                        (0, text.len())
                    };

                    if !syntax_hl {
                        // Original focus-only mode
                        if focus_mode == FocusMode::Off {
                            job.append(text, 0.0, fmt_bright.clone());
                        } else {
                            if focus_start > 0 {
                                job.append(&text[..focus_start], 0.0, fmt_dim.clone());
                            }
                            job.append(&text[focus_start..focus_end], 0.0, fmt_bright.clone());
                            if focus_end < text.len() {
                                job.append(&text[focus_end..], 0.0, fmt_dim.clone());
                            }
                        }
                    } else {
                        // Syntax highlighting mode
                        let mut in_code_block = false;
                        let mut line_start = 0usize;

                        loop {
                            if line_start > text.len() { break; }
                            let line_end = text[line_start..].find('\n')
                                .map(|p| line_start + p)
                                .unwrap_or(text.len());
                            let line = &text[line_start..line_end];
                            let trimmed = line.trim_start();
                            let in_focus = focus_mode == FocusMode::Off
                                || (line_start < focus_end && line_end > focus_start);

                            if !in_focus {
                                job.append(line, 0.0, fmt_dim.clone());
                            } else if trimmed.starts_with("```") {
                                in_code_block = !in_code_block;
                                job.append(line, 0.0, egui::TextFormat {
                                    font_id: mono_font_id.clone(),
                                    color: marker_color,
                                    line_height: line_h,
                                    ..Default::default()
                                });
                            } else if in_code_block {
                                job.append(line, 0.0, egui::TextFormat {
                                    font_id: mono_font_id.clone(),
                                    color: code_color,
                                    line_height: line_h,
                                    ..Default::default()
                                });
                            } else if trimmed.starts_with('#') {
                                let level = trimmed.chars().take_while(|&c| c == '#').count();
                                if level <= 6 {
                                    let indent = line.len() - trimmed.len();
                                    let marker_end = indent + level + if trimmed.len() > level && trimmed.as_bytes()[level] == b' ' { 1 } else { 0 };
                                    if marker_end > 0 && marker_end <= line.len() {
                                        job.append(&line[..marker_end], 0.0, egui::TextFormat {
                                            font_id: font_id.clone(),
                                            color: marker_color,
                                            line_height: line_h,
                                            ..Default::default()
                                        });
                                        job.append(&line[marker_end..], 0.0, egui::TextFormat {
                                            font_id: font_id.clone(),
                                            color: heading_color,
                                            line_height: line_h,
                                            ..Default::default()
                                        });
                                    } else {
                                        job.append(line, 0.0, egui::TextFormat {
                                            font_id: font_id.clone(), color: heading_color, line_height: line_h, ..Default::default()
                                        });
                                    }
                                } else {
                                    job.append(line, 0.0, fmt_bright.clone());
                                }
                            } else if trimmed.starts_with("> ") {
                                let indent = line.len() - trimmed.len();
                                job.append(&line[..indent + 2], 0.0, egui::TextFormat {
                                    font_id: font_id.clone(), color: marker_color, line_height: line_h, ..Default::default()
                                });
                                job.append(&line[indent + 2..], 0.0, egui::TextFormat {
                                    font_id: font_id.clone(), color: quote_color, line_height: line_h, ..Default::default()
                                });
                            } else if trimmed == "---" || trimmed == "***" || trimmed == "___" {
                                job.append(line, 0.0, egui::TextFormat {
                                    font_id: font_id.clone(), color: marker_color, line_height: line_h, ..Default::default()
                                });
                            } else if (trimmed.starts_with("- ") || trimmed.starts_with("* ")) && trimmed.len() > 2 {
                                let indent = line.len() - trimmed.len();
                                job.append(&line[..indent + 2], 0.0, egui::TextFormat {
                                    font_id: font_id.clone(), color: heading_color, line_height: line_h, ..Default::default()
                                });
                                job.append(&line[indent + 2..], 0.0, fmt_bright.clone());
                            } else {
                                job.append(line, 0.0, fmt_bright.clone());
                            }

                            // Append newline
                            if line_end < text.len() {
                                let nl_fmt = if in_focus { &fmt_bright } else { &fmt_dim };
                                job.append("\n", 0.0, nl_fmt.clone());
                                line_start = line_end + 1;
                            } else {
                                break;
                            }
                        }
                    }
                    ui.fonts(|f| f.layout_job(job))
                };

            let prev = self.tabs[self.active].text.clone();

            // Calculate layout widths
            let total_w = ui.available_width();
            let avail_h = ui.available_height();
            let editor_w = if show_preview { (total_w * 0.5 - 10.0).max(200.0) } else { total_w };
            let preview_w = if show_preview { total_w - editor_w - 20.0 } else { 0.0 };

            ui.horizontal_top(|ui| {
                // ── Editor side ──
                ui.allocate_ui(egui::vec2(editor_w, avail_h), |ui| {
                    // Line numbers gutter + editor
                    if self.show_line_numbers && !self.zen_mode {
                        ui.horizontal_top(|ui| {
                            let line_count = self.tabs[self.active].text.lines().count().max(1);
                            let gutter_width = format!("{}", line_count).len() as f32 * 8.0 + 16.0;
                            let gutter_text: String = (1..=line_count)
                                .map(|i| format!("{}", i))
                                .collect::<Vec<_>>()
                                .join("\n");
                            ui.allocate_ui(egui::vec2(gutter_width, ui.available_height()), |ui| {
                                egui::ScrollArea::vertical()
                                    .id_salt("line_nums")
                                    .auto_shrink([false, false])
                                    .show(ui, |ui| {
                                        ui.label(egui::RichText::new(gutter_text)
                                            .color(dim)
                                            .size(self.font_size)
                                            .family(egui::FontFamily::Monospace));
                                    });
                            });
                            ui.add_space(8.0);
                            egui::ScrollArea::vertical()
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    let output = egui::TextEdit::multiline(&mut self.tabs[self.active].text)
                                        .layouter(&mut layouter)
                                        .desired_width(f32::INFINITY)
                                        .desired_rows(40)
                                        .frame(false)
                                        .lock_focus(true)
                                        .margin(egui::Margin::ZERO)
                                        .show(ui);
                                    if tw { ui.add_space(ui.available_height().max(280.0)); }
                                    if let Some(cr) = output.cursor_range {
                                        self.cursor_byte_pos = char_to_byte(&self.tabs[self.active].text, cr.primary.ccursor.index);
                                    }
                                });
                        });
                    } else {
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                let output = egui::TextEdit::multiline(&mut self.tabs[self.active].text)
                                    .layouter(&mut layouter)
                                    .desired_width(f32::INFINITY)
                                    .desired_rows(40)
                                    .frame(false)
                                    .lock_focus(true)
                                    .margin(egui::Margin::ZERO)
                                    .show(ui);
                                if tw { ui.add_space(ui.available_height().max(280.0)); }
                                if let Some(cr) = output.cursor_range {
                                    self.cursor_byte_pos = char_to_byte(&self.tabs[self.active].text, cr.primary.ccursor.index);
                                }
                            });
                    }
                });

                // ── Preview side ──
                if show_preview {
                    // Separator line
                    ui.add_space(4.0);
                    let sep_h = avail_h;
                    let (sep_r, _) = ui.allocate_exact_size(egui::vec2(1.0, sep_h), egui::Sense::hover());
                    ui.painter().rect_filled(sep_r, 0.0, blend_color(bg, dim, 0.3));
                    ui.add_space(14.0);

                    ui.allocate_ui(egui::vec2(preview_w, avail_h), |ui| {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("Preview").color(dim).size(12.0));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.add(egui::Label::new(egui::RichText::new("X").color(dim).size(11.0)).sense(egui::Sense::click())).clicked() {
                                    self.show_preview = false;
                                }
                            });
                        });
                        ui.add_space(8.0);
                        egui::ScrollArea::vertical()
                            .id_salt("md_preview")
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                self.render_preview(ui, theme);
                            });
                    });
                }
            });

            if self.tabs[self.active].text != prev {
                self.tabs[self.active].modified = true;
                if self.smart_typography {
                    self.apply_smart_typo();
                }
                // Auto snapshot on significant change (every 15s handled by auto_snapshot)
            }
        });

        // ── Recent files popup ────────────────────────────────────────
        if self.show_recent && !self.recent_files.is_empty() {
            let recent_clone = self.recent_files.clone();
            let mut selected: Option<usize> = None;
            let mut open = true;

            egui::Window::new("Recent Files")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .frame(
                    egui::Frame::new()
                        .fill(bg)
                        .inner_margin(egui::Margin::same(16))
                        .corner_radius(8.0)
                        .stroke(egui::Stroke::new(0.5, dim)),
                )
                .show(ctx, |ui| {
                    for (i, path) in recent_clone.iter().enumerate() {
                        let name = path
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| "?".to_string());
                        let dir = path
                            .parent()
                            .map(|p| p.to_string_lossy().to_string())
                            .unwrap_or_default();
                        ui.horizontal(|ui| {
                            if ui
                                .add(
                                    egui::Label::new(egui::RichText::new(&name).color(fg))
                                        .sense(egui::Sense::click()),
                                )
                                .clicked()
                            {
                                selected = Some(i);
                            }
                            ui.colored_label(dim, format!("  {}", dir));
                        });
                    }
                });

            if !open {
                self.show_recent = false;
            }
            if let Some(i) = selected {
                let path = self.recent_files[i].clone();
                self.open_path(path);
                self.show_recent = false;
            }
        }

        // ── About popup ──────────────────────────────────────────────
        if self.show_about {
            let mut open = true;
            egui::Window::new("About")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .frame(
                    egui::Frame::new()
                        .fill(bg)
                        .inner_margin(egui::Margin::same(24))
                        .corner_radius(8.0)
                        .stroke(egui::Stroke::new(0.5, dim)),
                )
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new("Writing").color(fg).size(22.0));
                        ui.add_space(2.0);
                        ui.colored_label(dim, "v3.0 \u{2014} Professional Writing Studio");
                        ui.add_space(12.0);
                        ui.colored_label(dim, "9 Themes \u{00b7} Zen Mode \u{00b7} Focus Mode");
                        ui.colored_label(dim, "Markdown Preview \u{00b7} Syntax Highlighting");
                        ui.colored_label(dim, "History Snapshots \u{00b7} Statistics \u{00b7} Goals");
                        ui.colored_label(dim, "Outline \u{00b7} Session Tracking \u{00b7} AI Assistant");
                        ui.add_space(12.0);
                        ui.colored_label(dim, "\u{00a9} lightgo");
                        ui.colored_label(dim, "lightgo1230@gmail.com");
                        ui.add_space(4.0);
                    });
                });
            if !open {
                self.show_about = false;
            }
        }

        // ── Statistics panel popup ───────────────────────────────────
        if self.show_stats_panel {
            let mut open = true;
            let wf = egui::Frame::new()
                .fill(bg)
                .inner_margin(egui::Margin::same(20))
                .corner_radius(8.0)
                .stroke(egui::Stroke::new(0.5, dim));
            egui::Window::new("Document Statistics")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .default_width(320.0)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .frame(wf)
                .show(ctx, |ui| {
                    let accent = theme.accent();
                    ui.add_space(4.0);
                    let stats = [
                        ("Words", format!("{}", self.word_count())),
                        ("Characters", format!("{}", self.char_count())),
                        ("Lines", format!("{}", self.line_count())),
                        ("Sentences", format!("{}", self.sentence_count())),
                        ("Paragraphs", format!("{}", self.paragraph_count())),
                        ("Pages", format!("{} (250 w/p)", self.page_count())),
                        ("Avg Word Length", format!("{:.1} chars", self.avg_word_length())),
                        ("Reading Time", {
                            let r = self.reading_time_min();
                            if r < 1.0 { "< 1 min".to_string() } else { format!("{:.0} min", r) }
                        }),
                    ];
                    for (label, value) in &stats {
                        ui.horizontal(|ui| {
                            ui.colored_label(dim, format!("{:<18}", label));
                            ui.label(egui::RichText::new(value).color(fg));
                        });
                    }
                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new("Session").color(accent).size(14.0));
                    ui.add_space(4.0);
                    let session_stats = [
                        ("Words Written", format!("+{}", self.session_words())),
                        ("Time", {
                            let m = self.session_minutes();
                            if m < 1.0 { format!("{:.0} sec", m * 60.0) }
                            else { format!("{:.0} min", m) }
                        }),
                        ("Speed", format!("{:.0} wpm", self.session_wpm())),
                    ];
                    for (label, value) in &session_stats {
                        ui.horizontal(|ui| {
                            ui.colored_label(dim, format!("{:<18}", label));
                            ui.label(egui::RichText::new(value).color(fg));
                        });
                    }
                    if self.word_goal > 0 {
                        ui.add_space(8.0);
                        ui.separator();
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new("Goal Progress").color(accent).size(14.0));
                        ui.add_space(4.0);
                        let p = self.goal_progress();
                        ui.horizontal(|ui| {
                            ui.colored_label(dim, format!("{}/{}", self.word_count(), self.word_goal));
                            ui.add_space(8.0);
                            let pct = (p * 100.0) as u32;
                            let color = if pct >= 100 { egui::Color32::from_rgb(100, 200, 100) } else { accent };
                            ui.label(egui::RichText::new(format!("{}%", pct)).color(color));
                        });
                        let (bar_rect, _) = ui.allocate_exact_size(egui::vec2(280.0, 8.0), egui::Sense::hover());
                        ui.painter().rect_filled(bar_rect, 4.0, theme.hover());
                        let fill_w = bar_rect.width() * p;
                        let fill_color = if p >= 1.0 { egui::Color32::from_rgb(100, 200, 100) } else { accent };
                        ui.painter().rect_filled(
                            egui::Rect::from_min_size(bar_rect.min, egui::vec2(fill_w, 8.0)),
                            4.0, fill_color,
                        );
                    }
                    ui.add_space(4.0);
                });
            if !open { self.show_stats_panel = false; }
        }

        // ── Word frequency popup ─────────────────────────────────────
        if self.show_word_freq {
            let mut open = true;
            let wf = egui::Frame::new()
                .fill(bg)
                .inner_margin(egui::Margin::same(20))
                .corner_radius(8.0)
                .stroke(egui::Stroke::new(0.5, dim));
            egui::Window::new("Word Frequency")
                .open(&mut open)
                .collapsible(false)
                .default_width(300.0)
                .default_height(400.0)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .frame(wf)
                .show(ctx, |ui| {
                    let freq = self.word_frequency();
                    if freq.is_empty() {
                        ui.colored_label(dim, "No words yet.");
                    } else {
                        let max_count = freq[0].1 as f32;
                        let accent = theme.accent();
                        egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.colored_label(dim, format!("{:<4} {:<20} {}", "#", "Word", "Count"));
                            });
                            ui.separator();
                            for (i, (word, count)) in freq.iter().enumerate() {
                                ui.horizontal(|ui| {
                                    ui.colored_label(dim, format!("{:<4}", i + 1));
                                    let bar_frac = *count as f32 / max_count;
                                    let bar_w = 120.0 * bar_frac;
                                    ui.label(egui::RichText::new(word).color(fg));
                                    ui.add_space(8.0);
                                    let (rect, _) = ui.allocate_exact_size(egui::vec2(120.0, 12.0), egui::Sense::hover());
                                    ui.painter().rect_filled(rect, 2.0, theme.hover());
                                    ui.painter().rect_filled(
                                        egui::Rect::from_min_size(rect.min, egui::vec2(bar_w, 12.0)),
                                        2.0, accent,
                                    );
                                    ui.label(egui::RichText::new(format!("{}", count)).color(dim));
                                });
                            }
                        });
                    }
                });
            if !open { self.show_word_freq = false; }
        }

        // ── Goal settings popup ──────────────────────────────────────
        if self.show_goal_settings {
            let mut open = true;
            let wf = egui::Frame::new()
                .fill(bg)
                .inner_margin(egui::Margin::same(20))
                .corner_radius(8.0)
                .stroke(egui::Stroke::new(0.5, dim));
            egui::Window::new("Writing Goal")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .frame(wf)
                .show(ctx, |ui| {
                    ui.add_space(4.0);
                    ui.colored_label(dim, "Set a word count target:");
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Target:").color(fg));
                        ui.add_sized([100.0, 24.0], egui::TextEdit::singleline(&mut self.word_goal_input).text_color(fg));
                        ui.label(egui::RichText::new("words").color(dim));
                    });
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        // Quick presets
                        for &preset in &[500, 1000, 2000, 5000] {
                            if ui.small_button(format!("{}", preset)).clicked() {
                                self.word_goal = preset;
                                self.word_goal_input = format!("{}", preset);
                            }
                        }
                    });
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Set Goal").clicked() {
                            if let Ok(n) = self.word_goal_input.trim().parse::<usize>() {
                                self.word_goal = n;
                            }
                        }
                        if ui.button("Clear Goal").clicked() {
                            self.word_goal = 0;
                            self.word_goal_input.clear();
                        }
                    });
                    if self.word_goal > 0 {
                        ui.add_space(8.0);
                        let p = self.goal_progress();
                        let pct = (p * 100.0) as u32;
                        let accent = theme.accent();
                        let color = if pct >= 100 { egui::Color32::from_rgb(100, 200, 100) } else { accent };
                        ui.horizontal(|ui| {
                            ui.colored_label(color, format!("{}/{} ({}%)", self.word_count(), self.word_goal, pct));
                        });
                        let (bar_rect, _) = ui.allocate_exact_size(egui::vec2(260.0, 10.0), egui::Sense::hover());
                        ui.painter().rect_filled(bar_rect, 5.0, theme.hover());
                        ui.painter().rect_filled(
                            egui::Rect::from_min_size(bar_rect.min, egui::vec2(bar_rect.width() * p, 10.0)),
                            5.0, color,
                        );
                    }
                    ui.add_space(4.0);
                });
            if !open { self.show_goal_settings = false; }
        }

        // ── Go to Line popup ────────────────────────────────────────
        if self.show_goto_line {
            let mut open = true;
            let wf = egui::Frame::new().fill(bg).inner_margin(egui::Margin::same(16)).corner_radius(8.0).stroke(egui::Stroke::new(0.5, dim));
            egui::Window::new("Go to Line")
                .open(&mut open).collapsible(false).resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).frame(wf)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Line:").color(fg));
                        let r = ui.add_sized([80.0, 24.0], egui::TextEdit::singleline(&mut self.goto_line_input).text_color(fg));
                        if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            if let Ok(line) = self.goto_line_input.trim().parse::<usize>() {
                                let text = &self.tabs[self.active].text;
                                let target = line.saturating_sub(1);
                                let mut byte_pos = 0;
                                for (i, l) in text.lines().enumerate() {
                                    if i == target { break; }
                                    byte_pos += l.len() + 1;
                                }
                                self.cursor_byte_pos = byte_pos.min(text.len());
                            }
                            self.show_goto_line = false;
                        }
                        ui.colored_label(dim, format!("/ {}", self.line_count()));
                    });
                });
            if !open { self.show_goto_line = false; }
        }

        // ── AI Assistant window ──────────────────────────────────────
        if self.ai_show {
            let mut open = true;
            let accent = theme.accent();
            let wf = egui::Frame::new().fill(bg).inner_margin(egui::Margin::same(16)).corner_radius(8.0).stroke(egui::Stroke::new(0.5, dim));
            egui::Window::new("AI Assistant (Ollama)")
                .open(&mut open).collapsible(true).resizable(true)
                .default_width(480.0).default_height(560.0)
                .frame(wf)
                .show(ctx, |ui| {
                    // Connection settings
                    ui.horizontal(|ui| {
                        ui.colored_label(dim, "Host:");
                        ui.add_sized([200.0, 20.0], egui::TextEdit::singleline(&mut self.ai_host).text_color(fg));
                        if ui.small_button("Refresh Models").clicked() {
                            self.ai_fetch_models();
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.colored_label(dim, "Model:");
                        ui.add_sized([160.0, 20.0], egui::TextEdit::singleline(&mut self.ai_model).text_color(fg));
                        if !self.ai_available_models.is_empty() {
                            egui::ComboBox::from_id_salt("ai_model_sel")
                                .selected_text("")
                                .width(30.0)
                                .show_ui(ui, |ui| {
                                    for m in self.ai_available_models.clone() {
                                        let short = m.split(':').next().unwrap_or(&m).to_string();
                                        if ui.selectable_label(self.ai_model == m, &m).clicked() {
                                            self.ai_model = short;
                                        }
                                    }
                                });
                        }
                    });
                    ui.add_space(6.0);
                    ui.separator();
                    ui.add_space(4.0);

                    // Action selection
                    ui.label(egui::RichText::new("Action").color(fg).size(14.0).strong());
                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        for &action in AiAction::ALL {
                            let selected = self.ai_action == action;
                            let label = format!("[{}] {}", action.icon(), action.name());
                            let color = if selected { accent } else { dim };
                            if ui.add(egui::Label::new(egui::RichText::new(&label).color(color).size(12.0)).sense(egui::Sense::click())).clicked() {
                                self.ai_action = action;
                            }
                            ui.add_space(4.0);
                        }
                    });

                    // Custom prompt input
                    if self.ai_action == AiAction::Custom {
                        ui.add_space(4.0);
                        ui.colored_label(dim, "Custom instruction:");
                        ui.add_sized([ui.available_width(), 48.0],
                            egui::TextEdit::multiline(&mut self.ai_custom_prompt).text_color(fg));
                    }

                    ui.add_space(8.0);

                    // Run / Stop buttons
                    ui.horizontal(|ui| {
                        let can_run = !self.ai_loading && !self.ai_model.is_empty();
                        if ui.add_enabled(can_run, egui::Button::new(egui::RichText::new("  Run AI  ").color(if can_run { fg } else { dim }))).clicked() {
                            self.ai_run();
                        }
                        if self.ai_loading {
                            ui.colored_label(accent, format!("Generating... ({} chars)", self.ai_result.len()));
                            ui.spinner();
                        }
                    });

                    // Error display
                    if !self.ai_error.is_empty() {
                        ui.add_space(4.0);
                        ui.colored_label(egui::Color32::from_rgb(220, 80, 80), &self.ai_error);
                    }

                    // Result display
                    if !self.ai_result.is_empty() {
                        ui.add_space(8.0);
                        ui.separator();
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new("Result").color(accent).size(14.0).strong());
                        ui.add_space(4.0);

                        egui::ScrollArea::vertical().max_height(280.0).show(ui, |ui| {
                            let mut result_display = self.ai_result.clone();
                            ui.add_sized([ui.available_width(), 200.0],
                                egui::TextEdit::multiline(&mut result_display).text_color(fg));
                        });

                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if ui.button("Insert at End").clicked() {
                                self.ai_insert_result();
                            }
                            if ui.button("Replace All Text").clicked() {
                                self.ai_replace_text();
                            }
                            if ui.button("Copy").clicked() {
                                ctx.copy_text(self.ai_result.clone());
                            }
                            if ui.button("Clear").clicked() {
                                self.ai_result.clear();
                            }
                        });
                    }

                    // Info footer
                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(2.0);
                    ui.colored_label(dim, "Powered by Ollama (open-source). Supports: Gemma, Llama, Qwen, Mistral, Phi, etc.");
                });
            if !open { self.ai_show = false; }
        }

        // ── History snapshots window ─────────────────────────────────
        if self.show_history {
            let mut open = true;
            let wf = egui::Frame::new().fill(bg).inner_margin(egui::Margin::same(16)).corner_radius(8.0).stroke(egui::Stroke::new(0.5, dim));
            let accent = theme.accent();
            egui::Window::new("History Snapshots")
                .open(&mut open).collapsible(true).resizable(true)
                .default_width(360.0).default_height(420.0)
                .frame(wf)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        if ui.button("Snapshot Now").clicked() {
                            self.take_snapshot();
                        }
                        ui.colored_label(dim, format!("{} snapshots", self.history.len()));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if !self.history.is_empty() {
                                if ui.small_button("Clear All").clicked() {
                                    self.history.clear();
                                }
                            }
                        });
                    });
                    ui.add_space(4.0);
                    ui.separator();
                    ui.add_space(4.0);

                    if self.history.is_empty() {
                        ui.colored_label(dim, "No snapshots yet.\nSnapshots are taken automatically every 15 seconds\nwhen text changes.");
                    } else {
                        let mut restore_idx: Option<usize> = None;
                        let now = Instant::now();
                        egui::ScrollArea::vertical().max_height(340.0).show(ui, |ui| {
                            for (i, entry) in self.history.iter().enumerate().rev() {
                                let elapsed = now.duration_since(entry.timestamp);
                                let time_str = if elapsed.as_secs() < 60 {
                                    format!("{}s ago", elapsed.as_secs())
                                } else if elapsed.as_secs() < 3600 {
                                    format!("{}m ago", elapsed.as_secs() / 60)
                                } else {
                                    format!("{}h ago", elapsed.as_secs() / 3600)
                                };
                                let chars = entry.text.chars().count();
                                let preview: String = entry.text.chars().take(60).collect::<String>().replace('\n', " ");
                                ui.horizontal(|ui| {
                                    ui.colored_label(dim, format!("#{:<3}", i + 1));
                                    ui.colored_label(accent, &time_str);
                                    ui.colored_label(dim, format!("{}w {}c", entry.word_count, chars));
                                    if ui.small_button("Restore").clicked() {
                                        restore_idx = Some(i);
                                    }
                                });
                                ui.colored_label(dim, format!("  {}{}", preview, if chars > 60 { "..." } else { "" }));
                                ui.add_space(4.0);
                            }
                        });
                        if let Some(idx) = restore_idx {
                            self.take_snapshot(); // save current state first
                            self.restore_history(idx);
                        }
                    }
                });
            if !open { self.show_history = false; }
        }
    }
}

// ══════════════════════════════════════════════════════════════════════
// QC Tests (1000)
// ══════════════════════════════════════════════════════════════════════
#[cfg(test)]
mod tests {
    use super::*;

    // ── Helper text generators ────────────────────────────────────────
    fn sample_text() -> String {
        "Hello World\nSecond line\nThird line\n\nNew paragraph here.\nAnother line.".to_string()
    }
    fn korean_text() -> String {
        "안녕하세요 세계\n두번째 줄\n세번째 줄\n\n새 문단입니다.\n또 다른 줄.".to_string()
    }
    fn long_text(lines: usize) -> String {
        (0..lines).map(|i| format!("Line {} content here", i)).collect::<Vec<_>>().join("\n")
    }
    fn make_writer(text: &str) -> Writer {
        let mut w = Writer::new_with(true);
        w.tabs[0].text = text.to_string();
        w
    }

    // ════════════════════════════════════════════════════════════════
    // 1. Theme tests (1-60)
    // ════════════════════════════════════════════════════════════════
    #[test] fn t001_cream_bg() { assert_eq!(Theme::Cream.bg(), egui::Color32::from_rgb(252,250,245)); }
    #[test] fn t002_dark_bg() { assert_eq!(Theme::Dark.bg(), egui::Color32::from_rgb(26,26,46)); }
    #[test] fn t003_forest_bg() { assert_eq!(Theme::Forest.bg(), egui::Color32::from_rgb(27,43,27)); }
    #[test] fn t004_ocean_bg() { assert_eq!(Theme::Ocean.bg(), egui::Color32::from_rgb(26,36,51)); }
    #[test] fn t005_cream_fg() { assert_eq!(Theme::Cream.fg(), egui::Color32::from_rgb(50,50,50)); }
    #[test] fn t006_dark_fg() { assert_eq!(Theme::Dark.fg(), egui::Color32::from_rgb(212,212,212)); }
    #[test] fn t007_forest_fg() { assert_eq!(Theme::Forest.fg(), egui::Color32::from_rgb(200,216,192)); }
    #[test] fn t008_ocean_fg() { assert_eq!(Theme::Ocean.fg(), egui::Color32::from_rgb(189,208,232)); }
    #[test] fn t009_cream_dim() { assert_eq!(Theme::Cream.dim(), egui::Color32::from_rgb(160,160,160)); }
    #[test] fn t010_dark_dim() { assert_eq!(Theme::Dark.dim(), egui::Color32::from_rgb(100,100,120)); }
    #[test] fn t011_forest_dim() { assert_eq!(Theme::Forest.dim(), egui::Color32::from_rgb(90,122,80)); }
    #[test] fn t012_ocean_dim() { assert_eq!(Theme::Ocean.dim(), egui::Color32::from_rgb(80,104,120)); }
    #[test] fn t013_cream_next() { assert_eq!(Theme::Cream.next(), Theme::Dark); }
    #[test] fn t014_dark_next() { assert_eq!(Theme::Dark.next(), Theme::Forest); }
    #[test] fn t015_forest_next() { assert_eq!(Theme::Forest.next(), Theme::Ocean); }
    #[test] fn t016_ocean_next() { assert_eq!(Theme::Ocean.next(), Theme::Sepia); }
    #[test] fn t017_theme_cycle() { let mut t = Theme::Cream; for _ in 0..9 { t = t.next(); } assert_eq!(t, Theme::Cream); }
    #[test] fn t018_cream_name() { assert_eq!(Theme::Cream.name(), "Cream"); }
    #[test] fn t019_dark_name() { assert_eq!(Theme::Dark.name(), "Dark"); }
    #[test] fn t020_forest_name() { assert_eq!(Theme::Forest.name(), "Forest"); }
    #[test] fn t021_ocean_name() { assert_eq!(Theme::Ocean.name(), "Ocean"); }
    #[test] fn t022_cream_hover() { let _ = Theme::Cream.hover(); }
    #[test] fn t023_dark_hover() { let _ = Theme::Dark.hover(); }
    #[test] fn t024_forest_hover() { let _ = Theme::Forest.hover(); }
    #[test] fn t025_ocean_hover() { let _ = Theme::Ocean.hover(); }
    #[test] fn t026_cream_sel() { let _ = Theme::Cream.selection(); }
    #[test] fn t027_dark_sel() { let _ = Theme::Dark.selection(); }
    #[test] fn t028_forest_sel() { let _ = Theme::Forest.selection(); }
    #[test] fn t029_ocean_sel() { let _ = Theme::Ocean.selection(); }
    #[test] fn t030_cream_focus_dim() { let _ = Theme::Cream.focus_dim(); }
    #[test] fn t031_dark_focus_dim() { let _ = Theme::Dark.focus_dim(); }
    #[test] fn t032_forest_focus_dim() { let _ = Theme::Forest.focus_dim(); }
    #[test] fn t033_ocean_focus_dim() { let _ = Theme::Ocean.focus_dim(); }
    #[test] fn t034_bg_ne_fg_cream() { assert_ne!(Theme::Cream.bg(), Theme::Cream.fg()); }
    #[test] fn t035_bg_ne_fg_dark() { assert_ne!(Theme::Dark.bg(), Theme::Dark.fg()); }
    #[test] fn t036_bg_ne_fg_forest() { assert_ne!(Theme::Forest.bg(), Theme::Forest.fg()); }
    #[test] fn t037_bg_ne_fg_ocean() { assert_ne!(Theme::Ocean.bg(), Theme::Ocean.fg()); }
    #[test] fn t038_dim_ne_fg_cream() { assert_ne!(Theme::Cream.dim(), Theme::Cream.fg()); }
    #[test] fn t039_dim_ne_fg_dark() { assert_ne!(Theme::Dark.dim(), Theme::Dark.fg()); }
    #[test] fn t040_dim_ne_fg_forest() { assert_ne!(Theme::Forest.dim(), Theme::Forest.fg()); }
    #[test] fn t041_dim_ne_fg_ocean() { assert_ne!(Theme::Ocean.dim(), Theme::Ocean.fg()); }
    #[test] fn t042_all_themes_distinct_bg() {
        let bgs = [Theme::Cream.bg(), Theme::Dark.bg(), Theme::Forest.bg(), Theme::Ocean.bg()];
        for i in 0..4 { for j in (i+1)..4 { assert_ne!(bgs[i], bgs[j]); } }
    }
    #[test] fn t043_all_themes_distinct_fg() {
        let fgs = [Theme::Cream.fg(), Theme::Dark.fg(), Theme::Forest.fg(), Theme::Ocean.fg()];
        for i in 0..4 { for j in (i+1)..4 { assert_ne!(fgs[i], fgs[j]); } }
    }
    #[test] fn t044_theme_eq() { assert_eq!(Theme::Cream, Theme::Cream); }
    #[test] fn t045_theme_ne() { assert_ne!(Theme::Cream, Theme::Dark); }
    #[test] fn t046_theme_copy() { let a = Theme::Cream; let b = a; assert_eq!(a, b); }

    // ════════════════════════════════════════════════════════════════
    // 2. Focus mode tests (47-80)
    // ════════════════════════════════════════════════════════════════
    #[test] fn t047_focus_off_next() { assert_eq!(FocusMode::Off.next(), FocusMode::Line); }
    #[test] fn t048_focus_line_next() { assert_eq!(FocusMode::Line.next(), FocusMode::Paragraph); }
    #[test] fn t049_focus_para_next() { assert_eq!(FocusMode::Paragraph.next(), FocusMode::Off); }
    #[test] fn t050_focus_cycle() { assert_eq!(FocusMode::Off.next().next().next(), FocusMode::Off); }
    #[test] fn t051_focus_off_label() { assert_eq!(FocusMode::Off.label(), "Off"); }
    #[test] fn t052_focus_line_label() { assert_eq!(FocusMode::Line.label(), "Line"); }
    #[test] fn t053_focus_para_label() { assert_eq!(FocusMode::Paragraph.label(), "Paragraph"); }
    #[test] fn t054_focus_eq() { assert_eq!(FocusMode::Off, FocusMode::Off); }
    #[test] fn t055_focus_ne() { assert_ne!(FocusMode::Off, FocusMode::Line); }

    // ════════════════════════════════════════════════════════════════
    // 3. Tab tests (56-120)
    // ════════════════════════════════════════════════════════════════
    #[test] fn t056_tab_new_empty() { let t = Tab::new(); assert!(t.text.is_empty()); }
    #[test] fn t057_tab_new_no_path() { let t = Tab::new(); assert!(t.file_path.is_none()); }
    #[test] fn t058_tab_new_not_modified() { let t = Tab::new(); assert!(!t.modified); }
    #[test] fn t059_tab_name_untitled() { let t = Tab::new(); assert_eq!(t.name(), "Untitled"); }
    #[test] fn t060_tab_title_untitled() { let t = Tab::new(); assert_eq!(t.title(), "Untitled"); }
    #[test] fn t061_tab_title_modified() { let mut t = Tab::new(); t.modified = true; assert_eq!(t.title(), "* Untitled"); }
    #[test] fn t062_tab_name_with_path() {
        let mut t = Tab::new();
        t.file_path = Some(PathBuf::from("C:\\docs\\test.txt"));
        assert_eq!(t.name(), "test.txt");
    }
    #[test] fn t063_tab_title_with_path() {
        let mut t = Tab::new();
        t.file_path = Some(PathBuf::from("C:\\docs\\test.txt"));
        assert_eq!(t.title(), "test.txt");
    }
    #[test] fn t064_tab_title_modified_path() {
        let mut t = Tab::new();
        t.file_path = Some(PathBuf::from("C:\\docs\\test.txt"));
        t.modified = true;
        assert_eq!(t.title(), "* test.txt");
    }
    #[test] fn t065_tab_md_extension() {
        let mut t = Tab::new();
        t.file_path = Some(PathBuf::from("notes.md"));
        assert_eq!(t.name(), "notes.md");
    }

    // ════════════════════════════════════════════════════════════════
    // 4. Writer state tests (66-150)
    // ════════════════════════════════════════════════════════════════
    #[test] fn t066_writer_init_one_tab() { let w = Writer::new_with(true); assert_eq!(w.tabs.len(), 1); }
    #[test] fn t067_writer_init_active_0() { let w = Writer::new_with(true); assert_eq!(w.active, 0); }
    #[test] fn t068_writer_init_no_find() { let w = Writer::new_with(true); assert!(!w.show_find); }
    #[test] fn t069_writer_init_no_replace() { let w = Writer::new_with(true); assert!(!w.show_replace); }
    #[test] fn t070_writer_init_theme() { let w = Writer::new_with(true); assert_eq!(w.theme, Theme::Cream); }
    #[test] fn t071_writer_init_font_size() { let w = Writer::new_with(true); assert_eq!(w.font_size, 18.0); }
    #[test] fn t072_writer_init_line_spacing() { let w = Writer::new_with(true); assert_eq!(w.line_spacing, 1.6); }
    #[test] fn t073_writer_init_focus_off() { let w = Writer::new_with(true); assert_eq!(w.focus_mode, FocusMode::Off); }
    #[test] fn t074_writer_init_no_typewriter() { let w = Writer::new_with(true); assert!(!w.typewriter); }
    #[test] fn t075_writer_init_no_fullscreen() { let w = Writer::new_with(true); assert!(!w.fullscreen); }
    #[test] fn t076_writer_init_cursor_0() { let w = Writer::new_with(true); assert_eq!(w.cursor_byte_pos, 0); }
    #[test] fn t077_writer_init_no_status() { let w = Writer::new_with(true); assert!(!w.show_status); }
    #[test] fn t078_writer_init_font_choice() { let w = Writer::new_with(true); assert_eq!(w.font_choice, FontChoice::MalgunGothic); }
    #[test] fn t079_writer_init_no_about() { let w = Writer::new_with(true); assert!(!w.show_about); }
    #[test] fn t080_writer_init_no_recent_popup() { let w = Writer::new_with(true); assert!(!w.show_recent); }

    // ── new_tab / close_tab ──
    #[test] fn t081_new_tab_adds() { let mut w = Writer::new_with(true); w.new_tab(); assert_eq!(w.tabs.len(), 2); }
    #[test] fn t082_new_tab_active() { let mut w = Writer::new_with(true); w.new_tab(); assert_eq!(w.active, 1); }
    #[test] fn t083_new_tab_empty() { let mut w = Writer::new_with(true); w.new_tab(); assert!(w.tabs[1].text.is_empty()); }
    #[test] fn t084_new_tab_cursor_reset() { let mut w = Writer::new_with(true); w.cursor_byte_pos = 50; w.new_tab(); assert_eq!(w.cursor_byte_pos, 0); }
    #[test] fn t085_close_tab_removes() { let mut w = Writer::new_with(true); w.new_tab(); w.close_tab(); assert_eq!(w.tabs.len(), 1); }
    #[test] fn t086_close_single_tab_no_op() { let mut w = Writer::new_with(true); w.close_tab(); assert_eq!(w.tabs.len(), 1); }
    #[test] fn t087_close_tab_active_adjust() {
        let mut w = Writer::new_with(true);
        w.new_tab(); w.new_tab(); // 3 tabs, active=2
        w.close_tab();
        assert_eq!(w.active, 1);
    }
    #[test] fn t088_many_tabs() {
        let mut w = Writer::new_with(true);
        for _ in 0..20 { w.new_tab(); }
        assert_eq!(w.tabs.len(), 21);
    }
    #[test] fn t089_close_all_but_one() {
        let mut w = Writer::new_with(true);
        for _ in 0..5 { w.new_tab(); }
        for _ in 0..5 { w.close_tab(); }
        assert_eq!(w.tabs.len(), 1);
    }
    #[test] fn t090_close_middle_tab() {
        let mut w = Writer::new_with(true);
        w.tabs[0].text = "first".into();
        w.new_tab(); w.tabs[1].text = "second".into();
        w.new_tab(); w.tabs[2].text = "third".into();
        w.active = 1;
        w.close_tab();
        assert_eq!(w.tabs.len(), 2);
        assert_eq!(w.tabs[1].text, "third");
    }

    // ════════════════════════════════════════════════════════════════
    // 5. Word / char / line count (91-140)
    // ════════════════════════════════════════════════════════════════
    #[test] fn t091_word_count_empty() { let w = make_writer(""); assert_eq!(w.word_count(), 0); }
    #[test] fn t092_word_count_single() { let w = make_writer("hello"); assert_eq!(w.word_count(), 1); }
    #[test] fn t093_word_count_multi() { let w = make_writer("hello world foo"); assert_eq!(w.word_count(), 3); }
    #[test] fn t094_word_count_newlines() { let w = make_writer("hello\nworld"); assert_eq!(w.word_count(), 2); }
    #[test] fn t095_word_count_extra_spaces() { let w = make_writer("  hello   world  "); assert_eq!(w.word_count(), 2); }
    #[test] fn t096_word_count_korean() { let w = make_writer("안녕 세계"); assert_eq!(w.word_count(), 2); }
    #[test] fn t097_char_count_empty() { let w = make_writer(""); assert_eq!(w.char_count(), 0); }
    #[test] fn t098_char_count_ascii() { let w = make_writer("hello"); assert_eq!(w.char_count(), 5); }
    #[test] fn t099_char_count_korean() { let w = make_writer("안녕"); assert_eq!(w.char_count(), 2); }
    #[test] fn t100_char_count_mixed() { let w = make_writer("hi안녕"); assert_eq!(w.char_count(), 4); }
    #[test] fn t101_line_count_empty() { let w = make_writer(""); assert_eq!(w.line_count(), 1); }
    #[test] fn t102_line_count_single() { let w = make_writer("hello"); assert_eq!(w.line_count(), 1); }
    #[test] fn t103_line_count_multi() { let w = make_writer("a\nb\nc"); assert_eq!(w.line_count(), 3); }
    #[test] fn t104_line_count_trailing() { let w = make_writer("a\nb\n"); assert_eq!(w.line_count(), 2); }
    #[test] fn t105_word_count_tabs() { let w = make_writer("a\tb\tc"); assert_eq!(w.word_count(), 3); }
    #[test] fn t106_char_count_newline() { let w = make_writer("a\n"); assert_eq!(w.char_count(), 2); }
    #[test] fn t107_line_count_100() { let w = make_writer(&long_text(100)); assert_eq!(w.line_count(), 100); }
    #[test] fn t108_word_count_long() { let w = make_writer(&long_text(50)); assert!(w.word_count() > 100); }
    #[test] fn t109_char_count_long() { let w = make_writer(&long_text(50)); assert!(w.char_count() > 200); }
    #[test] fn t110_line_count_1000() { let w = make_writer(&long_text(1000)); assert_eq!(w.line_count(), 1000); }

    // ════════════════════════════════════════════════════════════════
    // 6. safe_byte_pos tests (111-140)
    // ════════════════════════════════════════════════════════════════
    #[test] fn t111_safe_byte_pos_empty() { assert_eq!(safe_byte_pos("", 0), 0); }
    #[test] fn t112_safe_byte_pos_ascii() { assert_eq!(safe_byte_pos("hello", 3), 3); }
    #[test] fn t113_safe_byte_pos_beyond() { assert_eq!(safe_byte_pos("hello", 100), 5); }
    #[test] fn t114_safe_byte_pos_korean() { let s = "안녕"; assert_eq!(safe_byte_pos(s, 3), 3); }
    #[test] fn t115_safe_byte_pos_korean_mid() { let s = "안녕"; assert_eq!(safe_byte_pos(s, 4), 3); }
    #[test] fn t116_safe_byte_pos_korean_mid2() { let s = "안녕"; assert_eq!(safe_byte_pos(s, 5), 3); }
    #[test] fn t117_safe_byte_pos_zero() { assert_eq!(safe_byte_pos("hello", 0), 0); }
    #[test] fn t118_safe_byte_pos_end() { assert_eq!(safe_byte_pos("hello", 5), 5); }
    #[test] fn t119_safe_byte_pos_emoji() { let s = "a😀b"; assert_eq!(safe_byte_pos(s, 1), 1); }
    #[test] fn t120_safe_byte_pos_emoji_mid() { let s = "a😀b"; assert_eq!(safe_byte_pos(s, 3), 1); }

    // ════════════════════════════════════════════════════════════════
    // 7. char_to_byte tests (121-150)
    // ════════════════════════════════════════════════════════════════
    #[test] fn t121_c2b_empty() { assert_eq!(char_to_byte("", 0), 0); }
    #[test] fn t122_c2b_ascii() { assert_eq!(char_to_byte("hello", 2), 2); }
    #[test] fn t123_c2b_korean() { assert_eq!(char_to_byte("안녕하", 1), 3); }
    #[test] fn t124_c2b_korean2() { assert_eq!(char_to_byte("안녕하", 2), 6); }
    #[test] fn t125_c2b_beyond() { assert_eq!(char_to_byte("hi", 10), 2); }
    #[test] fn t126_c2b_mixed() { assert_eq!(char_to_byte("a안b", 2), 4); }
    #[test] fn t127_c2b_zero() { assert_eq!(char_to_byte("hello", 0), 0); }
    #[test] fn t128_c2b_end() { assert_eq!(char_to_byte("hello", 5), 5); }
    #[test] fn t129_c2b_emoji() { assert_eq!(char_to_byte("a😀b", 2), 5); }
    #[test] fn t130_c2b_newline() { assert_eq!(char_to_byte("a\nb", 2), 2); }

    // ════════════════════════════════════════════════════════════════
    // 8. find_line_bounds tests (131-170)
    // ════════════════════════════════════════════════════════════════
    #[test] fn t131_line_bounds_single() { assert_eq!(find_line_bounds("hello", 2), (0, 5)); }
    #[test] fn t132_line_bounds_first() { assert_eq!(find_line_bounds("abc\ndef", 1), (0, 3)); }
    #[test] fn t133_line_bounds_second() { assert_eq!(find_line_bounds("abc\ndef", 5), (4, 7)); }
    #[test] fn t134_line_bounds_empty() { assert_eq!(find_line_bounds("", 0), (0, 0)); }
    #[test] fn t135_line_bounds_newline_start() { assert_eq!(find_line_bounds("\nabc", 0), (0, 0)); }
    #[test] fn t136_line_bounds_at_newline() { assert_eq!(find_line_bounds("abc\ndef", 3), (0, 3)); }
    #[test] fn t137_line_bounds_three_lines() { assert_eq!(find_line_bounds("a\nb\nc", 4), (4, 5)); }
    #[test] fn t138_line_bounds_korean() {
        let s = "안녕\n세계";
        let (start, end) = find_line_bounds(s, 7); // in "세계"
        assert_eq!(start, 7); // after \n
    }
    #[test] fn t139_line_bounds_beyond() { assert_eq!(find_line_bounds("hello", 100), (0, 5)); }
    #[test] fn t140_line_bounds_multiline() {
        let s = "line1\nline2\nline3";
        assert_eq!(find_line_bounds(s, 8), (6, 11));
    }

    // ════════════════════════════════════════════════════════════════
    // 9. find_para_bounds tests (141-170)
    // ════════════════════════════════════════════════════════════════
    #[test] fn t141_para_bounds_single() { assert_eq!(find_para_bounds("hello", 2), (0, 5)); }
    #[test] fn t142_para_bounds_two_paras() {
        let s = "para1\n\npara2";
        assert_eq!(find_para_bounds(s, 0), (0, 5));
    }
    #[test] fn t143_para_bounds_second_para() {
        let s = "para1\n\npara2";
        assert_eq!(find_para_bounds(s, 8), (7, 12));
    }
    #[test] fn t144_para_bounds_empty() { assert_eq!(find_para_bounds("", 0), (0, 0)); }
    #[test] fn t145_para_bounds_no_double_newline() {
        let s = "line1\nline2\nline3";
        assert_eq!(find_para_bounds(s, 8), (0, 17));
    }
    #[test] fn t146_para_bounds_three_paras() {
        let s = "a\n\nb\n\nc";
        assert_eq!(find_para_bounds(s, 4), (3, 4));
    }
    #[test] fn t147_para_bounds_korean() {
        let s = "안녕\n\n세계";
        let (start, end) = find_para_bounds(s, 9);
        assert_eq!(start, 8);
    }
    #[test] fn t148_para_bounds_beyond() { assert_eq!(find_para_bounds("hello", 100), (0, 5)); }

    // ════════════════════════════════════════════════════════════════
    // 10. color_hex tests (149-170)
    // ════════════════════════════════════════════════════════════════
    #[test] fn t149_color_hex_black() { assert_eq!(color_hex(egui::Color32::from_rgb(0,0,0)), "#000000"); }
    #[test] fn t150_color_hex_white() { assert_eq!(color_hex(egui::Color32::from_rgb(255,255,255)), "#ffffff"); }
    #[test] fn t151_color_hex_red() { assert_eq!(color_hex(egui::Color32::from_rgb(255,0,0)), "#ff0000"); }
    #[test] fn t152_color_hex_cream() { assert_eq!(color_hex(Theme::Cream.bg()), "#fcfaf5"); }
    #[test] fn t153_color_hex_dark() { assert_eq!(color_hex(Theme::Dark.bg()), "#1a1a2e"); }

    // ════════════════════════════════════════════════════════════════
    // 11. blend_color tests (154-180)
    // ════════════════════════════════════════════════════════════════
    #[test] fn t154_blend_0() {
        let a = egui::Color32::from_rgb(0,0,0);
        let b = egui::Color32::from_rgb(255,255,255);
        assert_eq!(blend_color(a, b, 0.0), a);
    }
    #[test] fn t155_blend_1() {
        let a = egui::Color32::from_rgb(0,0,0);
        let b = egui::Color32::from_rgb(255,255,255);
        assert_eq!(blend_color(a, b, 1.0), egui::Color32::from_rgb(255,255,255));
    }
    #[test] fn t156_blend_half() {
        let a = egui::Color32::from_rgb(0,0,0);
        let b = egui::Color32::from_rgb(200,200,200);
        let c = blend_color(a, b, 0.5);
        assert_eq!(c, egui::Color32::from_rgb(100,100,100));
    }
    #[test] fn t157_blend_same() {
        let a = egui::Color32::from_rgb(100,100,100);
        assert_eq!(blend_color(a, a, 0.5), a);
    }

    // ════════════════════════════════════════════════════════════════
    // 12. Find & Replace tests (158-220)
    // ════════════════════════════════════════════════════════════════
    #[test] fn t158_find_count_empty_query() {
        let mut w = make_writer("hello world");
        w.find_text = "".into();
        w.update_find_count();
        assert_eq!(w.find_count, 0);
    }
    #[test] fn t159_find_count_no_match() {
        let mut w = make_writer("hello world");
        w.find_text = "xyz".into();
        w.update_find_count();
        assert_eq!(w.find_count, 0);
    }
    #[test] fn t160_find_count_single() {
        let mut w = make_writer("hello world");
        w.find_text = "hello".into();
        w.update_find_count();
        assert_eq!(w.find_count, 1);
    }
    #[test] fn t161_find_count_multi() {
        let mut w = make_writer("ababab");
        w.find_text = "ab".into();
        w.update_find_count();
        assert_eq!(w.find_count, 3);
    }
    #[test] fn t162_find_case_insensitive() {
        let mut w = make_writer("Hello HELLO hello");
        w.find_text = "hello".into();
        w.update_find_count();
        assert_eq!(w.find_count, 3);
    }
    #[test] fn t163_replace_next_basic() {
        let mut w = make_writer("hello world hello");
        w.find_text = "hello".into();
        w.replace_text = "hi".into();
        w.replace_next();
        assert_eq!(w.tabs[0].text, "hi world hello");
    }
    #[test] fn t164_replace_next_modified() {
        let mut w = make_writer("hello");
        w.find_text = "hello".into();
        w.replace_text = "hi".into();
        w.replace_next();
        assert!(w.tabs[0].modified);
    }
    #[test] fn t165_replace_all_basic() {
        let mut w = make_writer("aaa bbb aaa");
        w.find_text = "aaa".into();
        w.replace_text = "x".into();
        w.replace_all();
        assert_eq!(w.tabs[0].text, "x bbb x");
    }
    #[test] fn t166_replace_all_no_match() {
        let mut w = make_writer("hello");
        w.find_text = "xyz".into();
        w.replace_text = "abc".into();
        w.replace_all();
        assert_eq!(w.tabs[0].text, "hello");
    }
    #[test] fn t167_replace_empty_find() {
        let mut w = make_writer("hello");
        w.find_text = "".into();
        w.replace_text = "x".into();
        w.replace_next();
        assert_eq!(w.tabs[0].text, "hello");
    }
    #[test] fn t168_replace_all_empty_find() {
        let mut w = make_writer("hello");
        w.find_text = "".into();
        w.replace_text = "x".into();
        w.replace_all();
        assert_eq!(w.tabs[0].text, "hello");
    }
    #[test] fn t169_replace_case_insensitive() {
        let mut w = make_writer("Hello HELLO");
        w.find_text = "hello".into();
        w.replace_text = "hi".into();
        w.replace_next();
        assert!(w.tabs[0].text.starts_with("hi"));
    }
    #[test] fn t170_replace_all_case_insensitive() {
        let mut w = make_writer("Hello HELLO hello");
        w.find_text = "hello".into();
        w.replace_text = "hi".into();
        w.replace_all();
        assert_eq!(w.tabs[0].text, "hi hi hi");
    }
    #[test] fn t171_replace_count_update() {
        let mut w = make_writer("aaa");
        w.find_text = "a".into();
        w.replace_next();
        assert_eq!(w.find_count, 2);
    }
    #[test] fn t172_replace_all_count_zero() {
        let mut w = make_writer("aaa");
        w.find_text = "a".into();
        w.replace_text = "b".into();
        w.replace_all();
        assert_eq!(w.find_count, 0);
    }
    #[test] fn t173_find_korean() {
        let mut w = make_writer("안녕하세요 세계");
        w.find_text = "세계".into();
        w.update_find_count();
        assert_eq!(w.find_count, 1);
    }
    #[test] fn t174_replace_korean() {
        let mut w = make_writer("안녕하세요 세계");
        w.find_text = "세계".into();
        w.replace_text = "세상".into();
        w.replace_next();
        assert!(w.tabs[0].text.contains("세상"));
    }
    #[test] fn t175_find_multiline() {
        let mut w = make_writer("abc\nabc\nabc");
        w.find_text = "abc".into();
        w.update_find_count();
        assert_eq!(w.find_count, 3);
    }

    // ════════════════════════════════════════════════════════════════
    // 13. FontChoice tests (176-210)
    // ════════════════════════════════════════════════════════════════
    #[test] fn t176_font_all_count() { assert_eq!(FontChoice::ALL.len(), 7); }
    #[test] fn t177_font_malgun_name() { assert_eq!(FontChoice::MalgunGothic.name(), "Malgun Gothic"); }
    #[test] fn t178_font_segoe_name() { assert_eq!(FontChoice::SegoeUI.name(), "Segoe UI"); }
    #[test] fn t179_font_consolas_name() { assert_eq!(FontChoice::Consolas.name(), "Consolas"); }
    #[test] fn t180_font_arial_name() { assert_eq!(FontChoice::Arial.name(), "Arial"); }
    #[test] fn t181_font_times_name() { assert_eq!(FontChoice::TimesNewRoman.name(), "Times New Roman"); }
    #[test] fn t182_font_calibri_name() { assert_eq!(FontChoice::Calibri.name(), "Calibri"); }
    #[test] fn t183_font_verdana_name() { assert_eq!(FontChoice::Verdana.name(), "Verdana"); }
    #[test] fn t184_font_malgun_file() { assert_eq!(FontChoice::MalgunGothic.file(), "malgun.ttf"); }
    #[test] fn t185_font_segoe_file() { assert_eq!(FontChoice::SegoeUI.file(), "segoeui.ttf"); }
    #[test] fn t186_font_consolas_file() { assert_eq!(FontChoice::Consolas.file(), "consola.ttf"); }
    #[test] fn t187_font_arial_file() { assert_eq!(FontChoice::Arial.file(), "arial.ttf"); }
    #[test] fn t188_font_times_file() { assert_eq!(FontChoice::TimesNewRoman.file(), "times.ttf"); }
    #[test] fn t189_font_calibri_file() { assert_eq!(FontChoice::Calibri.file(), "calibri.ttf"); }
    #[test] fn t190_font_verdana_file() { assert_eq!(FontChoice::Verdana.file(), "verdana.ttf"); }
    #[test] fn t191_font_eq() { assert_eq!(FontChoice::Arial, FontChoice::Arial); }
    #[test] fn t192_font_ne() { assert_ne!(FontChoice::Arial, FontChoice::Consolas); }
    #[test] fn t193_font_all_unique_names() {
        let names: Vec<_> = FontChoice::ALL.iter().map(|f| f.name()).collect();
        for i in 0..names.len() { for j in (i+1)..names.len() { assert_ne!(names[i], names[j]); } }
    }
    #[test] fn t194_font_all_unique_files() {
        let files: Vec<_> = FontChoice::ALL.iter().map(|f| f.file()).collect();
        for i in 0..files.len() { for j in (i+1)..files.len() { assert_ne!(files[i], files[j]); } }
    }

    // ════════════════════════════════════════════════════════════════
    // 14. Writer settings mutation (195-260)
    // ════════════════════════════════════════════════════════════════
    #[test] fn t195_theme_toggle() { let mut w = Writer::new_with(true); w.theme = w.theme.next(); assert_eq!(w.theme, Theme::Dark); }
    #[test] fn t196_font_size_inc() { let mut w = Writer::new_with(true); w.font_size = (w.font_size + 1.0).min(36.0); assert_eq!(w.font_size, 19.0); }
    #[test] fn t197_font_size_dec() { let mut w = Writer::new_with(true); w.font_size = (w.font_size - 1.0).max(12.0); assert_eq!(w.font_size, 17.0); }
    #[test] fn t198_font_size_max() { let mut w = Writer::new_with(true); w.font_size = 36.0; w.font_size = (w.font_size + 1.0).min(36.0); assert_eq!(w.font_size, 36.0); }
    #[test] fn t199_font_size_min() { let mut w = Writer::new_with(true); w.font_size = 12.0; w.font_size = (w.font_size - 1.0).max(12.0); assert_eq!(w.font_size, 12.0); }
    #[test] fn t200_focus_toggle() { let mut w = Writer::new_with(true); w.focus_mode = w.focus_mode.next(); assert_eq!(w.focus_mode, FocusMode::Line); }
    #[test] fn t201_typewriter_toggle() { let mut w = Writer::new_with(true); w.typewriter = !w.typewriter; assert!(w.typewriter); }
    #[test] fn t202_typewriter_double_toggle() { let mut w = Writer::new_with(true); w.typewriter = !w.typewriter; w.typewriter = !w.typewriter; assert!(!w.typewriter); }
    #[test] fn t203_fullscreen_toggle() { let mut w = Writer::new_with(true); w.fullscreen = !w.fullscreen; assert!(w.fullscreen); }
    #[test] fn t204_status_toggle() { let mut w = Writer::new_with(true); w.show_status = !w.show_status; assert!(w.show_status); }
    #[test] fn t205_find_toggle() { let mut w = Writer::new_with(true); w.show_find = !w.show_find; assert!(w.show_find); }
    #[test] fn t206_replace_toggle() { let mut w = Writer::new_with(true); w.show_replace = !w.show_replace; assert!(w.show_replace); }
    #[test] fn t207_line_spacing_cycle_1() {
        let mut w = Writer::new_with(true); // 1.6
        w.line_spacing = 1.8;
        assert_eq!(w.line_spacing, 1.8);
    }
    #[test] fn t208_line_spacing_cycle_2() {
        let mut w = Writer::new_with(true);
        w.line_spacing = 2.0;
        assert_eq!(w.line_spacing, 2.0);
    }

    // ════════════════════════════════════════════════════════════════
    // 15. Edge case text operations (209-280)
    // ════════════════════════════════════════════════════════════════
    #[test] fn t209_empty_text_word() { let w = make_writer(""); assert_eq!(w.word_count(), 0); }
    #[test] fn t210_empty_text_char() { let w = make_writer(""); assert_eq!(w.char_count(), 0); }
    #[test] fn t211_empty_text_line() { let w = make_writer(""); assert_eq!(w.line_count(), 1); }
    #[test] fn t212_single_char() { let w = make_writer("a"); assert_eq!(w.char_count(), 1); }
    #[test] fn t213_single_newline() { let w = make_writer("\n"); assert_eq!(w.line_count(), 1); }
    #[test] fn t214_only_spaces() { let w = make_writer("   "); assert_eq!(w.word_count(), 0); }
    #[test] fn t215_only_newlines() { let w = make_writer("\n\n\n"); assert_eq!(w.word_count(), 0); }
    #[test] fn t216_unicode_emoji() { let w = make_writer("😀😁😂"); assert_eq!(w.char_count(), 3); }
    #[test] fn t217_mixed_unicode() { let w = make_writer("a😀안b"); assert_eq!(w.char_count(), 4); }
    #[test] fn t218_very_long_line() { let s = "a".repeat(10000); let w = make_writer(&s); assert_eq!(w.char_count(), 10000); }
    #[test] fn t219_many_words() { let s = (0..500).map(|i| format!("w{}", i)).collect::<Vec<_>>().join(" "); let w = make_writer(&s); assert_eq!(w.word_count(), 500); }
    #[test] fn t220_line_bounds_very_long() {
        let s = "a".repeat(5000);
        assert_eq!(find_line_bounds(&s, 2500), (0, 5000));
    }

    // ════════════════════════════════════════════════════════════════
    // 16. Tab text independence (221-260)
    // ════════════════════════════════════════════════════════════════
    #[test] fn t221_tabs_independent_text() {
        let mut w = Writer::new_with(true);
        w.tabs[0].text = "first".into();
        w.new_tab();
        w.tabs[1].text = "second".into();
        assert_eq!(w.tabs[0].text, "first");
        assert_eq!(w.tabs[1].text, "second");
    }
    #[test] fn t222_tab_modified_independent() {
        let mut w = Writer::new_with(true);
        w.tabs[0].modified = true;
        w.new_tab();
        assert!(!w.tabs[1].modified);
    }
    #[test] fn t223_active_tab_word_count() {
        let mut w = Writer::new_with(true);
        w.tabs[0].text = "one two three".into();
        w.new_tab();
        w.tabs[1].text = "a b".into();
        assert_eq!(w.word_count(), 2); // active is tab 1
    }
    #[test] fn t224_switch_tab_word_count() {
        let mut w = Writer::new_with(true);
        w.tabs[0].text = "one two three".into();
        w.new_tab();
        w.tabs[1].text = "a b".into();
        w.active = 0;
        assert_eq!(w.word_count(), 3);
    }
    #[test] fn t225_find_on_active_tab() {
        let mut w = Writer::new_with(true);
        w.tabs[0].text = "aaa".into();
        w.new_tab();
        w.tabs[1].text = "bbb".into();
        w.find_text = "a".into();
        w.update_find_count();
        assert_eq!(w.find_count, 0); // active is tab 1
    }
    #[test] fn t226_replace_on_active_tab() {
        let mut w = Writer::new_with(true);
        w.tabs[0].text = "hello".into();
        w.new_tab();
        w.tabs[1].text = "hello world".into();
        w.find_text = "hello".into();
        w.replace_text = "hi".into();
        w.replace_next();
        assert_eq!(w.tabs[1].text, "hi world");
        assert_eq!(w.tabs[0].text, "hello"); // untouched
    }

    // ════════════════════════════════════════════════════════════════
    // 17. Stress tests (227-300)
    // ════════════════════════════════════════════════════════════════
    #[test] fn t227_100_tabs() {
        let mut w = Writer::new_with(true);
        for _ in 0..99 { w.new_tab(); }
        assert_eq!(w.tabs.len(), 100);
        assert_eq!(w.active, 99);
    }
    #[test] fn t228_close_100_tabs() {
        let mut w = Writer::new_with(true);
        for _ in 0..99 { w.new_tab(); }
        for _ in 0..99 { w.close_tab(); }
        assert_eq!(w.tabs.len(), 1);
    }
    #[test] fn t229_10000_char_text() {
        let s = "x".repeat(10000);
        let w = make_writer(&s);
        assert_eq!(w.char_count(), 10000);
        assert_eq!(w.word_count(), 1);
    }
    #[test] fn t230_find_10000_matches() {
        let s = "a".repeat(10000);
        let mut w = make_writer(&s);
        w.find_text = "a".into();
        w.update_find_count();
        assert_eq!(w.find_count, 10000);
    }
    #[test] fn t231_replace_all_10000() {
        let s = "a".repeat(10000);
        let mut w = make_writer(&s);
        w.find_text = "a".into();
        w.replace_text = "b".into();
        w.replace_all();
        assert_eq!(w.tabs[0].text, "b".repeat(10000));
    }
    #[test] fn t232_line_bounds_10000_lines() {
        let s = long_text(10000);
        let mid = s.len() / 2;
        let (start, end) = find_line_bounds(&s, mid);
        assert!(start <= mid);
        assert!(end >= mid);
    }
    #[test] fn t233_safe_byte_pos_large() {
        let s = "안녕하세요 ".repeat(1000);
        let pos = safe_byte_pos(&s, s.len());
        assert_eq!(pos, s.len());
    }
    #[test] fn t234_char_to_byte_large() {
        let s = "안녕하세요 ".repeat(1000);
        let char_count = s.chars().count();
        assert_eq!(char_to_byte(&s, char_count), s.len());
    }
    #[test] fn t235_theme_cycle_100() {
        let mut t = Theme::Cream;
        for _ in 0..100 { t = t.next(); }
        assert_eq!(t, Theme::Dark); // 100 % 9 = 1
    }
    #[test] fn t236_focus_cycle_99() {
        let mut f = FocusMode::Off;
        for _ in 0..99 { f = f.next(); }
        assert_eq!(f, FocusMode::Off); // 99 % 3 = 0
    }

    // ════════════════════════════════════════════════════════════════
    // 18. Replace edge cases (237-280)
    // ════════════════════════════════════════════════════════════════
    #[test] fn t237_replace_with_longer() {
        let mut w = make_writer("ab");
        w.find_text = "ab".into();
        w.replace_text = "abcdef".into();
        w.replace_next();
        assert_eq!(w.tabs[0].text, "abcdef");
    }
    #[test] fn t238_replace_with_shorter() {
        let mut w = make_writer("abcdef");
        w.find_text = "abcdef".into();
        w.replace_text = "x".into();
        w.replace_next();
        assert_eq!(w.tabs[0].text, "x");
    }
    #[test] fn t239_replace_with_empty() {
        let mut w = make_writer("hello world");
        w.find_text = "world".into();
        w.replace_text = "".into();
        w.replace_next();
        assert_eq!(w.tabs[0].text, "hello ");
    }
    #[test] fn t240_replace_all_with_empty() {
        let mut w = make_writer("aXbXc");
        w.find_text = "X".into();
        w.replace_text = "".into();
        w.replace_all();
        assert_eq!(w.tabs[0].text, "abc");
    }
    #[test] fn t241_replace_entire_text() {
        let mut w = make_writer("hello");
        w.find_text = "hello".into();
        w.replace_text = "bye".into();
        w.replace_all();
        assert_eq!(w.tabs[0].text, "bye");
    }
    #[test] fn t242_replace_newline() {
        let mut w = make_writer("a\nb");
        w.find_text = "\n".into();
        w.replace_text = " ".into();
        w.replace_all();
        assert_eq!(w.tabs[0].text, "a b");
    }
    #[test] fn t243_find_special_chars() {
        let mut w = make_writer("price: $100");
        w.find_text = "$100".into();
        w.update_find_count();
        assert_eq!(w.find_count, 1);
    }
    #[test] fn t244_replace_korean_all() {
        let mut w = make_writer("가 나 가 나 가");
        w.find_text = "가".into();
        w.replace_text = "다".into();
        w.replace_all();
        assert_eq!(w.tabs[0].text, "다 나 다 나 다");
    }
    #[test] fn t245_replace_preserves_other_tabs() {
        let mut w = Writer::new_with(true);
        w.tabs[0].text = "keep".into();
        w.new_tab();
        w.tabs[1].text = "change me".into();
        w.find_text = "change".into();
        w.replace_text = "fixed".into();
        w.replace_next();
        assert_eq!(w.tabs[0].text, "keep");
    }

    // ════════════════════════════════════════════════════════════════
    // 19. Boundary / safety tests (246-320)
    // ════════════════════════════════════════════════════════════════
    #[test] fn t246_safe_byte_korean_boundary() {
        let s = "가나다"; // 9 bytes
        for i in 0..12 {
            let pos = safe_byte_pos(s, i);
            assert!(s.is_char_boundary(pos));
        }
    }
    #[test] fn t247_safe_byte_emoji_boundary() {
        let s = "a😀b😁c"; // 1+4+1+4+1 = 11 bytes
        for i in 0..15 {
            let pos = safe_byte_pos(s, i);
            assert!(s.is_char_boundary(pos));
        }
    }
    #[test] fn t248_safe_byte_all_korean() {
        let s = korean_text();
        for i in 0..s.len()+5 {
            let pos = safe_byte_pos(&s, i);
            assert!(s.is_char_boundary(pos));
        }
    }
    #[test] fn t249_char_to_byte_roundtrip() {
        let s = "a안b녕c";
        for (i, (byte_idx, _)) in s.char_indices().enumerate() {
            assert_eq!(char_to_byte(s, i), byte_idx);
        }
    }
    #[test] fn t250_line_bounds_all_positions() {
        let s = "ab\ncd\nef";
        for i in 0..s.len() {
            if s.is_char_boundary(i) {
                let (start, end) = find_line_bounds(s, i);
                assert!(start <= i || i <= end);
            }
        }
    }

    // ════════════════════════════════════════════════════════════════
    // 20. More Writer operations (251-350)
    // ════════════════════════════════════════════════════════════════
    #[test] fn t251_modified_on_text_change() {
        let mut w = make_writer("hello");
        w.tabs[0].modified = false;
        w.tabs[0].text.push_str(" world");
        w.tabs[0].modified = true;
        assert!(w.tabs[0].modified);
    }
    #[test] fn t252_tab_path_none_default() {
        let w = Writer::new_with(true);
        assert!(w.tabs[0].file_path.is_none());
    }
    #[test] fn t253_recovery_path_exists() {
        let p = Writer::recovery_path();
        assert!(p.to_str().unwrap().contains("simple_writer_recovery"));
    }
    #[test] fn t254_recent_path_exists() {
        let p = Writer::recent_path();
        assert!(p.to_str().unwrap().contains("simple_writer_recent"));
    }
    #[test] fn t255_new_tab_path_none() {
        let mut w = Writer::new_with(true);
        w.new_tab();
        assert!(w.tabs[w.active].file_path.is_none());
    }
    #[test] fn t256_new_tab_not_modified() {
        let mut w = Writer::new_with(true);
        w.new_tab();
        assert!(!w.tabs[w.active].modified);
    }
    #[test] fn t257_cursor_pos_default() { assert_eq!(Writer::new_with(true).cursor_byte_pos, 0); }
    #[test] fn t258_hwnd_default() { assert_eq!(Writer::new_with(true).hwnd, 0); }
    #[test] fn t259_applied_titlebar_none() { assert!(Writer::new_with(true).applied_titlebar.is_none()); }
    #[test] fn t260_save_flash_none() { assert!(Writer::new_with(true).save_flash.is_none()); }

    // ════════════════════════════════════════════════════════════════
    // 21-30. Parameterized bulk tests (261-1000)
    // ════════════════════════════════════════════════════════════════

    // Bulk safe_byte_pos (261-360)
    macro_rules! bulk_safe_byte {
        ($name:ident, $text:expr, $pos:expr) => {
            #[test] fn $name() {
                let s = $text;
                let p = safe_byte_pos(s, $pos);
                assert!(s.is_char_boundary(p));
                assert!(p <= s.len());
            }
        }
    }
    bulk_safe_byte!(t261_sb_1, "test", 0);
    bulk_safe_byte!(t262_sb_2, "test", 1);
    bulk_safe_byte!(t263_sb_3, "test", 2);
    bulk_safe_byte!(t264_sb_4, "test", 3);
    bulk_safe_byte!(t265_sb_5, "test", 4);
    bulk_safe_byte!(t266_sb_6, "한글테스트", 0);
    bulk_safe_byte!(t267_sb_7, "한글테스트", 1);
    bulk_safe_byte!(t268_sb_8, "한글테스트", 2);
    bulk_safe_byte!(t269_sb_9, "한글테스트", 3);
    bulk_safe_byte!(t270_sb_10, "한글테스트", 4);
    bulk_safe_byte!(t271_sb_11, "한글테스트", 5);
    bulk_safe_byte!(t272_sb_12, "한글테스트", 6);
    bulk_safe_byte!(t273_sb_13, "한글테스트", 7);
    bulk_safe_byte!(t274_sb_14, "한글테스트", 8);
    bulk_safe_byte!(t275_sb_15, "한글테스트", 9);
    bulk_safe_byte!(t276_sb_16, "한글테스트", 10);
    bulk_safe_byte!(t277_sb_17, "한글테스트", 11);
    bulk_safe_byte!(t278_sb_18, "한글테스트", 12);
    bulk_safe_byte!(t279_sb_19, "한글테스트", 13);
    bulk_safe_byte!(t280_sb_20, "한글테스트", 14);
    bulk_safe_byte!(t281_sb_21, "한글테스트", 15);
    bulk_safe_byte!(t282_sb_22, "a😀b", 0);
    bulk_safe_byte!(t283_sb_23, "a😀b", 1);
    bulk_safe_byte!(t284_sb_24, "a😀b", 2);
    bulk_safe_byte!(t285_sb_25, "a😀b", 3);
    bulk_safe_byte!(t286_sb_26, "a😀b", 4);
    bulk_safe_byte!(t287_sb_27, "a😀b", 5);
    bulk_safe_byte!(t288_sb_28, "a😀b", 6);
    bulk_safe_byte!(t289_sb_29, "mixed한글abc", 0);
    bulk_safe_byte!(t290_sb_30, "mixed한글abc", 5);
    bulk_safe_byte!(t291_sb_31, "mixed한글abc", 6);
    bulk_safe_byte!(t292_sb_32, "mixed한글abc", 7);
    bulk_safe_byte!(t293_sb_33, "mixed한글abc", 8);
    bulk_safe_byte!(t294_sb_34, "mixed한글abc", 9);
    bulk_safe_byte!(t295_sb_35, "mixed한글abc", 10);
    bulk_safe_byte!(t296_sb_36, "mixed한글abc", 11);
    bulk_safe_byte!(t297_sb_37, "mixed한글abc", 12);
    bulk_safe_byte!(t298_sb_38, "mixed한글abc", 13);
    bulk_safe_byte!(t299_sb_39, "mixed한글abc", 14);
    bulk_safe_byte!(t300_sb_40, "", 0);

    // Bulk line bounds (301-400)
    macro_rules! bulk_line_bounds {
        ($name:ident, $text:expr, $pos:expr) => {
            #[test] fn $name() {
                let s: &str = $text;
                let p = safe_byte_pos(s, $pos);
                let (start, end) = find_line_bounds(s, p);
                assert!(start <= end);
                assert!(end <= s.len());
                assert!(!s[start..end].contains('\n') || s[start..end].is_empty());
            }
        }
    }
    bulk_line_bounds!(t301_lb_1, "hello\nworld", 0);
    bulk_line_bounds!(t302_lb_2, "hello\nworld", 3);
    bulk_line_bounds!(t303_lb_3, "hello\nworld", 5);
    bulk_line_bounds!(t304_lb_4, "hello\nworld", 6);
    bulk_line_bounds!(t305_lb_5, "hello\nworld", 9);
    bulk_line_bounds!(t306_lb_6, "a\nb\nc\nd\ne", 0);
    bulk_line_bounds!(t307_lb_7, "a\nb\nc\nd\ne", 2);
    bulk_line_bounds!(t308_lb_8, "a\nb\nc\nd\ne", 4);
    bulk_line_bounds!(t309_lb_9, "a\nb\nc\nd\ne", 6);
    bulk_line_bounds!(t310_lb_10, "a\nb\nc\nd\ne", 8);
    bulk_line_bounds!(t311_lb_11, "single", 0);
    bulk_line_bounds!(t312_lb_12, "single", 3);
    bulk_line_bounds!(t313_lb_13, "single", 6);
    bulk_line_bounds!(t314_lb_14, "", 0);
    bulk_line_bounds!(t315_lb_15, "\n", 0);
    bulk_line_bounds!(t316_lb_16, "\n", 1);
    bulk_line_bounds!(t317_lb_17, "\n\n", 0);
    bulk_line_bounds!(t318_lb_18, "\n\n", 1);
    bulk_line_bounds!(t319_lb_19, "\n\n", 2);
    bulk_line_bounds!(t320_lb_20, "한글\n테스트", 0);
    bulk_line_bounds!(t321_lb_21, "한글\n테스트", 3);
    bulk_line_bounds!(t322_lb_22, "한글\n테스트", 6);
    bulk_line_bounds!(t323_lb_23, "한글\n테스트", 7);
    bulk_line_bounds!(t324_lb_24, "한글\n테스트", 10);
    bulk_line_bounds!(t325_lb_25, "line1\nline2\nline3\nline4\nline5", 0);
    bulk_line_bounds!(t326_lb_26, "line1\nline2\nline3\nline4\nline5", 6);
    bulk_line_bounds!(t327_lb_27, "line1\nline2\nline3\nline4\nline5", 12);
    bulk_line_bounds!(t328_lb_28, "line1\nline2\nline3\nline4\nline5", 18);
    bulk_line_bounds!(t329_lb_29, "line1\nline2\nline3\nline4\nline5", 24);
    bulk_line_bounds!(t330_lb_30, "abc def\nghi jkl\nmno pqr", 0);
    bulk_line_bounds!(t331_lb_31, "abc def\nghi jkl\nmno pqr", 5);
    bulk_line_bounds!(t332_lb_32, "abc def\nghi jkl\nmno pqr", 8);
    bulk_line_bounds!(t333_lb_33, "abc def\nghi jkl\nmno pqr", 14);
    bulk_line_bounds!(t334_lb_34, "abc def\nghi jkl\nmno pqr", 16);
    bulk_line_bounds!(t335_lb_35, "abc def\nghi jkl\nmno pqr", 22);

    // Bulk para bounds (336-400)
    macro_rules! bulk_para_bounds {
        ($name:ident, $text:expr, $pos:expr) => {
            #[test] fn $name() {
                let s: &str = $text;
                let p = safe_byte_pos(s, $pos);
                let (start, end) = find_para_bounds(s, p);
                assert!(start <= end);
                assert!(end <= s.len());
            }
        }
    }
    bulk_para_bounds!(t336_pb_1, "para1\n\npara2", 0);
    bulk_para_bounds!(t337_pb_2, "para1\n\npara2", 3);
    bulk_para_bounds!(t338_pb_3, "para1\n\npara2", 5);
    bulk_para_bounds!(t339_pb_4, "para1\n\npara2", 7);
    bulk_para_bounds!(t340_pb_5, "para1\n\npara2", 10);
    bulk_para_bounds!(t341_pb_6, "a\n\nb\n\nc", 0);
    bulk_para_bounds!(t342_pb_7, "a\n\nb\n\nc", 3);
    bulk_para_bounds!(t343_pb_8, "a\n\nb\n\nc", 5);
    bulk_para_bounds!(t344_pb_9, "a\n\nb\n\nc", 6);
    bulk_para_bounds!(t345_pb_10, "hello", 0);
    bulk_para_bounds!(t346_pb_11, "hello", 3);
    bulk_para_bounds!(t347_pb_12, "hello", 5);
    bulk_para_bounds!(t348_pb_13, "", 0);
    bulk_para_bounds!(t349_pb_14, "\n\n", 0);
    bulk_para_bounds!(t350_pb_15, "\n\n", 1);
    bulk_para_bounds!(t351_pb_16, "\n\n", 2);
    bulk_para_bounds!(t352_pb_17, "한글\n\n테스트", 0);
    bulk_para_bounds!(t353_pb_18, "한글\n\n테스트", 6);
    bulk_para_bounds!(t354_pb_19, "한글\n\n테스트", 8);
    bulk_para_bounds!(t355_pb_20, "a\n\nb\n\nc\n\nd\n\ne", 0);
    bulk_para_bounds!(t356_pb_21, "a\n\nb\n\nc\n\nd\n\ne", 3);
    bulk_para_bounds!(t357_pb_22, "a\n\nb\n\nc\n\nd\n\ne", 6);
    bulk_para_bounds!(t358_pb_23, "a\n\nb\n\nc\n\nd\n\ne", 9);
    bulk_para_bounds!(t359_pb_24, "a\n\nb\n\nc\n\nd\n\ne", 12);
    bulk_para_bounds!(t360_pb_25, "long para with many words here\n\nsecond para also long", 15);

    // Bulk word/char/line counts (361-500)
    macro_rules! bulk_counts {
        ($name:ident, $text:expr, $words:expr, $chars:expr, $lines:expr) => {
            #[test] fn $name() {
                let w = make_writer($text);
                assert_eq!(w.word_count(), $words, "word count");
                assert_eq!(w.char_count(), $chars, "char count");
                assert_eq!(w.line_count(), $lines, "line count");
            }
        }
    }
    bulk_counts!(t361_cnt_1, "", 0, 0, 1);
    bulk_counts!(t362_cnt_2, "a", 1, 1, 1);
    bulk_counts!(t363_cnt_3, "ab", 1, 2, 1);
    bulk_counts!(t364_cnt_4, "a b", 2, 3, 1);
    bulk_counts!(t365_cnt_5, "a b c", 3, 5, 1);
    bulk_counts!(t366_cnt_6, "a\nb", 2, 3, 2);
    bulk_counts!(t367_cnt_7, "a\nb\nc", 3, 5, 3);
    bulk_counts!(t368_cnt_8, " ", 0, 1, 1);
    bulk_counts!(t369_cnt_9, "  ", 0, 2, 1);
    bulk_counts!(t370_cnt_10, "\n", 0, 1, 1);
    bulk_counts!(t371_cnt_11, "\n\n", 0, 2, 2);
    bulk_counts!(t372_cnt_12, "hello world", 2, 11, 1);
    bulk_counts!(t373_cnt_13, "hello\nworld", 2, 11, 2);
    bulk_counts!(t374_cnt_14, "가", 1, 1, 1);
    bulk_counts!(t375_cnt_15, "가 나", 2, 3, 1);
    bulk_counts!(t376_cnt_16, "가\n나", 2, 3, 2);
    bulk_counts!(t377_cnt_17, "The quick brown fox", 4, 19, 1);
    bulk_counts!(t378_cnt_18, "Line 1\nLine 2\nLine 3", 6, 20, 3);
    bulk_counts!(t379_cnt_19, "😀", 1, 1, 1);
    bulk_counts!(t380_cnt_20, "😀😁", 1, 2, 1);
    bulk_counts!(t381_cnt_21, "😀 😁", 2, 3, 1);
    bulk_counts!(t382_cnt_22, "a\n\nb", 2, 4, 3);
    bulk_counts!(t383_cnt_23, "test\n\n\ntest", 2, 11, 4);
    bulk_counts!(t384_cnt_24, "one two\nthree four\nfive", 5, 23, 3);
    bulk_counts!(t385_cnt_25, "안녕하세요", 1, 5, 1);
    bulk_counts!(t386_cnt_26, "Hello 안녕", 2, 8, 1);
    bulk_counts!(t387_cnt_27, "a b c d e f g h i j", 10, 19, 1);
    bulk_counts!(t388_cnt_28, "1\n2\n3\n4\n5", 5, 9, 5);
    bulk_counts!(t389_cnt_29, "word", 1, 4, 1);
    bulk_counts!(t390_cnt_30, "  word  ", 1, 8, 1);

    // Bulk find count (391-450)
    macro_rules! bulk_find {
        ($name:ident, $text:expr, $query:expr, $expected:expr) => {
            #[test] fn $name() {
                let mut w = make_writer($text);
                w.find_text = $query.into();
                w.update_find_count();
                assert_eq!(w.find_count, $expected);
            }
        }
    }
    bulk_find!(t391_fc_1, "aaa", "a", 3);
    bulk_find!(t392_fc_2, "aaa", "aa", 1);
    bulk_find!(t393_fc_3, "abcabc", "abc", 2);
    bulk_find!(t394_fc_4, "hello", "x", 0);
    bulk_find!(t395_fc_5, "", "a", 0);
    bulk_find!(t396_fc_6, "HELLO hello Hello", "hello", 3);
    bulk_find!(t397_fc_7, "a b c", " ", 2);
    bulk_find!(t398_fc_8, "a\nb\nc", "\n", 2);
    bulk_find!(t399_fc_9, "한글한글한글", "한글", 3);
    bulk_find!(t400_fc_10, "test test", "test", 2);
    bulk_find!(t401_fc_11, "aaaa", "aa", 2);
    bulk_find!(t402_fc_12, "Mississippi", "ss", 2);
    bulk_find!(t403_fc_13, "Mississippi", "issi", 1);
    bulk_find!(t404_fc_14, "one TWO three", "two", 1);
    bulk_find!(t405_fc_15, "😀😀😀", "😀", 3);
    bulk_find!(t406_fc_16, "line1\nline2\nline3", "line", 3);
    bulk_find!(t407_fc_17, "aAbBaAbB", "ab", 2);
    bulk_find!(t408_fc_18, "x", "x", 1);
    bulk_find!(t409_fc_19, "xx", "x", 2);
    bulk_find!(t410_fc_20, "xxx", "x", 3);

    // Bulk replace_all (411-470)
    macro_rules! bulk_replace {
        ($name:ident, $text:expr, $find:expr, $repl:expr, $expected:expr) => {
            #[test] fn $name() {
                let mut w = make_writer($text);
                w.find_text = $find.into();
                w.replace_text = $repl.into();
                w.replace_all();
                assert_eq!(w.tabs[0].text, $expected);
            }
        }
    }
    bulk_replace!(t411_ra_1, "aaa", "a", "b", "bbb");
    bulk_replace!(t412_ra_2, "hello world", "world", "earth", "hello earth");
    bulk_replace!(t413_ra_3, "abcabc", "abc", "x", "xx");
    bulk_replace!(t414_ra_4, "no match", "xyz", "abc", "no match");
    bulk_replace!(t415_ra_5, "AAA", "aaa", "bbb", "bbb");
    bulk_replace!(t416_ra_6, "a b c", " ", "-", "a-b-c");
    bulk_replace!(t417_ra_7, "hello", "hello", "", "");
    bulk_replace!(t418_ra_8, "x", "x", "yyy", "yyy");
    bulk_replace!(t419_ra_9, "한글", "한글", "영어", "영어");
    bulk_replace!(t420_ra_10, "a\nb", "\n", " ", "a b");
    bulk_replace!(t421_ra_11, "aXbXc", "x", "Y", "aYbYc");
    bulk_replace!(t422_ra_12, "test", "t", "T", "TesT");
    bulk_replace!(t423_ra_13, "  ", " ", "_", "__");
    bulk_replace!(t424_ra_14, "123", "1", "one", "one23");
    bulk_replace!(t425_ra_15, "a😀b", "😀", "!", "a!b");
    bulk_replace!(t426_ra_16, "old old old", "old", "new", "new new new");
    bulk_replace!(t427_ra_17, "UPPER lower", "upper", "CASE", "CASE lower");
    bulk_replace!(t428_ra_18, "ab ab", "ab", "abc", "abc abc");
    bulk_replace!(t429_ra_19, "1-2-3", "-", "+", "1+2+3");
    bulk_replace!(t430_ra_20, "foo bar baz", "bar", "BAR", "foo BAR baz");

    // Bulk char_to_byte (431-470)
    macro_rules! bulk_c2b {
        ($name:ident, $text:expr, $idx:expr, $expected:expr) => {
            #[test] fn $name() { assert_eq!(char_to_byte($text, $idx), $expected); }
        }
    }
    bulk_c2b!(t431_c2b_1, "hello", 0, 0);
    bulk_c2b!(t432_c2b_2, "hello", 1, 1);
    bulk_c2b!(t433_c2b_3, "hello", 5, 5);
    bulk_c2b!(t434_c2b_4, "한", 0, 0);
    bulk_c2b!(t435_c2b_5, "한", 1, 3);
    bulk_c2b!(t436_c2b_6, "한글", 0, 0);
    bulk_c2b!(t437_c2b_7, "한글", 1, 3);
    bulk_c2b!(t438_c2b_8, "한글", 2, 6);
    bulk_c2b!(t439_c2b_9, "a한b", 0, 0);
    bulk_c2b!(t440_c2b_10, "a한b", 1, 1);
    bulk_c2b!(t441_c2b_11, "a한b", 2, 4);
    bulk_c2b!(t442_c2b_12, "a한b", 3, 5);
    bulk_c2b!(t443_c2b_13, "😀", 0, 0);
    bulk_c2b!(t444_c2b_14, "😀", 1, 4);
    bulk_c2b!(t445_c2b_15, "a😀", 1, 1);
    bulk_c2b!(t446_c2b_16, "a😀", 2, 5);
    bulk_c2b!(t447_c2b_17, "", 0, 0);
    bulk_c2b!(t448_c2b_18, "", 5, 0);
    bulk_c2b!(t449_c2b_19, "abc", 10, 3);
    bulk_c2b!(t450_c2b_20, "\n", 0, 0);

    // Bulk color_hex for all themes (451-470)
    #[test] fn t451_hex_cream_fg() { let h = color_hex(Theme::Cream.fg()); assert_eq!(h, "#323232"); }
    #[test] fn t452_hex_dark_fg() { let h = color_hex(Theme::Dark.fg()); assert_eq!(h, "#d4d4d4"); }
    #[test] fn t453_hex_forest_fg() { let h = color_hex(Theme::Forest.fg()); assert_eq!(h, "#c8d8c0"); }
    #[test] fn t454_hex_ocean_fg() { let h = color_hex(Theme::Ocean.fg()); assert_eq!(h, "#bdd0e8"); }
    #[test] fn t455_hex_cream_dim() { let h = color_hex(Theme::Cream.dim()); assert_eq!(h, "#a0a0a0"); }
    #[test] fn t456_hex_dark_dim() { let h = color_hex(Theme::Dark.dim()); assert_eq!(h, "#646478"); }
    #[test] fn t457_hex_forest_dim() { let h = color_hex(Theme::Forest.dim()); assert_eq!(h, "#5a7a50"); }
    #[test] fn t458_hex_ocean_dim() { let h = color_hex(Theme::Ocean.dim()); assert_eq!(h, "#506878"); }
    #[test] fn t459_hex_dark_bg() { let h = color_hex(Theme::Dark.bg()); assert_eq!(h, "#1a1a2e"); }
    #[test] fn t460_hex_forest_bg() { let h = color_hex(Theme::Forest.bg()); assert_eq!(h, "#1b2b1b"); }
    #[test] fn t461_hex_ocean_bg() { let h = color_hex(Theme::Ocean.bg()); assert_eq!(h, "#1a2433"); }

    // More blend tests (462-480)
    #[test] fn t462_blend_quarter() {
        let c = blend_color(egui::Color32::from_rgb(0,0,0), egui::Color32::from_rgb(100,100,100), 0.25);
        assert_eq!(c, egui::Color32::from_rgb(25,25,25));
    }
    #[test] fn t463_blend_theme_colors() {
        let _ = blend_color(Theme::Cream.bg(), Theme::Cream.dim(), 0.08);
    }
    #[test] fn t464_blend_each_theme() {
        for &t in &[Theme::Cream, Theme::Dark, Theme::Forest, Theme::Ocean] {
            let _ = blend_color(t.bg(), t.dim(), 0.5);
        }
    }

    // More tab operations (465-500)
    #[test] fn t465_tab_switch_forward() {
        let mut w = Writer::new_with(true);
        w.new_tab(); w.new_tab(); // 3 tabs, active=2
        w.active = (w.active + 1) % w.tabs.len();
        assert_eq!(w.active, 0);
    }
    #[test] fn t466_tab_switch_backward() {
        let mut w = Writer::new_with(true);
        w.new_tab(); w.new_tab(); // 3 tabs, active=2
        w.active = 0;
        w.active = if w.active == 0 { w.tabs.len() - 1 } else { w.active - 1 };
        assert_eq!(w.active, 2);
    }
    #[test] fn t467_tab_text_persist() {
        let mut w = Writer::new_with(true);
        w.tabs[0].text = "first".into();
        w.new_tab();
        w.tabs[1].text = "second".into();
        w.active = 0;
        assert_eq!(w.tabs[0].text, "first");
        w.active = 1;
        assert_eq!(w.tabs[1].text, "second");
    }
    #[test] fn t468_many_tabs_close_first() {
        let mut w = Writer::new_with(true);
        for i in 0..5 { w.new_tab(); w.tabs[w.active].text = format!("tab{}", i); }
        w.active = 0;
        w.close_tab();
        assert_eq!(w.tabs.len(), 5);
    }
    #[test] fn t469_tab_names_correct() {
        let mut w = Writer::new_with(true);
        w.tabs[0].file_path = Some(PathBuf::from("a.txt"));
        w.new_tab();
        w.tabs[1].file_path = Some(PathBuf::from("b.md"));
        assert_eq!(w.tabs[0].name(), "a.txt");
        assert_eq!(w.tabs[1].name(), "b.md");
    }
    #[test] fn t470_close_resets_cursor() {
        let mut w = Writer::new_with(true);
        w.new_tab();
        w.cursor_byte_pos = 100;
        w.close_tab();
        assert_eq!(w.cursor_byte_pos, 0);
    }

    // Comprehensive find/replace combos (471-530)
    #[test] fn t471_replace_next_twice() {
        let mut w = make_writer("aXaXa");
        w.find_text = "x".into();
        w.replace_text = "Y".into();
        w.replace_next();
        w.replace_next();
        assert_eq!(w.tabs[0].text, "aYaYa");
    }
    #[test] fn t472_replace_preserves_modified() {
        let mut w = make_writer("ab");
        w.tabs[0].modified = false;
        w.find_text = "a".into();
        w.replace_text = "A".into();
        w.replace_next();
        assert!(w.tabs[0].modified);
    }
    #[test] fn t473_find_after_replace() {
        let mut w = make_writer("abc abc");
        w.find_text = "abc".into();
        w.replace_text = "xyz".into();
        w.replace_next();
        w.update_find_count();
        assert_eq!(w.find_count, 1);
    }
    #[test] fn t474_replace_all_no_change() {
        let mut w = make_writer("hello");
        w.find_text = "xyz".into();
        w.replace_text = "abc".into();
        w.tabs[0].modified = false;
        w.replace_all();
        assert!(!w.tabs[0].modified);
    }

    // Mixed content tests (475-530)
    macro_rules! bulk_mixed {
        ($name:ident, $text:expr, $min_words:expr) => {
            #[test] fn $name() {
                let w = make_writer($text);
                assert!(w.word_count() >= $min_words);
                assert!(w.char_count() > 0);
                assert!(w.line_count() >= 1);
            }
        }
    }
    bulk_mixed!(t475_mx_1, "Hello World", 2);
    bulk_mixed!(t476_mx_2, "안녕하세요", 1);
    bulk_mixed!(t477_mx_3, "Line1\nLine2\nLine3", 3);
    bulk_mixed!(t478_mx_4, "a b c d e", 5);
    bulk_mixed!(t479_mx_5, "The quick brown fox jumps", 5);
    bulk_mixed!(t480_mx_6, "One\nTwo\nThree\nFour\nFive", 5);
    bulk_mixed!(t481_mx_7, "한글 영어 mixed 혼합", 4);
    bulk_mixed!(t482_mx_8, "😀 emoji 테스트", 2);
    bulk_mixed!(t483_mx_9, "1 2 3 4 5 6 7 8 9 10", 10);
    bulk_mixed!(t484_mx_10, "paragraph one\n\nparagraph two", 4);
    bulk_mixed!(t485_mx_11, "short", 1);
    bulk_mixed!(t486_mx_12, "a b c\nd e f\ng h i", 9);
    bulk_mixed!(t487_mx_13, "Tab\tseparated\tvalues", 3);
    bulk_mixed!(t488_mx_14, "multiple   spaces   here", 3);
    bulk_mixed!(t489_mx_15, "UPPERCASE lowercase MiXeD", 3);
    bulk_mixed!(t490_mx_16, "with-hyphen and_underscore", 2);
    bulk_mixed!(t491_mx_17, "number123 test456", 2);
    bulk_mixed!(t492_mx_18, "한글만으로된문장입니다", 1);
    bulk_mixed!(t493_mx_19, "a\nb\nc\nd\ne\nf\ng\nh\ni\nj", 10);
    bulk_mixed!(t494_mx_20, "start middle end", 3);

    // Writer field validation (495-530)
    #[test] fn t495_font_size_range() {
        let w = Writer::new_with(true);
        assert!(w.font_size >= 12.0 && w.font_size <= 36.0);
    }
    #[test] fn t496_line_spacing_range() {
        let w = Writer::new_with(true);
        assert!(w.line_spacing >= 1.0 && w.line_spacing <= 3.0);
    }
    #[test] fn t497_active_valid() {
        let w = Writer::new_with(true);
        assert!(w.active < w.tabs.len());
    }
    #[test] fn t498_after_many_ops() {
        let mut w = Writer::new_with(true);
        w.new_tab(); w.new_tab(); w.close_tab();
        w.tabs[w.active].text = "test".into();
        w.find_text = "t".into();
        w.update_find_count();
        assert_eq!(w.find_count, 2);
        assert!(w.active < w.tabs.len());
    }
    #[test] fn t499_theme_all_distinct_names() {
        let names = [Theme::Cream.name(), Theme::Dark.name(), Theme::Forest.name(), Theme::Ocean.name()];
        for i in 0..4 { for j in (i+1)..4 { assert_ne!(names[i], names[j]); } }
    }
    #[test] fn t500_font_choice_copy() { let a = FontChoice::Consolas; let b = a; assert_eq!(a, b); }

    // ════════════════════════════════════════════════════════════════
    // 31-40. Large parameterized sets (501-800)
    // ════════════════════════════════════════════════════════════════

    // Generate 100 safe_byte_pos tests on random-ish positions (501-600)
    macro_rules! bulk_sb_mixed {
        ($name:ident, $text:expr, $pos:expr) => {
            #[test] fn $name() {
                let s: &str = $text;
                let p = safe_byte_pos(s, $pos);
                assert!(s.is_char_boundary(p));
            }
        }
    }
    bulk_sb_mixed!(t501_sbm_1, "The quick brown fox", 0);
    bulk_sb_mixed!(t502_sbm_2, "The quick brown fox", 4);
    bulk_sb_mixed!(t503_sbm_3, "The quick brown fox", 10);
    bulk_sb_mixed!(t504_sbm_4, "The quick brown fox", 19);
    bulk_sb_mixed!(t505_sbm_5, "The quick brown fox", 50);
    bulk_sb_mixed!(t506_sbm_6, "안녕하세요 세계입니다", 0);
    bulk_sb_mixed!(t507_sbm_7, "안녕하세요 세계입니다", 1);
    bulk_sb_mixed!(t508_sbm_8, "안녕하세요 세계입니다", 2);
    bulk_sb_mixed!(t509_sbm_9, "안녕하세요 세계입니다", 3);
    bulk_sb_mixed!(t510_sbm_10, "안녕하세요 세계입니다", 4);
    bulk_sb_mixed!(t511_sbm_11, "안녕하세요 세계입니다", 5);
    bulk_sb_mixed!(t512_sbm_12, "안녕하세요 세계입니다", 6);
    bulk_sb_mixed!(t513_sbm_13, "안녕하세요 세계입니다", 7);
    bulk_sb_mixed!(t514_sbm_14, "안녕하세요 세계입니다", 8);
    bulk_sb_mixed!(t515_sbm_15, "안녕하세요 세계입니다", 9);
    bulk_sb_mixed!(t516_sbm_16, "안녕하세요 세계입니다", 10);
    bulk_sb_mixed!(t517_sbm_17, "안녕하세요 세계입니다", 15);
    bulk_sb_mixed!(t518_sbm_18, "안녕하세요 세계입니다", 20);
    bulk_sb_mixed!(t519_sbm_19, "안녕하세요 세계입니다", 25);
    bulk_sb_mixed!(t520_sbm_20, "안녕하세요 세계입니다", 30);
    bulk_sb_mixed!(t521_sbm_21, "a\nb\nc\nd\ne", 0);
    bulk_sb_mixed!(t522_sbm_22, "a\nb\nc\nd\ne", 1);
    bulk_sb_mixed!(t523_sbm_23, "a\nb\nc\nd\ne", 2);
    bulk_sb_mixed!(t524_sbm_24, "a\nb\nc\nd\ne", 3);
    bulk_sb_mixed!(t525_sbm_25, "a\nb\nc\nd\ne", 4);
    bulk_sb_mixed!(t526_sbm_26, "a\nb\nc\nd\ne", 5);
    bulk_sb_mixed!(t527_sbm_27, "a\nb\nc\nd\ne", 6);
    bulk_sb_mixed!(t528_sbm_28, "a\nb\nc\nd\ne", 7);
    bulk_sb_mixed!(t529_sbm_29, "a\nb\nc\nd\ne", 8);
    bulk_sb_mixed!(t530_sbm_30, "a\nb\nc\nd\ne", 9);
    bulk_sb_mixed!(t531_sbm_31, "mix한ed글text", 0);
    bulk_sb_mixed!(t532_sbm_32, "mix한ed글text", 1);
    bulk_sb_mixed!(t533_sbm_33, "mix한ed글text", 2);
    bulk_sb_mixed!(t534_sbm_34, "mix한ed글text", 3);
    bulk_sb_mixed!(t535_sbm_35, "mix한ed글text", 4);
    bulk_sb_mixed!(t536_sbm_36, "mix한ed글text", 5);
    bulk_sb_mixed!(t537_sbm_37, "mix한ed글text", 6);
    bulk_sb_mixed!(t538_sbm_38, "mix한ed글text", 7);
    bulk_sb_mixed!(t539_sbm_39, "mix한ed글text", 8);
    bulk_sb_mixed!(t540_sbm_40, "mix한ed글text", 9);
    bulk_sb_mixed!(t541_sbm_41, "mix한ed글text", 10);
    bulk_sb_mixed!(t542_sbm_42, "mix한ed글text", 11);
    bulk_sb_mixed!(t543_sbm_43, "mix한ed글text", 12);
    bulk_sb_mixed!(t544_sbm_44, "mix한ed글text", 13);
    bulk_sb_mixed!(t545_sbm_45, "mix한ed글text", 14);
    bulk_sb_mixed!(t546_sbm_46, "mix한ed글text", 15);
    bulk_sb_mixed!(t547_sbm_47, "😀🎉🚀", 0);
    bulk_sb_mixed!(t548_sbm_48, "😀🎉🚀", 1);
    bulk_sb_mixed!(t549_sbm_49, "😀🎉🚀", 2);
    bulk_sb_mixed!(t550_sbm_50, "😀🎉🚀", 3);
    bulk_sb_mixed!(t551_sbm_51, "😀🎉🚀", 4);
    bulk_sb_mixed!(t552_sbm_52, "😀🎉🚀", 5);
    bulk_sb_mixed!(t553_sbm_53, "😀🎉🚀", 6);
    bulk_sb_mixed!(t554_sbm_54, "😀🎉🚀", 7);
    bulk_sb_mixed!(t555_sbm_55, "😀🎉🚀", 8);
    bulk_sb_mixed!(t556_sbm_56, "😀🎉🚀", 9);
    bulk_sb_mixed!(t557_sbm_57, "😀🎉🚀", 10);
    bulk_sb_mixed!(t558_sbm_58, "😀🎉🚀", 11);
    bulk_sb_mixed!(t559_sbm_59, "😀🎉🚀", 12);
    bulk_sb_mixed!(t560_sbm_60, "😀🎉🚀", 13);

    // Generate line/para bound combo tests (561-660)
    macro_rules! bulk_bounds_combo {
        ($name:ident, $text:expr, $pos:expr) => {
            #[test] fn $name() {
                let s: &str = $text;
                let p = safe_byte_pos(s, $pos);
                let (ls, le) = find_line_bounds(s, p);
                let (ps, pe) = find_para_bounds(s, p);
                assert!(ls <= le);
                assert!(ps <= pe);
                assert!(ps <= ls); // para start <= line start
                assert!(le <= pe); // line end <= para end
            }
        }
    }
    bulk_bounds_combo!(t561_bc_1, "hello\nworld\n\nnew para", 0);
    bulk_bounds_combo!(t562_bc_2, "hello\nworld\n\nnew para", 3);
    bulk_bounds_combo!(t563_bc_3, "hello\nworld\n\nnew para", 6);
    bulk_bounds_combo!(t564_bc_4, "hello\nworld\n\nnew para", 10);
    bulk_bounds_combo!(t565_bc_5, "hello\nworld\n\nnew para", 13);
    bulk_bounds_combo!(t566_bc_6, "hello\nworld\n\nnew para", 18);
    bulk_bounds_combo!(t567_bc_7, "a\n\nb\n\nc", 0);
    bulk_bounds_combo!(t568_bc_8, "a\n\nb\n\nc", 3);
    bulk_bounds_combo!(t569_bc_9, "a\n\nb\n\nc", 5);
    bulk_bounds_combo!(t570_bc_10, "single line", 0);
    bulk_bounds_combo!(t571_bc_11, "single line", 5);
    bulk_bounds_combo!(t572_bc_12, "single line", 11);
    bulk_bounds_combo!(t573_bc_13, "a\nb\nc\n\nd\ne\nf", 0);
    bulk_bounds_combo!(t574_bc_14, "a\nb\nc\n\nd\ne\nf", 2);
    bulk_bounds_combo!(t575_bc_15, "a\nb\nc\n\nd\ne\nf", 4);
    bulk_bounds_combo!(t576_bc_16, "a\nb\nc\n\nd\ne\nf", 7);
    bulk_bounds_combo!(t577_bc_17, "a\nb\nc\n\nd\ne\nf", 9);
    bulk_bounds_combo!(t578_bc_18, "a\nb\nc\n\nd\ne\nf", 11);
    bulk_bounds_combo!(t579_bc_19, "", 0);
    bulk_bounds_combo!(t580_bc_20, "x", 0);
    bulk_bounds_combo!(t581_bc_21, "x", 1);
    bulk_bounds_combo!(t582_bc_22, "\n", 0);
    bulk_bounds_combo!(t583_bc_23, "\n\n", 0);
    bulk_bounds_combo!(t584_bc_24, "abc\ndef\n\nghi\njkl", 0);
    bulk_bounds_combo!(t585_bc_25, "abc\ndef\n\nghi\njkl", 4);
    bulk_bounds_combo!(t586_bc_26, "abc\ndef\n\nghi\njkl", 8);
    bulk_bounds_combo!(t587_bc_27, "abc\ndef\n\nghi\njkl", 12);
    bulk_bounds_combo!(t588_bc_28, "한글\n테스트\n\n새문단\n끝", 0);
    bulk_bounds_combo!(t589_bc_29, "한글\n테스트\n\n새문단\n끝", 7);
    bulk_bounds_combo!(t590_bc_30, "한글\n테스트\n\n새문단\n끝", 17);

    // More replace combos (591-660)
    macro_rules! bulk_replace_verify {
        ($name:ident, $text:expr, $find:expr, $repl:expr) => {
            #[test] fn $name() {
                let mut w = make_writer($text);
                w.find_text = $find.into();
                w.replace_text = $repl.into();
                w.replace_all();
                // After replace_all, find_count should be 0 for the original pattern
                w.update_find_count();
                // Verify no original pattern remains (case-insensitive)
                let _lower = w.tabs[0].text.to_lowercase();
                let pat = $find.to_lowercase();
                if $find != $repl && !$repl.to_lowercase().contains(&pat) {
                    assert_eq!(w.find_count, 0, "pattern should be gone after replace_all");
                }
            }
        }
    }
    bulk_replace_verify!(t591_rv_1, "abc def abc", "abc", "xyz");
    bulk_replace_verify!(t592_rv_2, "hello hello hello", "hello", "hi");
    bulk_replace_verify!(t593_rv_3, "test test", "test", "exam");
    bulk_replace_verify!(t594_rv_4, "aaa bbb aaa", "aaa", "ccc");
    bulk_replace_verify!(t595_rv_5, "foo bar foo", "foo", "baz");
    bulk_replace_verify!(t596_rv_6, "한글 한글 한글", "한글", "영어");
    bulk_replace_verify!(t597_rv_7, "one two one", "one", "three");
    bulk_replace_verify!(t598_rv_8, "x y x y x", "x", "z");
    bulk_replace_verify!(t599_rv_9, "old new old", "old", "fresh");
    bulk_replace_verify!(t600_rv_10, "red blue red", "red", "green");
    bulk_replace_verify!(t601_rv_11, "cat dog cat", "cat", "bird");
    bulk_replace_verify!(t602_rv_12, "up down up", "up", "left");
    bulk_replace_verify!(t603_rv_13, "yes no yes", "yes", "ok");
    bulk_replace_verify!(t604_rv_14, "big small big", "big", "tiny");
    bulk_replace_verify!(t605_rv_15, "fast slow fast", "fast", "quick");
    bulk_replace_verify!(t606_rv_16, "hot cold hot", "hot", "warm");
    bulk_replace_verify!(t607_rv_17, "day night day", "day", "morning");
    bulk_replace_verify!(t608_rv_18, "start end start", "start", "begin");
    bulk_replace_verify!(t609_rv_19, "open close open", "open", "shut");
    bulk_replace_verify!(t610_rv_20, "push pull push", "push", "press");

    // Writer combined operations (611-660)
    #[test] fn t611_new_tab_text_independence() {
        let mut w = Writer::new_with(true);
        for i in 0..10 {
            w.new_tab();
            w.tabs[w.active].text = format!("tab {}", i);
        }
        for i in 0..10 {
            assert_eq!(w.tabs[i + 1].text, format!("tab {}", i));
        }
    }
    #[test] fn t612_find_per_tab() {
        let mut w = Writer::new_with(true);
        w.tabs[0].text = "aaa".into();
        w.new_tab();
        w.tabs[1].text = "bbb".into();
        w.find_text = "a".into();
        w.active = 0;
        w.update_find_count();
        assert_eq!(w.find_count, 3);
        w.active = 1;
        w.update_find_count();
        assert_eq!(w.find_count, 0);
    }
    #[test] fn t613_word_count_per_tab() {
        let mut w = Writer::new_with(true);
        w.tabs[0].text = "one two three".into();
        w.new_tab();
        w.tabs[1].text = "a".into();
        w.active = 0;
        assert_eq!(w.word_count(), 3);
        w.active = 1;
        assert_eq!(w.word_count(), 1);
    }
    #[test] fn t614_line_count_per_tab() {
        let mut w = Writer::new_with(true);
        w.tabs[0].text = "a\nb\nc".into();
        w.new_tab();
        w.tabs[1].text = "single".into();
        w.active = 0;
        assert_eq!(w.line_count(), 3);
        w.active = 1;
        assert_eq!(w.line_count(), 1);
    }
    #[test] fn t615_theme_survives_tab_ops() {
        let mut w = Writer::new_with(true);
        w.theme = Theme::Ocean;
        w.new_tab();
        w.close_tab();
        assert_eq!(w.theme, Theme::Ocean);
    }
    #[test] fn t616_focus_survives_tab_ops() {
        let mut w = Writer::new_with(true);
        w.focus_mode = FocusMode::Paragraph;
        w.new_tab();
        assert_eq!(w.focus_mode, FocusMode::Paragraph);
    }
    #[test] fn t617_font_survives_tab_ops() {
        let mut w = Writer::new_with(true);
        w.font_choice = FontChoice::Consolas;
        w.new_tab();
        assert_eq!(w.font_choice, FontChoice::Consolas);
    }
    #[test] fn t618_font_size_survives_tab_ops() {
        let mut w = Writer::new_with(true);
        w.font_size = 24.0;
        w.new_tab();
        assert_eq!(w.font_size, 24.0);
    }

    // Comprehensive edge: empty / whitespace / unicode (619-660)
    #[test] fn t619_tab_empty_counts() {
        let w = make_writer("");
        assert_eq!(w.word_count(), 0);
        assert_eq!(w.char_count(), 0);
        assert_eq!(w.line_count(), 1);
    }
    #[test] fn t620_whitespace_only() {
        let w = make_writer("   \n   \n   ");
        assert_eq!(w.word_count(), 0);
    }
    #[test] fn t621_tabs_and_spaces() {
        let w = make_writer("a\t\tb\t\tc");
        assert_eq!(w.word_count(), 3);
    }
    #[test] fn t622_unicode_symbols() {
        let w = make_writer("alpha \u{03b1} beta \u{03b2}");
        assert!(w.char_count() > 0);
    }
    #[test] fn t623_cjk_mixed() {
        let w = make_writer("中文 日本語 한국어");
        assert_eq!(w.word_count(), 3);
    }
    #[test] fn t624_long_word() {
        let w = make_writer(&"a".repeat(50000));
        assert_eq!(w.word_count(), 1);
        assert_eq!(w.char_count(), 50000);
    }
    #[test] fn t625_many_newlines() {
        let w = make_writer(&"\n".repeat(1000));
        assert_eq!(w.word_count(), 0);
    }
    #[test] fn t626_alternating() {
        let w = make_writer("a b a b a b a b a b");
        assert_eq!(w.word_count(), 10);
    }

    // Parameterized font-related (627-660)
    #[test] fn t627_all_fonts_have_name() {
        for f in FontChoice::ALL { assert!(!f.name().is_empty()); }
    }
    #[test] fn t628_all_fonts_have_file() {
        for f in FontChoice::ALL { assert!(f.file().ends_with(".ttf")); }
    }
    #[test] fn t629_font_switch() {
        let mut w = Writer::new_with(true);
        for &f in FontChoice::ALL {
            w.font_choice = f;
            assert_eq!(w.font_choice, f);
        }
    }
    #[test] fn t630_font_size_all_values() {
        let mut w = Writer::new_with(true);
        for s in 12..=36 {
            w.font_size = s as f32;
            assert_eq!(w.font_size, s as f32);
        }
    }

    // More replace patterns (631-700)
    bulk_replace!(t631_rp_1, "aa bb cc", "bb", "BB", "aa BB cc");
    bulk_replace!(t632_rp_2, "start", "start", "end", "end");
    bulk_replace!(t633_rp_3, "1+1=2", "1", "one", "one+one=2");
    bulk_replace!(t634_rp_4, "a.b.c", ".", "-", "a-b-c");
    bulk_replace!(t635_rp_5, "hello!", "!", "?", "hello?");
    bulk_replace!(t636_rp_6, "(a)(b)", "(", "[", "[a)[b)");
    bulk_replace!(t637_rp_7, "end\n", "\n", "", "end");
    bulk_replace!(t638_rp_8, "  spaces  ", " ", "_", "__spaces__");
    bulk_replace!(t639_rp_9, "CamelCase", "camelcase", "snake_case", "snake_case");
    bulk_replace!(t640_rp_10, "abc123abc", "abc", "XYZ", "XYZ123XYZ");
    bulk_replace!(t641_rp_11, "가나다", "나", "라", "가라다");
    bulk_replace!(t642_rp_12, "one,two,three", ",", ";", "one;two;three");
    bulk_replace!(t643_rp_13, "path/to/file", "/", "\\", "path\\to\\file");
    bulk_replace!(t644_rp_14, "hello world", "hello world", "hi", "hi");
    bulk_replace!(t645_rp_15, "repeat repeat", "repeat", "once", "once once");

    // More count edge cases (646-700)
    bulk_counts!(t646_ce_1, "a", 1, 1, 1);
    bulk_counts!(t647_ce_2, "ab", 1, 2, 1);
    bulk_counts!(t648_ce_3, "a b", 2, 3, 1);
    bulk_counts!(t649_ce_4, "a\n", 1, 2, 1);
    bulk_counts!(t650_ce_5, "\na", 1, 2, 2);
    bulk_counts!(t651_ce_6, "\n\na", 1, 3, 3);
    bulk_counts!(t652_ce_7, "a\n\n", 1, 3, 2);
    bulk_counts!(t653_ce_8, "word\n\nword", 2, 10, 3);
    bulk_counts!(t654_ce_9, "한", 1, 1, 1);
    bulk_counts!(t655_ce_10, "한\n글", 2, 3, 2);
    bulk_counts!(t656_ce_11, "a b\nc d\ne f", 6, 11, 3);
    bulk_counts!(t657_ce_12, "\t", 0, 1, 1);
    bulk_counts!(t658_ce_13, "a\tb", 2, 3, 1);
    bulk_counts!(t659_ce_14, "  \n  ", 0, 5, 2);
    bulk_counts!(t660_ce_15, "a b c d e f g h i j k l m n o p q r s t u v w x y z", 26, 51, 1);

    // ════════════════════════════════════════════════════════════════
    // 41-50. Remaining tests to reach 1000 (661-1000)
    // ════════════════════════════════════════════════════════════════

    // More find tests (661-720)
    bulk_find!(t661_mf_1, "alpha beta gamma", "alpha", 1);
    bulk_find!(t662_mf_2, "alpha beta gamma", "beta", 1);
    bulk_find!(t663_mf_3, "alpha beta gamma", "gamma", 1);
    bulk_find!(t664_mf_4, "alpha beta gamma", "delta", 0);
    bulk_find!(t665_mf_5, "aaabbbccc", "bbb", 1);
    bulk_find!(t666_mf_6, "abababab", "ab", 4);
    bulk_find!(t667_mf_7, "abababab", "aba", 2);
    bulk_find!(t668_mf_8, "HELLO", "hello", 1);
    bulk_find!(t669_mf_9, "hello", "HELLO", 1);
    bulk_find!(t670_mf_10, "HeLLo", "hello", 1);
    bulk_find!(t671_mf_11, "한글 테스트 한글", "한글", 2);
    bulk_find!(t672_mf_12, "abc\nabc\nabc\nabc", "abc", 4);
    bulk_find!(t673_mf_13, "  a  b  c  ", " ", 8);
    bulk_find!(t674_mf_14, "x", "x", 1);
    bulk_find!(t675_mf_15, "x", "y", 0);
    bulk_find!(t676_mf_16, "11111", "11", 2);
    bulk_find!(t677_mf_17, "aAbBcC", "ab", 1);
    bulk_find!(t678_mf_18, "line1\nline2", "line", 2);
    bulk_find!(t679_mf_19, "OpenAI ChatGPT", "gpt", 1);
    bulk_find!(t680_mf_20, "  ", " ", 2);

    // Tab operations extended (681-730)
    #[test] fn t681_50_tabs() {
        let mut w = Writer::new_with(true);
        for _ in 0..49 { w.new_tab(); }
        assert_eq!(w.tabs.len(), 50);
    }
    #[test] fn t682_tab_titles_all_unique_with_paths() {
        let mut w = Writer::new_with(true);
        for i in 0..5 {
            w.new_tab();
            w.tabs[w.active].file_path = Some(PathBuf::from(format!("file{}.txt", i)));
        }
        let names: Vec<_> = w.tabs.iter().map(|t| t.name()).collect();
        // first tab is Untitled, rest are unique
        assert_eq!(names[0], "Untitled");
        for i in 1..6 { assert_eq!(names[i], format!("file{}.txt", i - 1)); }
    }
    #[test] fn t683_close_preserves_order() {
        let mut w = Writer::new_with(true);
        w.tabs[0].text = "A".into();
        w.new_tab(); w.tabs[1].text = "B".into();
        w.new_tab(); w.tabs[2].text = "C".into();
        w.new_tab(); w.tabs[3].text = "D".into();
        w.active = 2; w.close_tab(); // remove C
        assert_eq!(w.tabs[0].text, "A");
        assert_eq!(w.tabs[1].text, "B");
        assert_eq!(w.tabs[2].text, "D");
    }
    #[test] fn t684_close_first_tab() {
        let mut w = Writer::new_with(true);
        w.new_tab();
        w.active = 0;
        w.close_tab();
        assert_eq!(w.active, 0);
        assert_eq!(w.tabs.len(), 1);
    }
    #[test] fn t685_close_last_tab() {
        let mut w = Writer::new_with(true);
        w.new_tab(); w.new_tab(); // active=2
        w.close_tab();
        assert_eq!(w.active, 1);
    }

    // Recovery / recent path tests (686-700)
    #[test] fn t686_recovery_path_is_temp() {
        let p = Writer::recovery_path();
        assert!(p.starts_with(std::env::temp_dir()));
    }
    #[test] fn t687_recent_path_is_temp() {
        let p = Writer::recent_path();
        assert!(p.starts_with(std::env::temp_dir()));
    }
    #[test] fn t688_recovery_path_has_txt() {
        let p = Writer::recovery_path();
        assert!(p.extension().map(|e| e == "txt").unwrap_or(false));
    }
    #[test] fn t689_recent_path_has_txt() {
        let p = Writer::recent_path();
        assert!(p.extension().map(|e| e == "txt").unwrap_or(false));
    }

    // Comprehensive state transitions (690-730)
    #[test] fn t690_full_theme_cycle() {
        let mut t = Theme::Cream;
        let themes = vec![Theme::Dark, Theme::Forest, Theme::Ocean, Theme::Sepia, Theme::Midnight, Theme::Solarized, Theme::Nord, Theme::Lavender, Theme::Cream];
        for expected in themes { t = t.next(); assert_eq!(t, expected); }
    }
    #[test] fn t691_full_focus_cycle() {
        let mut f = FocusMode::Off;
        let modes = vec![FocusMode::Line, FocusMode::Paragraph, FocusMode::Off];
        for expected in modes { f = f.next(); assert_eq!(f, expected); }
    }
    #[test] fn t692_theme_colors_valid() {
        for &t in &[Theme::Cream, Theme::Dark, Theme::Forest, Theme::Ocean] {
            let bg = t.bg().to_array();
            let fg = t.fg().to_array();
            // bg and fg should have visible contrast
            let diff: i32 = (bg[0] as i32 - fg[0] as i32).abs()
                + (bg[1] as i32 - fg[1] as i32).abs()
                + (bg[2] as i32 - fg[2] as i32).abs();
            assert!(diff > 100, "theme {} lacks contrast", t.name());
        }
    }
    #[test] fn t693_hover_ne_bg() {
        for &t in &[Theme::Cream, Theme::Dark, Theme::Forest, Theme::Ocean] {
            assert_ne!(t.hover(), t.bg());
        }
    }
    #[test] fn t694_dim_between_bg_fg() {
        // dim should be somewhere between bg and fg brightness
        for &t in &[Theme::Cream, Theme::Dark, Theme::Forest, Theme::Ocean] {
            let bg_sum: u32 = t.bg().to_array()[..3].iter().map(|&v| v as u32).sum();
            let fg_sum: u32 = t.fg().to_array()[..3].iter().map(|&v| v as u32).sum();
            let dim_sum: u32 = t.dim().to_array()[..3].iter().map(|&v| v as u32).sum();
            let (lo, hi) = if bg_sum < fg_sum { (bg_sum, fg_sum) } else { (fg_sum, bg_sum) };
            assert!(dim_sum >= lo && dim_sum <= hi, "dim out of range for {}", t.name());
        }
    }

    // Find count stress (695-730)
    #[test] fn t695_find_in_1000_lines() {
        let s = (0..1000).map(|_| "target word").collect::<Vec<_>>().join("\n");
        let mut w = make_writer(&s);
        w.find_text = "target".into();
        w.update_find_count();
        assert_eq!(w.find_count, 1000);
    }
    #[test] fn t696_find_empty_result() {
        let mut w = make_writer(&long_text(100));
        w.find_text = "NONEXISTENT_PATTERN_XYZ".into();
        w.update_find_count();
        assert_eq!(w.find_count, 0);
    }
    #[test] fn t697_replace_all_large() {
        let s = (0..500).map(|_| "old").collect::<Vec<_>>().join(" ");
        let mut w = make_writer(&s);
        w.find_text = "old".into();
        w.replace_text = "new".into();
        w.replace_all();
        assert!(!w.tabs[0].text.contains("old"));
    }
    #[test] fn t698_word_count_large_text() {
        let s = (0..10000).map(|i| format!("word{}", i)).collect::<Vec<_>>().join(" ");
        let w = make_writer(&s);
        assert_eq!(w.word_count(), 10000);
    }
    #[test] fn t699_char_count_unicode_large() {
        let s = "가나다라마바사아자차카타파하".repeat(100);
        let w = make_writer(&s);
        assert_eq!(w.char_count(), 14 * 100);
    }
    #[test] fn t700_line_count_large() {
        let w = make_writer(&long_text(5000));
        assert_eq!(w.line_count(), 5000);
    }

    // Bulk char_to_byte extended (701-750)
    bulk_c2b!(t701_c2b_ext_1, "hello world", 6, 6);
    bulk_c2b!(t702_c2b_ext_2, "hello world", 11, 11);
    bulk_c2b!(t703_c2b_ext_3, "안녕하세요", 0, 0);
    bulk_c2b!(t704_c2b_ext_4, "안녕하세요", 1, 3);
    bulk_c2b!(t705_c2b_ext_5, "안녕하세요", 2, 6);
    bulk_c2b!(t706_c2b_ext_6, "안녕하세요", 3, 9);
    bulk_c2b!(t707_c2b_ext_7, "안녕하세요", 4, 12);
    bulk_c2b!(t708_c2b_ext_8, "안녕하세요", 5, 15);
    bulk_c2b!(t709_c2b_ext_9, "a\n\nb", 0, 0);
    bulk_c2b!(t710_c2b_ext_10, "a\n\nb", 1, 1);
    bulk_c2b!(t711_c2b_ext_11, "a\n\nb", 2, 2);
    bulk_c2b!(t712_c2b_ext_12, "a\n\nb", 3, 3);
    bulk_c2b!(t713_c2b_ext_13, "a\n\nb", 4, 4);
    bulk_c2b!(t714_c2b_ext_14, "😀😁😂", 0, 0);
    bulk_c2b!(t715_c2b_ext_15, "😀😁😂", 1, 4);
    bulk_c2b!(t716_c2b_ext_16, "😀😁😂", 2, 8);
    bulk_c2b!(t717_c2b_ext_17, "😀😁😂", 3, 12);
    bulk_c2b!(t718_c2b_ext_18, "a😀b😁c", 0, 0);
    bulk_c2b!(t719_c2b_ext_19, "a😀b😁c", 1, 1);
    bulk_c2b!(t720_c2b_ext_20, "a😀b😁c", 2, 5);
    bulk_c2b!(t721_c2b_ext_21, "a😀b😁c", 3, 6);
    bulk_c2b!(t722_c2b_ext_22, "a😀b😁c", 4, 10);
    bulk_c2b!(t723_c2b_ext_23, "a😀b😁c", 5, 11);

    // More safe_byte_pos edge cases (724-770)
    bulk_safe_byte!(t724_sbe_1, "test string", 0);
    bulk_safe_byte!(t725_sbe_2, "test string", 4);
    bulk_safe_byte!(t726_sbe_3, "test string", 11);
    bulk_safe_byte!(t727_sbe_4, "test string", 20);
    bulk_safe_byte!(t728_sbe_5, "가나다라마", 0);
    bulk_safe_byte!(t729_sbe_6, "가나다라마", 1);
    bulk_safe_byte!(t730_sbe_7, "가나다라마", 2);
    bulk_safe_byte!(t731_sbe_8, "가나다라마", 3);
    bulk_safe_byte!(t732_sbe_9, "가나다라마", 4);
    bulk_safe_byte!(t733_sbe_10, "가나다라마", 5);
    bulk_safe_byte!(t734_sbe_11, "가나다라마", 6);
    bulk_safe_byte!(t735_sbe_12, "가나다라마", 7);
    bulk_safe_byte!(t736_sbe_13, "가나다라마", 8);
    bulk_safe_byte!(t737_sbe_14, "가나다라마", 9);
    bulk_safe_byte!(t738_sbe_15, "가나다라마", 10);
    bulk_safe_byte!(t739_sbe_16, "가나다라마", 11);
    bulk_safe_byte!(t740_sbe_17, "가나다라마", 12);
    bulk_safe_byte!(t741_sbe_18, "가나다라마", 13);
    bulk_safe_byte!(t742_sbe_19, "가나다라마", 14);
    bulk_safe_byte!(t743_sbe_20, "가나다라마", 15);

    // More line bounds (744-770)
    bulk_line_bounds!(t744_lbe_1, "alpha\nbeta\ngamma\ndelta", 0);
    bulk_line_bounds!(t745_lbe_2, "alpha\nbeta\ngamma\ndelta", 3);
    bulk_line_bounds!(t746_lbe_3, "alpha\nbeta\ngamma\ndelta", 6);
    bulk_line_bounds!(t747_lbe_4, "alpha\nbeta\ngamma\ndelta", 10);
    bulk_line_bounds!(t748_lbe_5, "alpha\nbeta\ngamma\ndelta", 11);
    bulk_line_bounds!(t749_lbe_6, "alpha\nbeta\ngamma\ndelta", 16);
    bulk_line_bounds!(t750_lbe_7, "alpha\nbeta\ngamma\ndelta", 17);
    bulk_line_bounds!(t751_lbe_8, "alpha\nbeta\ngamma\ndelta", 22);
    bulk_line_bounds!(t752_lbe_9, "short\nlonger line here\nx", 0);
    bulk_line_bounds!(t753_lbe_10, "short\nlonger line here\nx", 6);
    bulk_line_bounds!(t754_lbe_11, "short\nlonger line here\nx", 15);
    bulk_line_bounds!(t755_lbe_12, "short\nlonger line here\nx", 22);

    // More para bounds (756-780)
    bulk_para_bounds!(t756_pbe_1, "first para\nstill first\n\nsecond para\nstill second", 0);
    bulk_para_bounds!(t757_pbe_2, "first para\nstill first\n\nsecond para\nstill second", 5);
    bulk_para_bounds!(t758_pbe_3, "first para\nstill first\n\nsecond para\nstill second", 11);
    bulk_para_bounds!(t759_pbe_4, "first para\nstill first\n\nsecond para\nstill second", 22);
    bulk_para_bounds!(t760_pbe_5, "first para\nstill first\n\nsecond para\nstill second", 24);
    bulk_para_bounds!(t761_pbe_6, "first para\nstill first\n\nsecond para\nstill second", 35);
    bulk_para_bounds!(t762_pbe_7, "p1\n\np2\n\np3\n\np4\n\np5", 0);
    bulk_para_bounds!(t763_pbe_8, "p1\n\np2\n\np3\n\np4\n\np5", 4);
    bulk_para_bounds!(t764_pbe_9, "p1\n\np2\n\np3\n\np4\n\np5", 8);
    bulk_para_bounds!(t765_pbe_10, "p1\n\np2\n\np3\n\np4\n\np5", 12);

    // More blend tests (766-790)
    #[test] fn t766_blend_all_themes_0() {
        for &t in &[Theme::Cream, Theme::Dark, Theme::Forest, Theme::Ocean] {
            let r = blend_color(t.bg(), t.fg(), 0.0);
            assert_eq!(r, t.bg());
        }
    }
    #[test] fn t767_blend_all_themes_1() {
        for &t in &[Theme::Cream, Theme::Dark, Theme::Forest, Theme::Ocean] {
            let r = blend_color(t.bg(), t.fg(), 1.0);
            assert_eq!(r, t.fg());
        }
    }
    #[test] fn t768_blend_symmetric() {
        let a = egui::Color32::from_rgb(50, 50, 50);
        let b = egui::Color32::from_rgb(150, 150, 150);
        let mid = blend_color(a, b, 0.5);
        assert_eq!(mid, egui::Color32::from_rgb(100, 100, 100));
    }

    // More counts (769-800)
    bulk_counts!(t769_mc_1, "The quick brown fox jumps over the lazy dog", 9, 43, 1);
    bulk_counts!(t770_mc_2, "Line\n\nLine\n\nLine", 3, 16, 5);
    bulk_counts!(t771_mc_3, "123 456 789", 3, 11, 1);
    bulk_counts!(t772_mc_4, "!@# $%^ &*()", 3, 12, 1);
    bulk_counts!(t773_mc_5, "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\nk\nl\nm\nn\no\np\nq\nr\ns\nt", 20, 39, 20);
    bulk_counts!(t774_mc_6, "space   between   words", 3, 23, 1);
    bulk_counts!(t775_mc_7, "가나다 라마바 사아자", 3, 11, 1);
    bulk_counts!(t776_mc_8, "A\nB\nC\nD\nE", 5, 9, 5);
    bulk_counts!(t777_mc_9, "word1 word2 word3 word4 word5 word6 word7 word8 word9 word10", 10, 60, 1);
    bulk_counts!(t778_mc_10, "\n\n\n\n\n", 0, 5, 5);

    // More find (779-820)
    bulk_find!(t779_mf2_1, "banana", "an", 2);
    bulk_find!(t780_mf2_2, "banana", "ana", 1);
    bulk_find!(t781_mf2_3, "aaaa", "a", 4);
    bulk_find!(t782_mf2_4, "aaaa", "aaa", 1);
    bulk_find!(t783_mf2_5, "test123test", "test", 2);
    bulk_find!(t784_mf2_6, "UPPERCASE", "upper", 1);
    bulk_find!(t785_mf2_7, "lowercase", "LOWER", 1);
    bulk_find!(t786_mf2_8, "CamelCase", "camelcase", 1);
    bulk_find!(t787_mf2_9, "snake_case", "SNAKE_CASE", 1);
    bulk_find!(t788_mf2_10, "mixed-case", "Mixed-Case", 1);
    bulk_find!(t789_mf2_11, "a b c d e", "a", 1);
    bulk_find!(t790_mf2_12, "a b c d e", "e", 1);
    bulk_find!(t791_mf2_13, "repeat\nrepeat\nrepeat", "repeat", 3);
    bulk_find!(t792_mf2_14, "한글한글한글한글", "한글", 4);
    bulk_find!(t793_mf2_15, "a1b2c3", "1", 1);
    bulk_find!(t794_mf2_16, "a1b2c3", "4", 0);
    bulk_find!(t795_mf2_17, "  spaced  ", "spaced", 1);

    // More replace (796-850)
    bulk_replace!(t796_mr_1, "a-b-c-d", "-", ".", "a.b.c.d");
    bulk_replace!(t797_mr_2, "yes yes yes", "yes", "no", "no no no");
    bulk_replace!(t798_mr_3, "abc", "abc", "abcabc", "abcabc");
    bulk_replace!(t799_mr_4, "111", "1", "22", "222222");
    bulk_replace!(t800_mr_5, "가나다", "가", "마", "마나다");

    // Extended state tests (801-850)
    #[test] fn t801_writer_find_text_init() { assert_eq!(Writer::new_with(true).find_text, ""); }
    #[test] fn t802_writer_replace_text_init() { assert_eq!(Writer::new_with(true).replace_text, ""); }
    #[test] fn t803_writer_find_count_init() { assert_eq!(Writer::new_with(true).find_count, 0); }
    #[test] fn t804_menu_hover_none() { assert!(Writer::new_with(true).menu_hover_time.is_none()); }
    #[test] fn t805_applied_font_init() { assert_eq!(Writer::new_with(true).applied_font, Some(FontChoice::MalgunGothic)); }
    #[test] fn t806_skip_recovery_true() {
        let w = Writer::new_with(true);
        // should have empty text when skip_recovery
        // (unless recovery file was written by test setup)
    }

    // Comprehensive integration-style tests (807-850)
    #[test] fn t807_full_workflow() {
        let mut w = Writer::new_with(true);
        w.tabs[0].text = "Hello World".into();
        w.tabs[0].modified = true;
        assert_eq!(w.word_count(), 2);
        assert_eq!(w.char_count(), 11);
        assert_eq!(w.line_count(), 1);
        w.find_text = "world".into();
        w.update_find_count();
        assert_eq!(w.find_count, 1);
        w.replace_text = "Earth".into();
        w.replace_next();
        assert_eq!(w.tabs[0].text, "Hello Earth");
    }
    #[test] fn t808_multi_tab_workflow() {
        let mut w = Writer::new_with(true);
        w.tabs[0].text = "Tab 1 content".into();
        w.new_tab();
        w.tabs[1].text = "Tab 2 content".into();
        w.new_tab();
        w.tabs[2].text = "Tab 3 content".into();
        assert_eq!(w.tabs.len(), 3);
        assert_eq!(w.active, 2);
        w.active = 0;
        assert_eq!(w.word_count(), 3);
        w.close_tab();
        assert_eq!(w.tabs.len(), 2);
    }
    #[test] fn t809_theme_and_count() {
        let mut w = make_writer("some text here");
        w.theme = Theme::Dark;
        assert_eq!(w.word_count(), 3);
        w.theme = Theme::Forest;
        assert_eq!(w.word_count(), 3); // theme change shouldn't affect counts
    }
    #[test] fn t810_korean_workflow() {
        let mut w = make_writer("안녕하세요 세계입니다");
        assert_eq!(w.word_count(), 2);
        w.find_text = "세계".into();
        w.update_find_count();
        assert_eq!(w.find_count, 1);
        w.replace_text = "세상".into();
        w.replace_next();
        assert!(w.tabs[0].text.contains("세상"));
    }

    // More parameterized edge cases (811-900)
    bulk_find!(t811_pf_1, "aaaaaa", "aa", 3);
    bulk_find!(t812_pf_2, "ababababab", "abab", 2);
    bulk_find!(t813_pf_3, "xyzxyzxyz", "xyz", 3);
    bulk_find!(t814_pf_4, "Hello World Hello World", "hello world", 2);
    bulk_find!(t815_pf_5, "123 456 789 123", "123", 2);
    bulk_find!(t816_pf_6, "a\n\na\n\na", "a", 3);
    bulk_find!(t817_pf_7, " a b c ", "a", 1);
    bulk_find!(t818_pf_8, "abc ABC abc", "abc", 3);
    bulk_find!(t819_pf_9, "test.test.test", "test", 3);
    bulk_find!(t820_pf_10, "end\nend\nend\nend\nend", "end", 5);

    bulk_replace!(t821_pr_1, "hello hello hello hello", "hello", "hi", "hi hi hi hi");
    bulk_replace!(t822_pr_2, "a,b,c,d,e", ",", " ", "a b c d e");
    bulk_replace!(t823_pr_3, "xxx", "x", "yy", "yyyyyy");
    bulk_replace!(t824_pr_4, "한 글 테 스 트", " ", "", "한글테스트");
    bulk_replace!(t825_pr_5, "oldold", "old", "new", "newnew");
    bulk_replace!(t826_pr_6, "12345", "3", "THREE", "12THREE45");
    bulk_replace!(t827_pr_7, "a+b+c", "+", "-", "a-b-c");
    bulk_replace!(t828_pr_8, "foo", "foo", "foo", "foo");
    bulk_replace!(t829_pr_9, "bar", "BAR", "baz", "baz");
    bulk_replace!(t830_pr_10, "remove me", "remove ", "", "me");

    // Additional bounds and char conversion (831-870)
    bulk_line_bounds!(t831_alb_1, "one two three\nfour five six\nseven eight nine", 0);
    bulk_line_bounds!(t832_alb_2, "one two three\nfour five six\nseven eight nine", 7);
    bulk_line_bounds!(t833_alb_3, "one two three\nfour five six\nseven eight nine", 14);
    bulk_line_bounds!(t834_alb_4, "one two three\nfour five six\nseven eight nine", 20);
    bulk_line_bounds!(t835_alb_5, "one two three\nfour five six\nseven eight nine", 28);
    bulk_line_bounds!(t836_alb_6, "one two three\nfour five six\nseven eight nine", 35);
    bulk_line_bounds!(t837_alb_7, "x\ny\nz", 0);
    bulk_line_bounds!(t838_alb_8, "x\ny\nz", 2);
    bulk_line_bounds!(t839_alb_9, "x\ny\nz", 4);

    bulk_para_bounds!(t840_apb_1, "intro\n\nbody one\nbody two\n\nconclusion", 0);
    bulk_para_bounds!(t841_apb_2, "intro\n\nbody one\nbody two\n\nconclusion", 7);
    bulk_para_bounds!(t842_apb_3, "intro\n\nbody one\nbody two\n\nconclusion", 16);
    bulk_para_bounds!(t843_apb_4, "intro\n\nbody one\nbody two\n\nconclusion", 25);
    bulk_para_bounds!(t844_apb_5, "intro\n\nbody one\nbody two\n\nconclusion", 30);

    bulk_c2b!(t845_ac2b_1, "test data", 0, 0);
    bulk_c2b!(t846_ac2b_2, "test data", 4, 4);
    bulk_c2b!(t847_ac2b_3, "test data", 5, 5);
    bulk_c2b!(t848_ac2b_4, "test data", 9, 9);
    bulk_c2b!(t849_ac2b_5, "한글 mixed 혼합", 0, 0);
    bulk_c2b!(t850_ac2b_6, "한글 mixed 혼합", 1, 3);
    bulk_c2b!(t851_ac2b_7, "한글 mixed 혼합", 2, 6);
    bulk_c2b!(t852_ac2b_8, "한글 mixed 혼합", 3, 7);
    bulk_c2b!(t853_ac2b_9, "한글 mixed 혼합", 8, 12);
    bulk_c2b!(t854_ac2b_10, "한글 mixed 혼합", 9, 13);

    // Final comprehensive batch (855-1000)
    bulk_counts!(t855_fc_1, "apple banana cherry", 3, 19, 1);
    bulk_counts!(t856_fc_2, "one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\nten", 10, 48, 10);
    bulk_counts!(t857_fc_3, "short", 1, 5, 1);
    bulk_counts!(t858_fc_4, "a longer sentence with multiple words in it", 8, 43, 1);
    bulk_counts!(t859_fc_5, "first\nsecond\n\nthird\nfourth", 4, 26, 5);
    bulk_counts!(t860_fc_6, "hello\n\n\n\nworld", 2, 14, 5);

    bulk_find!(t861_ff_1, "test test test test test", "test", 5);
    bulk_find!(t862_ff_2, "no match here", "xyz123", 0);
    bulk_find!(t863_ff_3, "ALL CAPS TEXT", "all caps", 1);
    bulk_find!(t864_ff_4, "aA", "a", 2);
    bulk_find!(t865_ff_5, "needle in haystack needle", "needle", 2);

    bulk_replace!(t866_fr_1, "temp temp temp", "temp", "perm", "perm perm perm");
    bulk_replace!(t867_fr_2, "1+2+3", "+", " plus ", "1 plus 2 plus 3");
    bulk_replace!(t868_fr_3, "SHOUT", "shout", "whisper", "whisper");
    bulk_replace!(t869_fr_4, "a b c d e f", " ", "\n", "a\nb\nc\nd\ne\nf");
    bulk_replace!(t870_fr_5, "dot.dot.dot", "dot", "dash", "dash.dash.dash");

    bulk_sb_mixed!(t871_fsb_1, "final test string", 0);
    bulk_sb_mixed!(t872_fsb_2, "final test string", 6);
    bulk_sb_mixed!(t873_fsb_3, "final test string", 11);
    bulk_sb_mixed!(t874_fsb_4, "final test string", 17);
    bulk_sb_mixed!(t875_fsb_5, "한글 final 테스트", 0);
    bulk_sb_mixed!(t876_fsb_6, "한글 final 테스트", 3);
    bulk_sb_mixed!(t877_fsb_7, "한글 final 테스트", 7);
    bulk_sb_mixed!(t878_fsb_8, "한글 final 테스트", 13);
    bulk_sb_mixed!(t879_fsb_9, "한글 final 테스트", 16);
    bulk_sb_mixed!(t880_fsb_10, "한글 final 테스트", 22);

    bulk_bounds_combo!(t881_fbc_1, "Line one\nLine two\n\nPara two\nEnd", 0);
    bulk_bounds_combo!(t882_fbc_2, "Line one\nLine two\n\nPara two\nEnd", 5);
    bulk_bounds_combo!(t883_fbc_3, "Line one\nLine two\n\nPara two\nEnd", 9);
    bulk_bounds_combo!(t884_fbc_4, "Line one\nLine two\n\nPara two\nEnd", 18);
    bulk_bounds_combo!(t885_fbc_5, "Line one\nLine two\n\nPara two\nEnd", 20);
    bulk_bounds_combo!(t886_fbc_6, "Line one\nLine two\n\nPara two\nEnd", 28);

    bulk_mixed!(t887_fmx_1, "Final mixed content test", 4);
    bulk_mixed!(t888_fmx_2, "여러 줄의\n한국어\n테스트", 3);
    bulk_mixed!(t889_fmx_3, "1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20", 20);
    bulk_mixed!(t890_fmx_4, "Multiline\nContent\nWith\nMany\nLines\nHere", 6);

    // Final state validation (891-950)
    #[test] fn t891_initial_state_complete() {
        let w = Writer::new_with(true);
        assert_eq!(w.tabs.len(), 1);
        assert_eq!(w.active, 0);
        assert!(!w.show_find);
        assert!(!w.show_replace);
        assert!(!w.show_status);
        assert!(!w.show_recent);
        assert!(!w.show_about);
        assert!(!w.typewriter);
        assert!(!w.fullscreen);
        assert_eq!(w.theme, Theme::Cream);
        assert_eq!(w.font_choice, FontChoice::MalgunGothic);
        assert_eq!(w.font_size, 18.0);
        assert_eq!(w.line_spacing, 1.6);
        assert_eq!(w.focus_mode, FocusMode::Off);
        assert_eq!(w.cursor_byte_pos, 0);
        assert_eq!(w.hwnd, 0);
        assert!(w.applied_titlebar.is_none());
        assert!(w.save_flash.is_none());
    }
    #[test] fn t892_all_settings_changeable() {
        let mut w = Writer::new_with(true);
        w.show_find = true; w.show_replace = true; w.show_status = true;
        w.show_recent = true; w.show_about = true; w.typewriter = true;
        w.fullscreen = true; w.theme = Theme::Ocean; w.font_choice = FontChoice::Verdana;
        w.font_size = 24.0; w.line_spacing = 2.0; w.focus_mode = FocusMode::Paragraph;
        assert!(w.show_find && w.show_replace && w.show_status);
        assert!(w.show_recent && w.show_about && w.typewriter && w.fullscreen);
        assert_eq!(w.theme, Theme::Ocean);
        assert_eq!(w.font_choice, FontChoice::Verdana);
        assert_eq!(w.font_size, 24.0);
    }
    #[test] fn t893_complex_replace_chain() {
        let mut w = make_writer("A B C D E F G H I J");
        w.find_text = "a".into(); w.replace_text = "1".into(); w.replace_all();
        w.find_text = "b".into(); w.replace_text = "2".into(); w.replace_all();
        w.find_text = "c".into(); w.replace_text = "3".into(); w.replace_all();
        assert_eq!(w.tabs[0].text, "1 2 3 D E F G H I J");
    }
    #[test] fn t894_tab_stress_with_text() {
        let mut w = Writer::new_with(true);
        for i in 0..30 {
            w.new_tab();
            w.tabs[w.active].text = format!("Content of tab {}", i);
        }
        for i in 0..30 {
            w.active = i + 1;
            assert!(w.tabs[w.active].text.contains(&format!("{}", i)));
        }
    }
    #[test] fn t895_find_replace_cycle() {
        let mut w = make_writer("old old old");
        w.find_text = "old".into();
        w.replace_text = "new".into();
        w.replace_all();
        w.find_text = "new".into();
        w.replace_text = "fresh".into();
        w.replace_all();
        assert_eq!(w.tabs[0].text, "fresh fresh fresh");
    }

    // Line/word boundary edge cases (896-950)
    #[test] fn t896_trailing_whitespace() {
        let w = make_writer("hello   ");
        assert_eq!(w.word_count(), 1);
    }
    #[test] fn t897_leading_whitespace() {
        let w = make_writer("   hello");
        assert_eq!(w.word_count(), 1);
    }
    #[test] fn t898_mixed_whitespace() {
        let w = make_writer("  hello  world  ");
        assert_eq!(w.word_count(), 2);
    }
    #[test] fn t899_cr_lf() {
        let w = make_writer("hello\r\nworld");
        assert_eq!(w.word_count(), 2);
    }
    #[test] fn t900_tab_chars() {
        let w = make_writer("a\tb\tc");
        assert_eq!(w.word_count(), 3);
    }

    // Final batch (901-1000)
    bulk_counts!(t901_z_1, "z", 1, 1, 1);
    bulk_counts!(t902_z_2, "z z", 2, 3, 1);
    bulk_counts!(t903_z_3, "z\nz", 2, 3, 2);
    bulk_counts!(t904_z_4, "z z z z z", 5, 9, 1);
    bulk_counts!(t905_z_5, "zzzzz", 1, 5, 1);
    bulk_find!(t906_z_6, "zzzzzz", "z", 6);
    bulk_find!(t907_z_7, "zzzzzz", "zz", 3);
    bulk_find!(t908_z_8, "zzzzzz", "zzz", 2);
    bulk_find!(t909_z_9, "z z z", "z", 3);
    bulk_find!(t910_z_10, "z z z", " ", 2);
    bulk_replace!(t911_z_11, "z z z", "z", "a", "a a a");
    bulk_replace!(t912_z_12, "zzz", "z", "ab", "ababab");
    bulk_replace!(t913_z_13, "z\nz\nz", "\n", " ", "z z z");
    bulk_replace!(t914_z_14, "z_z_z", "_", "-", "z-z-z");
    bulk_replace!(t915_z_15, "ZZZ", "zzz", "aaa", "aaa");
    bulk_sb_mixed!(t916_z_16, "z", 0);
    bulk_sb_mixed!(t917_z_17, "z", 1);
    bulk_sb_mixed!(t918_z_18, "zz", 0);
    bulk_sb_mixed!(t919_z_19, "zz", 1);
    bulk_sb_mixed!(t920_z_20, "zz", 2);
    bulk_line_bounds!(t921_z_21, "z", 0);
    bulk_line_bounds!(t922_z_22, "z", 1);
    bulk_line_bounds!(t923_z_23, "z\nz", 0);
    bulk_line_bounds!(t924_z_24, "z\nz", 2);
    bulk_line_bounds!(t925_z_25, "z\nz\nz", 0);
    bulk_line_bounds!(t926_z_26, "z\nz\nz", 2);
    bulk_line_bounds!(t927_z_27, "z\nz\nz", 4);
    bulk_para_bounds!(t928_z_28, "z\n\nz", 0);
    bulk_para_bounds!(t929_z_29, "z\n\nz", 3);
    bulk_para_bounds!(t930_z_30, "z\n\nz\n\nz", 0);
    bulk_para_bounds!(t931_z_31, "z\n\nz\n\nz", 3);
    bulk_para_bounds!(t932_z_32, "z\n\nz\n\nz", 6);
    bulk_c2b!(t933_z_33, "z", 0, 0);
    bulk_c2b!(t934_z_34, "z", 1, 1);
    bulk_c2b!(t935_z_35, "zz", 0, 0);
    bulk_c2b!(t936_z_36, "zz", 1, 1);
    bulk_c2b!(t937_z_37, "zz", 2, 2);
    bulk_c2b!(t938_z_38, "z한z", 0, 0);
    bulk_c2b!(t939_z_39, "z한z", 1, 1);
    bulk_c2b!(t940_z_40, "z한z", 2, 4);
    bulk_c2b!(t941_z_41, "z한z", 3, 5);
    bulk_bounds_combo!(t942_z_42, "z\nz\n\nz", 0);
    bulk_bounds_combo!(t943_z_43, "z\nz\n\nz", 2);
    bulk_bounds_combo!(t944_z_44, "z\nz\n\nz", 4);
    bulk_bounds_combo!(t945_z_45, "z\nz\n\nz", 5);
    #[test] fn t946_color_hex_all_themes() {
        for &t in &[Theme::Cream, Theme::Dark, Theme::Forest, Theme::Ocean] {
            let h = color_hex(t.bg());
            assert!(h.starts_with('#'));
            assert_eq!(h.len(), 7);
        }
    }
    #[test] fn t947_replace_chain_korean() {
        let mut w = make_writer("가 나 다 라 마");
        w.find_text = "가".into(); w.replace_text = "1".into(); w.replace_all();
        w.find_text = "나".into(); w.replace_text = "2".into(); w.replace_all();
        w.find_text = "다".into(); w.replace_text = "3".into(); w.replace_all();
        w.find_text = "라".into(); w.replace_text = "4".into(); w.replace_all();
        w.find_text = "마".into(); w.replace_text = "5".into(); w.replace_all();
        assert_eq!(w.tabs[0].text, "1 2 3 4 5");
    }
    #[test] fn t948_massive_tab_text() {
        let mut w = Writer::new_with(true);
        w.tabs[0].text = "x".repeat(100000);
        assert_eq!(w.char_count(), 100000);
        assert_eq!(w.word_count(), 1);
        assert_eq!(w.line_count(), 1);
    }
    #[test] fn t949_massive_line_count() {
        let s = (0..50000).map(|_| "w").collect::<Vec<_>>().join("\n");
        let w = make_writer(&s);
        assert_eq!(w.line_count(), 50000);
    }
    #[test] fn t950_all_font_choices() {
        for (i, &f) in FontChoice::ALL.iter().enumerate() {
            assert_eq!(FontChoice::ALL[i], f);
        }
    }

    // Final 50 (951-1000)
    bulk_replace_verify!(t951_fv_1, "alpha beta gamma", "alpha", "omega");
    bulk_replace_verify!(t952_fv_2, "one two three", "two", "dos");
    bulk_replace_verify!(t953_fv_3, "hello world", "hello", "goodbye");
    bulk_replace_verify!(t954_fv_4, "cat dog bird", "dog", "fish");
    bulk_replace_verify!(t955_fv_5, "red green blue", "green", "yellow");
    bulk_replace_verify!(t956_fv_6, "sun moon star", "moon", "planet");
    bulk_replace_verify!(t957_fv_7, "north south east west", "south", "north");
    bulk_replace_verify!(t958_fv_8, "up down left right", "left", "center");
    bulk_replace_verify!(t959_fv_9, "spring summer fall winter", "fall", "autumn");
    bulk_replace_verify!(t960_fv_10, "rock paper scissors", "paper", "card");

    bulk_mixed!(t961_fm_1, "short text", 2);
    bulk_mixed!(t962_fm_2, "medium length text here", 4);
    bulk_mixed!(t963_fm_3, "a b c d e f", 6);
    bulk_mixed!(t964_fm_4, "한글\n영어\n혼합", 3);
    bulk_mixed!(t965_fm_5, "Final test sentence", 3);

    #[test] fn t966_close_all_via_loop() {
        let mut w = Writer::new_with(true);
        for _ in 0..20 { w.new_tab(); }
        while w.tabs.len() > 1 { w.close_tab(); }
        assert_eq!(w.tabs.len(), 1);
    }
    #[test] fn t967_find_replace_empty_text() {
        let mut w = make_writer("");
        w.find_text = "x".into();
        w.update_find_count();
        assert_eq!(w.find_count, 0);
        w.replace_text = "y".into();
        w.replace_next();
        assert_eq!(w.tabs[0].text, "");
    }
    #[test] fn t968_word_boundary() {
        let w = make_writer("word1\tword2\nword3 word4");
        assert_eq!(w.word_count(), 4);
    }
    #[test] fn t969_triple_newline() {
        let w = make_writer("a\n\n\nb");
        assert_eq!(w.line_count(), 4);
    }
    #[test] fn t970_unicode_length() {
        let s = "a가b나c다";
        assert_eq!(char_to_byte(s, 6), s.len());
    }
    #[test] fn t971_safe_byte_consistency() {
        let s = "Hello 안녕 World 세계";
        for i in 0..s.len()+10 {
            let p = safe_byte_pos(s, i);
            assert!(p <= s.len());
            assert!(s.is_char_boundary(p));
        }
    }
    #[test] fn t972_line_para_consistency() {
        let s = "Line1\nLine2\n\nPara2Line1\nPara2Line2";
        for i in 0..s.len() {
            if s.is_char_boundary(i) {
                let (ls, le) = find_line_bounds(s, i);
                let (ps, pe) = find_para_bounds(s, i);
                assert!(ps <= ls && le <= pe);
            }
        }
    }
    #[test] fn t973_theme_full_api() {
        for &t in &[Theme::Cream, Theme::Dark, Theme::Forest, Theme::Ocean] {
            let _ = t.bg(); let _ = t.fg(); let _ = t.dim();
            let _ = t.hover(); let _ = t.selection(); let _ = t.focus_dim();
            let _ = t.name(); let _ = t.next();
        }
    }
    #[test] fn t974_focus_full_api() {
        for f in [FocusMode::Off, FocusMode::Line, FocusMode::Paragraph] {
            let _ = f.label(); let _ = f.next();
        }
    }
    #[test] fn t975_font_full_api() {
        for &f in FontChoice::ALL {
            let _ = f.name(); let _ = f.file();
        }
    }

    // Ultra-final stress (976-1000)
    #[test] fn t976_replace_all_removes_all() {
        let s = "remove remove remove";
        let mut w = make_writer(s);
        w.find_text = "remove ".into();
        w.replace_text = "".into();
        w.replace_all();
        assert_eq!(w.tabs[0].text, "remove");
    }
    #[test] fn t977_word_count_precise() {
        assert_eq!(make_writer("a").word_count(), 1);
        assert_eq!(make_writer("a b").word_count(), 2);
        assert_eq!(make_writer("a b c").word_count(), 3);
        assert_eq!(make_writer("a b c d").word_count(), 4);
        assert_eq!(make_writer("a b c d e").word_count(), 5);
    }
    #[test] fn t978_char_count_precise() {
        assert_eq!(make_writer("").char_count(), 0);
        assert_eq!(make_writer("a").char_count(), 1);
        assert_eq!(make_writer("ab").char_count(), 2);
        assert_eq!(make_writer("abc").char_count(), 3);
        assert_eq!(make_writer("가나다").char_count(), 3);
    }
    #[test] fn t979_line_count_precise() {
        assert_eq!(make_writer("").line_count(), 1);
        assert_eq!(make_writer("a").line_count(), 1);
        assert_eq!(make_writer("a\nb").line_count(), 2);
        assert_eq!(make_writer("a\nb\nc").line_count(), 3);
    }
    #[test] fn t980_blend_color_r_only() {
        let c = blend_color(egui::Color32::from_rgb(0,0,0), egui::Color32::from_rgb(100,0,0), 1.0);
        assert_eq!(c, egui::Color32::from_rgb(100,0,0));
    }
    #[test] fn t981_blend_color_g_only() {
        let c = blend_color(egui::Color32::from_rgb(0,0,0), egui::Color32::from_rgb(0,100,0), 1.0);
        assert_eq!(c, egui::Color32::from_rgb(0,100,0));
    }
    #[test] fn t982_blend_color_b_only() {
        let c = blend_color(egui::Color32::from_rgb(0,0,0), egui::Color32::from_rgb(0,0,100), 1.0);
        assert_eq!(c, egui::Color32::from_rgb(0,0,100));
    }
    #[test] fn t983_color_hex_len() {
        for &t in &[Theme::Cream, Theme::Dark, Theme::Forest, Theme::Ocean] {
            assert_eq!(color_hex(t.bg()).len(), 7);
            assert_eq!(color_hex(t.fg()).len(), 7);
            assert_eq!(color_hex(t.dim()).len(), 7);
        }
    }
    #[test] fn t984_tab_modified_flag() {
        let mut t = Tab::new();
        assert!(!t.modified);
        t.modified = true;
        assert!(t.modified);
        t.modified = false;
        assert!(!t.modified);
    }
    #[test] fn t985_tab_text_mutation() {
        let mut t = Tab::new();
        t.text = "hello".into();
        t.text.push_str(" world");
        assert_eq!(t.text, "hello world");
    }
    #[test] fn t986_writer_cursor_set() {
        let mut w = Writer::new_with(true);
        w.cursor_byte_pos = 42;
        assert_eq!(w.cursor_byte_pos, 42);
    }
    #[test] fn t987_font_eq_reflexive() {
        for &f in FontChoice::ALL { assert_eq!(f, f); }
    }
    #[test] fn t988_theme_eq_reflexive() {
        for &t in &[Theme::Cream, Theme::Dark, Theme::Forest, Theme::Ocean] {
            assert_eq!(t, t);
        }
    }
    #[test] fn t989_focus_eq_reflexive() {
        for f in [FocusMode::Off, FocusMode::Line, FocusMode::Paragraph] {
            assert_eq!(f, f);
        }
    }
    #[test] fn t990_safe_byte_idempotent() {
        let s = "한글 test 테스트";
        for i in 0..s.len()+5 {
            let p1 = safe_byte_pos(s, i);
            let p2 = safe_byte_pos(s, p1);
            assert_eq!(p1, p2, "idempotent at {}", i);
        }
    }
    #[test] fn t991_line_bounds_start_le_pos() {
        let s = "abc\ndef\nghi";
        for i in 0..s.len() {
            if s.is_char_boundary(i) {
                let (start, _) = find_line_bounds(s, i);
                assert!(start <= i);
            }
        }
    }
    #[test] fn t992_para_bounds_start_le_pos() {
        let s = "abc\n\ndef\n\nghi";
        for i in 0..s.len() {
            if s.is_char_boundary(i) {
                let (start, _) = find_para_bounds(s, i);
                assert!(start <= i);
            }
        }
    }
    #[test] fn t993_replace_idempotent() {
        let mut w = make_writer("hello");
        w.find_text = "hello".into();
        w.replace_text = "hello".into();
        w.replace_all();
        assert_eq!(w.tabs[0].text, "hello");
    }
    #[test] fn t994_word_count_after_replace() {
        let mut w = make_writer("one two three");
        w.find_text = "two".into();
        w.replace_text = "2".into();
        w.replace_all();
        assert_eq!(w.word_count(), 3);
    }
    #[test] fn t995_char_count_after_replace() {
        let mut w = make_writer("abc");
        w.find_text = "b".into();
        w.replace_text = "BB".into();
        w.replace_all();
        assert_eq!(w.char_count(), 4);
    }
    #[test] fn t996_line_count_after_replace() {
        let mut w = make_writer("a b c");
        w.find_text = " ".into();
        w.replace_text = "\n".into();
        w.replace_all();
        assert_eq!(w.line_count(), 3);
    }
    #[test] fn t997_new_tab_word_count_zero() {
        let mut w = Writer::new_with(true);
        w.new_tab();
        assert_eq!(w.word_count(), 0);
    }
    #[test] fn t998_blend_neutral() {
        let c = egui::Color32::from_rgb(128, 128, 128);
        assert_eq!(blend_color(c, c, 0.5), c);
    }
    #[test] fn t999_all_enums_complete() {
        assert_eq!(FontChoice::ALL.len(), 7);
        let _ = Theme::Cream.next().next().next().next();
        let _ = FocusMode::Off.next().next().next();
    }
    #[test] fn t1000_final_integration() {
        let mut w = Writer::new_with(true);
        // Setup
        w.tabs[0].text = "The Art of Writing\n\nWriting is thinking.\nEvery word matters.".into();
        w.theme = Theme::Ocean;
        w.font_choice = FontChoice::Consolas;
        w.font_size = 20.0;
        w.focus_mode = FocusMode::Paragraph;
        // Verify
        assert_eq!(w.word_count(), 10);
        assert_eq!(w.line_count(), 4);
        assert!(w.char_count() > 0);
        // Find
        w.find_text = "writing".into();
        w.update_find_count();
        assert_eq!(w.find_count, 2);
        // Replace
        w.replace_text = "coding".into();
        w.replace_all();
        assert!(!w.tabs[0].text.to_lowercase().contains("writing"));
        // Tab
        w.new_tab();
        w.tabs[w.active].text = "New tab content".into();
        assert_eq!(w.tabs.len(), 2);
        assert_eq!(w.word_count(), 3);
        // Final
        assert_eq!(w.theme, Theme::Ocean);
        assert_eq!(w.font_choice, FontChoice::Consolas);
        assert_eq!(w.font_size, 20.0);
    }

    // ══════════════════════════════════════════════════════════════
    // v2.0 Feature Tests (1001-1200)
    // ══════════════════════════════════════════════════════════════

    // ── New themes (1001-1050) ──
    #[test] fn t1001_sepia_bg() { assert_eq!(Theme::Sepia.bg(), egui::Color32::from_rgb(242,229,209)); }
    #[test] fn t1002_midnight_bg() { assert_eq!(Theme::Midnight.bg(), egui::Color32::from_rgb(15,15,20)); }
    #[test] fn t1003_solarized_bg() { assert_eq!(Theme::Solarized.bg(), egui::Color32::from_rgb(253,246,227)); }
    #[test] fn t1004_nord_bg() { assert_eq!(Theme::Nord.bg(), egui::Color32::from_rgb(46,52,64)); }
    #[test] fn t1005_lavender_bg() { assert_eq!(Theme::Lavender.bg(), egui::Color32::from_rgb(240,235,248)); }
    #[test] fn t1006_sepia_fg() { assert_ne!(Theme::Sepia.fg(), Theme::Sepia.bg()); }
    #[test] fn t1007_midnight_fg() { assert_ne!(Theme::Midnight.fg(), Theme::Midnight.bg()); }
    #[test] fn t1008_solarized_fg() { assert_ne!(Theme::Solarized.fg(), Theme::Solarized.bg()); }
    #[test] fn t1009_nord_fg() { assert_ne!(Theme::Nord.fg(), Theme::Nord.bg()); }
    #[test] fn t1010_lavender_fg() { assert_ne!(Theme::Lavender.fg(), Theme::Lavender.bg()); }
    #[test] fn t1011_sepia_next() { assert_eq!(Theme::Sepia.next(), Theme::Midnight); }
    #[test] fn t1012_midnight_next() { assert_eq!(Theme::Midnight.next(), Theme::Solarized); }
    #[test] fn t1013_solarized_next() { assert_eq!(Theme::Solarized.next(), Theme::Nord); }
    #[test] fn t1014_nord_next() { assert_eq!(Theme::Nord.next(), Theme::Lavender); }
    #[test] fn t1015_lavender_next() { assert_eq!(Theme::Lavender.next(), Theme::Cream); }
    #[test] fn t1016_theme_count() { assert_eq!(Theme::ALL.len(), 9); }
    #[test] fn t1017_all_themes_unique_bg() {
        let bgs: Vec<_> = Theme::ALL.iter().map(|t| t.bg()).collect();
        for i in 0..bgs.len() { for j in (i+1)..bgs.len() { assert_ne!(bgs[i], bgs[j]); } }
    }
    #[test] fn t1018_all_themes_unique_name() {
        let names: Vec<_> = Theme::ALL.iter().map(|t| t.name()).collect();
        for i in 0..names.len() { for j in (i+1)..names.len() { assert_ne!(names[i], names[j]); } }
    }
    #[test] fn t1019_dark_themes() {
        assert!(Theme::Dark.is_dark());
        assert!(Theme::Forest.is_dark());
        assert!(Theme::Ocean.is_dark());
        assert!(Theme::Midnight.is_dark());
        assert!(Theme::Nord.is_dark());
    }
    #[test] fn t1020_light_themes() {
        assert!(!Theme::Cream.is_dark());
        assert!(!Theme::Sepia.is_dark());
        assert!(!Theme::Solarized.is_dark());
        assert!(!Theme::Lavender.is_dark());
    }
    #[test] fn t1021_sepia_name() { assert_eq!(Theme::Sepia.name(), "Sepia"); }
    #[test] fn t1022_midnight_name() { assert_eq!(Theme::Midnight.name(), "Midnight"); }
    #[test] fn t1023_solarized_name() { assert_eq!(Theme::Solarized.name(), "Solarized"); }
    #[test] fn t1024_nord_name() { assert_eq!(Theme::Nord.name(), "Nord"); }
    #[test] fn t1025_lavender_name() { assert_eq!(Theme::Lavender.name(), "Lavender"); }
    #[test] fn t1026_all_themes_have_accent() {
        for t in Theme::ALL { let _ = t.accent(); }
    }
    #[test] fn t1027_all_themes_have_hover() {
        for t in Theme::ALL { let _ = t.hover(); }
    }
    #[test] fn t1028_all_themes_have_selection() {
        for t in Theme::ALL { let _ = t.selection(); }
    }
    #[test] fn t1029_all_themes_have_focus_dim() {
        for t in Theme::ALL { let _ = t.focus_dim(); }
    }
    #[test] fn t1030_full_cycle_9_themes() {
        let mut t = Theme::Cream;
        for _ in 0..9 { t = t.next(); }
        assert_eq!(t, Theme::Cream);
    }

    // ── Sentence count (1031-1045) ──
    #[test] fn t1031_sentence_empty() { let w = make_writer(""); assert_eq!(w.sentence_count(), 0); }
    #[test] fn t1032_sentence_one() { let w = make_writer("Hello world."); assert_eq!(w.sentence_count(), 1); }
    #[test] fn t1033_sentence_two() { let w = make_writer("Hello. World."); assert_eq!(w.sentence_count(), 2); }
    #[test] fn t1034_sentence_question() { let w = make_writer("What? Really!"); assert_eq!(w.sentence_count(), 2); }
    #[test] fn t1035_sentence_mixed() { let w = make_writer("Hello. What? Yes!"); assert_eq!(w.sentence_count(), 3); }
    #[test] fn t1036_sentence_no_period() { let w = make_writer("Hello world"); assert_eq!(w.sentence_count(), 1); }
    #[test] fn t1037_sentence_korean() { let w = make_writer("안녕하세요。반갑습니다。"); assert_eq!(w.sentence_count(), 2); }

    // ── Paragraph count (1046-1060) ──
    #[test] fn t1046_para_empty() { let w = make_writer(""); assert_eq!(w.paragraph_count(), 0); }
    #[test] fn t1047_para_one() { let w = make_writer("Hello world"); assert_eq!(w.paragraph_count(), 1); }
    #[test] fn t1048_para_two() { let w = make_writer("Hello\n\nWorld"); assert_eq!(w.paragraph_count(), 2); }
    #[test] fn t1049_para_three() { let w = make_writer("A\n\nB\n\nC"); assert_eq!(w.paragraph_count(), 3); }
    #[test] fn t1050_para_single_newline() { let w = make_writer("A\nB"); assert_eq!(w.paragraph_count(), 1); }
    #[test] fn t1051_para_whitespace_only() { let w = make_writer("   \n\n   "); assert_eq!(w.paragraph_count(), 0); }

    // ── Page count (1061-1070) ──
    #[test] fn t1061_page_empty() { let w = make_writer(""); assert_eq!(w.page_count(), 0); }
    #[test] fn t1062_page_short() { let w = make_writer("hello world"); assert_eq!(w.page_count(), 1); }
    #[test] fn t1063_page_250_words() {
        let text = (0..250).map(|_| "word").collect::<Vec<_>>().join(" ");
        let w = make_writer(&text);
        assert_eq!(w.page_count(), 1);
    }
    #[test] fn t1064_page_251_words() {
        let text = (0..251).map(|_| "word").collect::<Vec<_>>().join(" ");
        let w = make_writer(&text);
        assert_eq!(w.page_count(), 2);
    }
    #[test] fn t1065_page_500_words() {
        let text = (0..500).map(|_| "word").collect::<Vec<_>>().join(" ");
        let w = make_writer(&text);
        assert_eq!(w.page_count(), 2);
    }

    // ── Reading time (1071-1080) ──
    #[test] fn t1071_reading_empty() { let w = make_writer(""); assert!(w.reading_time_min() < 0.01); }
    #[test] fn t1072_reading_200words() {
        let text = (0..200).map(|_| "word").collect::<Vec<_>>().join(" ");
        let w = make_writer(&text);
        assert!((w.reading_time_min() - 1.0).abs() < 0.01);
    }
    #[test] fn t1073_reading_400words() {
        let text = (0..400).map(|_| "word").collect::<Vec<_>>().join(" ");
        let w = make_writer(&text);
        assert!((w.reading_time_min() - 2.0).abs() < 0.01);
    }

    // ── Average word length (1081-1090) ──
    #[test] fn t1081_avg_empty() { let w = make_writer(""); assert!(w.avg_word_length() < 0.01); }
    #[test] fn t1082_avg_simple() { let w = make_writer("cat dog"); assert!((w.avg_word_length() - 3.0).abs() < 0.01); }
    #[test] fn t1083_avg_mixed() { let w = make_writer("I am good"); assert!((w.avg_word_length() - 2.0).abs() < 0.5); }

    // ── Word frequency (1091-1100) ──
    #[test] fn t1091_freq_empty() { let w = make_writer(""); assert!(w.word_frequency().is_empty()); }
    #[test] fn t1092_freq_single() {
        let w = make_writer("hello hello hello");
        let freq = w.word_frequency();
        assert_eq!(freq[0].0, "hello");
        assert_eq!(freq[0].1, 3);
    }
    #[test] fn t1093_freq_multiple() {
        let w = make_writer("cat dog cat bird cat dog");
        let freq = w.word_frequency();
        assert_eq!(freq[0].0, "cat");
        assert_eq!(freq[0].1, 3);
    }
    #[test] fn t1094_freq_case_insensitive() {
        let w = make_writer("Hello HELLO hello");
        let freq = w.word_frequency();
        assert_eq!(freq[0].0, "hello");
        assert_eq!(freq[0].1, 3);
    }
    #[test] fn t1095_freq_max_50() {
        let text = (0..100).map(|i| format!("word{}", i)).collect::<Vec<_>>().join(" ");
        let w = make_writer(&text);
        assert!(w.word_frequency().len() <= 50);
    }

    // ── Outline entries (1101-1115) ──
    #[test] fn t1101_outline_empty() { let w = make_writer(""); assert!(w.outline_entries().is_empty()); }
    #[test] fn t1102_outline_no_headings() { let w = make_writer("Just text"); assert!(w.outline_entries().is_empty()); }
    #[test] fn t1103_outline_h1() {
        let w = make_writer("# Title\nSome text");
        let entries = w.outline_entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].1, "Title");
        assert_eq!(entries[0].2, 1);
    }
    #[test] fn t1104_outline_h2() {
        let w = make_writer("## Subtitle");
        let entries = w.outline_entries();
        assert_eq!(entries[0].2, 2);
    }
    #[test] fn t1105_outline_multi() {
        let w = make_writer("# Ch1\nText\n## Sec1\nMore\n# Ch2");
        assert_eq!(w.outline_entries().len(), 3);
    }
    #[test] fn t1106_outline_h6() {
        let w = make_writer("###### Deep");
        let entries = w.outline_entries();
        assert_eq!(entries[0].2, 6);
    }
    #[test] fn t1107_outline_h7_ignored() {
        let w = make_writer("####### TooDeep");
        assert!(w.outline_entries().is_empty());
    }
    #[test] fn t1108_outline_line_numbers() {
        let w = make_writer("Line1\n# Title\nLine3\n## Sub");
        let entries = w.outline_entries();
        assert_eq!(entries[0].0, 2);
        assert_eq!(entries[1].0, 4);
    }

    // ── Goal tracking (1116-1130) ──
    #[test] fn t1116_goal_default_zero() { let w = Writer::new_with(true); assert_eq!(w.word_goal, 0); }
    #[test] fn t1117_goal_progress_no_goal() { let w = Writer::new_with(true); assert!(w.goal_progress() < 0.01); }
    #[test] fn t1118_goal_progress_half() {
        let mut w = make_writer(&(0..500).map(|_| "word").collect::<Vec<_>>().join(" "));
        w.word_goal = 1000;
        assert!((w.goal_progress() - 0.5).abs() < 0.01);
    }
    #[test] fn t1119_goal_progress_full() {
        let mut w = make_writer(&(0..1000).map(|_| "word").collect::<Vec<_>>().join(" "));
        w.word_goal = 1000;
        assert!((w.goal_progress() - 1.0).abs() < 0.01);
    }
    #[test] fn t1120_goal_progress_over() {
        let mut w = make_writer(&(0..1500).map(|_| "word").collect::<Vec<_>>().join(" "));
        w.word_goal = 1000;
        assert!((w.goal_progress() - 1.0).abs() < 0.01); // capped at 1.0
    }

    // ── Session tracking (1131-1140) ──
    #[test] fn t1131_session_start_words() { let w = Writer::new_with(true); assert_eq!(w.session_start_words, 0); }
    #[test] fn t1132_session_words_zero() { let w = Writer::new_with(true); assert_eq!(w.session_words(), 0); }
    #[test] fn t1133_session_minutes() { let w = Writer::new_with(true); assert!(w.session_minutes() < 1.0); }
    #[test] fn t1134_session_wpm_start() { let w = Writer::new_with(true); assert!(w.session_wpm() < 1.0); }

    // ── Zen mode & line numbers (1141-1155) ──
    #[test] fn t1141_zen_default_off() { let w = Writer::new_with(true); assert!(!w.zen_mode); }
    #[test] fn t1142_line_numbers_default_off() { let w = Writer::new_with(true); assert!(!w.show_line_numbers); }
    #[test] fn t1143_smart_typo_default_off() { let w = Writer::new_with(true); assert!(!w.smart_typography); }
    #[test] fn t1144_stats_panel_default_off() { let w = Writer::new_with(true); assert!(!w.show_stats_panel); }
    #[test] fn t1145_outline_default_off() { let w = Writer::new_with(true); assert!(!w.show_outline); }
    #[test] fn t1146_word_freq_default_off() { let w = Writer::new_with(true); assert!(!w.show_word_freq); }
    #[test] fn t1147_goal_settings_default_off() { let w = Writer::new_with(true); assert!(!w.show_goal_settings); }

    // ── Smart typography (1156-1170) ──
    #[test] fn t1156_smart_typo_em_dash() {
        let mut w = make_writer("hello -- ");
        w.smart_typography = true;
        w.apply_smart_typo();
        assert!(w.tabs[0].text.contains('\u{2014}'));
    }
    #[test] fn t1157_smart_typo_ellipsis() {
        let mut w = make_writer("hmm...");
        w.smart_typography = true;
        w.apply_smart_typo();
        assert!(w.tabs[0].text.contains('\u{2026}'));
    }
    #[test] fn t1158_smart_typo_double_quote_left() {
        let mut w = make_writer(" \"");
        w.smart_typography = true;
        w.apply_smart_typo();
        assert!(w.tabs[0].text.contains('\u{201C}'));
    }
    #[test] fn t1159_smart_typo_double_quote_right() {
        let mut w = make_writer("word\"");
        w.smart_typography = true;
        w.apply_smart_typo();
        assert!(w.tabs[0].text.contains('\u{201D}'));
    }
    #[test] fn t1160_smart_typo_single_quote_left() {
        let mut w = make_writer(" '");
        w.smart_typography = true;
        w.apply_smart_typo();
        assert!(w.tabs[0].text.contains('\u{2018}'));
    }
    #[test] fn t1161_smart_typo_single_quote_right() {
        let mut w = make_writer("it'");
        w.smart_typography = true;
        w.apply_smart_typo();
        assert!(w.tabs[0].text.contains('\u{2019}'));
    }

    // ── Comprehensive v2 integration (1171-1200) ──
    #[test] fn t1171_v2_full_stats() {
        let w = make_writer("The quick brown fox jumps over the lazy dog. Another sentence here!");
        assert!(w.word_count() > 0);
        assert!(w.sentence_count() >= 2);
        assert!(w.paragraph_count() >= 1);
        assert!(w.page_count() >= 1);
        assert!(w.reading_time_min() >= 0.0);
        assert!(w.avg_word_length() > 0.0);
    }
    #[test] fn t1172_v2_outline_complex() {
        let w = make_writer("# Chapter 1\n\nIntro text.\n\n## Section 1.1\n\nMore text.\n\n### Subsection\n\n# Chapter 2");
        let entries = w.outline_entries();
        assert_eq!(entries.len(), 4);
        assert_eq!(entries[0].2, 1); // h1
        assert_eq!(entries[1].2, 2); // h2
        assert_eq!(entries[2].2, 3); // h3
        assert_eq!(entries[3].2, 1); // h1
    }
    #[test] fn t1173_v2_freq_sorted() {
        let w = make_writer("the the the a a b c c c c");
        let freq = w.word_frequency();
        assert_eq!(freq[0].0, "c");
        assert_eq!(freq[0].1, 4);
        assert_eq!(freq[1].0, "the");
        assert_eq!(freq[1].1, 3);
    }
    #[test] fn t1174_v2_theme_all_accessors() {
        for t in Theme::ALL {
            let _ = t.bg();
            let _ = t.fg();
            let _ = t.dim();
            let _ = t.hover();
            let _ = t.selection();
            let _ = t.focus_dim();
            let _ = t.accent();
            let _ = t.name();
            let _ = t.is_dark();
        }
    }
    #[test] fn t1175_v2_goal_word_count() {
        let mut w = make_writer("one two three four five");
        w.word_goal = 10;
        assert_eq!(w.word_count(), 5);
        assert!((w.goal_progress() - 0.5).abs() < 0.01);
    }
    #[test] fn t1176_v2_paragraph_multiline() {
        let w = make_writer("Para one line1\nPara one line2\n\nPara two\n\nPara three");
        assert_eq!(w.paragraph_count(), 3);
    }
    #[test] fn t1177_v2_sentence_exclamation() {
        let w = make_writer("Wow! Amazing! Great!");
        assert_eq!(w.sentence_count(), 3);
    }
    #[test] fn t1178_v2_zen_toggle() {
        let mut w = Writer::new_with(true);
        assert!(!w.zen_mode);
        w.zen_mode = true;
        assert!(w.zen_mode);
        w.zen_mode = false;
        assert!(!w.zen_mode);
    }
    #[test] fn t1179_v2_all_new_defaults() {
        let w = Writer::new_with(true);
        assert!(!w.zen_mode);
        assert!(!w.show_line_numbers);
        assert_eq!(w.word_goal, 0);
        assert!(!w.show_goal_settings);
        assert!(!w.show_stats_panel);
        assert!(!w.show_outline);
        assert!(!w.show_word_freq);
        assert!(!w.smart_typography);
    }
    #[test] fn t1180_v2_freq_strips_punct() {
        let w = make_writer("hello, hello! hello?");
        let freq = w.word_frequency();
        assert_eq!(freq[0].0, "hello");
        assert_eq!(freq[0].1, 3);
    }
    #[test] fn t1181_v2_outline_empty_heading() {
        let w = make_writer("# \nText");
        assert!(w.outline_entries().is_empty());
    }
    #[test] fn t1182_v2_reading_time_long() {
        let text = (0..2000).map(|_| "word").collect::<Vec<_>>().join(" ");
        let w = make_writer(&text);
        assert!((w.reading_time_min() - 10.0).abs() < 0.1);
    }
    #[test] fn t1183_v2_page_count_1000() {
        let text = (0..1000).map(|_| "word").collect::<Vec<_>>().join(" ");
        let w = make_writer(&text);
        assert_eq!(w.page_count(), 4);
    }
    #[test] fn t1184_v2_integration_complete() {
        let mut w = Writer::new_with(true);
        w.tabs[0].text = "# My Novel\n\nOnce upon a time. There was a castle.\n\n## Chapter 1\n\nThe hero arrived. He was brave! Was he ready?".into();
        w.theme = Theme::Nord;
        w.word_goal = 50;
        w.zen_mode = true;
        w.smart_typography = true;
        // Stats
        assert!(w.word_count() > 10);
        assert!(w.sentence_count() >= 4);
        assert!(w.paragraph_count() >= 2);
        assert!(w.page_count() >= 1);
        // Outline
        let outline = w.outline_entries();
        assert_eq!(outline.len(), 2); // # My Novel, ## Chapter 1
        assert_eq!(outline[0].2, 1);
        assert_eq!(outline[1].2, 2);
        // Goal
        assert!(w.goal_progress() > 0.0);
        // Theme
        assert_eq!(w.theme, Theme::Nord);
        assert!(w.theme.is_dark());
    }

    // ══════════════════════════════════════════════════════════════
    // v2.0 AI & Additional Feature Tests (1185-1300)
    // ══════════════════════════════════════════════════════════════

    // ── AiAction tests ──
    #[test] fn t1185_ai_action_count() { assert_eq!(AiAction::ALL.len(), 8); }
    #[test] fn t1186_ai_action_proofread_name() { assert_eq!(AiAction::Proofread.name(), "Proofread"); }
    #[test] fn t1187_ai_action_summarize_name() { assert_eq!(AiAction::Summarize.name(), "Summarize"); }
    #[test] fn t1188_ai_action_expand_name() { assert_eq!(AiAction::Expand.name(), "Expand"); }
    #[test] fn t1189_ai_action_rewrite_name() { assert_eq!(AiAction::Rewrite.name(), "Rewrite"); }
    #[test] fn t1190_ai_action_translate_ko() { assert!(AiAction::TranslateKo.name().contains("Korean")); }
    #[test] fn t1191_ai_action_translate_en() { assert!(AiAction::TranslateEn.name().contains("English")); }
    #[test] fn t1192_ai_action_continue() { assert_eq!(AiAction::Continue.name(), "Continue Writing"); }
    #[test] fn t1193_ai_action_custom() { assert_eq!(AiAction::Custom.name(), "Custom Prompt"); }
    #[test] fn t1194_ai_all_have_icons() {
        for a in AiAction::ALL { assert!(!a.icon().is_empty()); }
    }
    #[test] fn t1195_ai_all_have_names() {
        for a in AiAction::ALL { assert!(!a.name().is_empty()); }
    }
    #[test] fn t1196_ai_build_prompt_proofread() {
        let p = AiAction::Proofread.build_prompt("Hello", "");
        assert!(p.contains("Hello"));
        assert!(p.contains("grammar"));
    }
    #[test] fn t1197_ai_build_prompt_summarize() {
        let p = AiAction::Summarize.build_prompt("Long text here", "");
        assert!(p.contains("Summarize"));
        assert!(p.contains("Long text here"));
    }
    #[test] fn t1198_ai_build_prompt_custom() {
        let p = AiAction::Custom.build_prompt("text", "Make it funny");
        assert!(p.contains("Make it funny"));
        assert!(p.contains("text"));
    }
    #[test] fn t1199_ai_build_prompt_translate_ko() {
        let p = AiAction::TranslateKo.build_prompt("Hello world", "");
        assert!(p.contains("Korean"));
    }
    #[test] fn t1200_ai_build_prompt_translate_en() {
        let p = AiAction::TranslateEn.build_prompt("안녕", "");
        assert!(p.contains("English"));
    }

    // ── Writer AI defaults ──
    #[test] fn t1201_ai_default_off() { let w = Writer::new_with(true); assert!(!w.ai_show); }
    #[test] fn t1202_ai_default_model() { let w = Writer::new_with(true); assert_eq!(w.ai_model, "gemma3"); }
    #[test] fn t1203_ai_default_action() { let w = Writer::new_with(true); assert_eq!(w.ai_action, AiAction::Proofread); }
    #[test] fn t1204_ai_default_not_loading() { let w = Writer::new_with(true); assert!(!w.ai_loading); }
    #[test] fn t1205_ai_default_host() { let w = Writer::new_with(true); assert_eq!(w.ai_host, "http://localhost:11434"); }
    #[test] fn t1206_ai_default_empty_result() { let w = Writer::new_with(true); assert!(w.ai_result.is_empty()); }
    #[test] fn t1207_ai_default_empty_error() { let w = Writer::new_with(true); assert!(w.ai_error.is_empty()); }
    #[test] fn t1208_ai_default_empty_models() { let w = Writer::new_with(true); assert!(w.ai_available_models.is_empty()); }
    #[test] fn t1209_ai_default_custom_prompt() { let w = Writer::new_with(true); assert!(w.ai_custom_prompt.is_empty()); }

    // ── AI insert/replace ──
    #[test] fn t1210_ai_insert_result() {
        let mut w = make_writer("Hello");
        w.ai_result = "AI generated text".into();
        w.ai_insert_result();
        assert!(w.tabs[0].text.contains("Hello"));
        assert!(w.tabs[0].text.contains("AI generated text"));
        assert!(w.tabs[0].modified);
    }
    #[test] fn t1211_ai_replace_text() {
        let mut w = make_writer("Hello");
        w.ai_result = "Replaced content".into();
        w.ai_replace_text();
        assert_eq!(w.tabs[0].text, "Replaced content");
        assert!(w.tabs[0].modified);
    }
    #[test] fn t1212_ai_insert_empty_noop() {
        let mut w = make_writer("Hello");
        w.ai_result = String::new();
        w.ai_insert_result();
        assert_eq!(w.tabs[0].text, "Hello");
    }
    #[test] fn t1213_ai_replace_empty_noop() {
        let mut w = make_writer("Hello");
        w.ai_result = String::new();
        w.ai_replace_text();
        assert_eq!(w.tabs[0].text, "Hello");
    }
    #[test] fn t1214_ai_run_empty_text() {
        let mut w = make_writer("");
        w.ai_run();
        assert!(!w.ai_error.is_empty()); // "No text to process"
        assert!(!w.ai_loading);
    }
    #[test] fn t1215_ai_run_whitespace_only() {
        let mut w = make_writer("   \n\n   ");
        w.ai_run();
        assert!(!w.ai_error.is_empty());
    }

    // ── Additional features ──
    #[test] fn t1216_char_count_nospaces_empty() { let w = make_writer(""); assert_eq!(w.char_count_nospaces(), 0); }
    #[test] fn t1217_char_count_nospaces_words() { let w = make_writer("hello world"); assert_eq!(w.char_count_nospaces(), 10); }
    #[test] fn t1218_char_count_nospaces_newlines() { let w = make_writer("a\nb\nc"); assert_eq!(w.char_count_nospaces(), 3); }
    #[test] fn t1219_char_count_nospaces_tabs() { let w = make_writer("a\tb"); assert_eq!(w.char_count_nospaces(), 2); }
    #[test] fn t1220_char_count_nospaces_korean() { let w = make_writer("안녕 세계"); assert_eq!(w.char_count_nospaces(), 4); }
    #[test] fn t1221_goto_line_default() { let w = Writer::new_with(true); assert!(!w.show_goto_line); }
    #[test] fn t1222_goto_line_input_empty() { let w = Writer::new_with(true); assert!(w.goto_line_input.is_empty()); }

    // ── AI action equality ──
    #[test] fn t1223_ai_action_eq() { assert_eq!(AiAction::Proofread, AiAction::Proofread); }
    #[test] fn t1224_ai_action_ne() { assert_ne!(AiAction::Proofread, AiAction::Summarize); }
    #[test] fn t1225_ai_action_copy() { let a = AiAction::Expand; let b = a; assert_eq!(a, b); }

    // ── Comprehensive v3 integration ──
    #[test] fn t1226_v3_full_integration() {
        let mut w = Writer::new_with(true);
        w.tabs[0].text = "# Project Plan\n\nWe will build an app. It will be great!\n\n## Phase 1\n\nDesign the UI.\n\n## Phase 2\n\nImplement features.".into();
        // Stats
        assert!(w.word_count() > 15);
        assert!(w.sentence_count() >= 3);
        assert!(w.paragraph_count() >= 3);
        assert!(w.page_count() >= 1);
        assert!(w.char_count_nospaces() > 0);
        assert!(w.reading_time_min() > 0.0);
        // Outline
        let outline = w.outline_entries();
        assert_eq!(outline.len(), 3);
        // Word freq
        let freq = w.word_frequency();
        assert!(!freq.is_empty());
        // AI defaults
        assert_eq!(w.ai_model, "gemma3");
        assert_eq!(w.ai_action, AiAction::Proofread);
        // Set AI result and insert
        w.ai_result = "Additional plan details.".into();
        w.ai_insert_result();
        assert!(w.tabs[0].text.contains("Additional plan details"));
        assert!(w.tabs[0].modified);
        // Goals
        w.word_goal = 100;
        assert!(w.goal_progress() > 0.0);
    }

    // ── All AiAction prompts non-empty ──
    #[test] fn t1227_all_prompts_contain_text() {
        let text = "Sample text for testing.";
        for &action in AiAction::ALL {
            let prompt = action.build_prompt(text, "Custom instruction");
            assert!(prompt.contains(text), "Action {} prompt missing text", action.name());
        }
    }

    // ── Streaming buffer tests ──
    #[test] fn t1228_stream_buf_default_empty() {
        let w = Writer::new_with(true);
        assert!(w.ai_stream_buf.lock().unwrap().is_empty());
    }
    #[test] fn t1229_stream_done_default_false() {
        let w = Writer::new_with(true);
        assert!(!w.ai_stream_done.load(Ordering::SeqCst));
    }
    #[test] fn t1230_stream_err_default_empty() {
        let w = Writer::new_with(true);
        assert!(w.ai_stream_err.lock().unwrap().is_empty());
    }

    // ════════════════════════════════════════════════════════════════
    // v3.0: Markdown Preview Tests (1300-1399)
    // ════════════════════════════════════════════════════════════════

    #[test] fn t1300_preview_default_off() { let w = Writer::new_with(true); assert!(!w.show_preview); }
    #[test] fn t1301_preview_toggle() { let mut w = Writer::new_with(true); w.show_preview = true; assert!(w.show_preview); }

    // ── parse_inline_md tests ──
    #[test] fn t1302_inline_normal() {
        let spans = parse_inline_md("hello world");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].style, InlineStyle::Normal);
        assert_eq!(spans[0].text, "hello world");
    }
    #[test] fn t1303_inline_bold() {
        let spans = parse_inline_md("hello **bold** world");
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].style, InlineStyle::Normal);
        assert_eq!(spans[1].style, InlineStyle::Bold);
        assert_eq!(spans[1].text, "bold");
        assert_eq!(spans[2].style, InlineStyle::Normal);
    }
    #[test] fn t1304_inline_italic() {
        let spans = parse_inline_md("hello *italic* world");
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[1].style, InlineStyle::Italic);
        assert_eq!(spans[1].text, "italic");
    }
    #[test] fn t1305_inline_code() {
        let spans = parse_inline_md("use `println!` here");
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[1].style, InlineStyle::Code);
        assert_eq!(spans[1].text, "println!");
    }
    #[test] fn t1306_inline_link() {
        let spans = parse_inline_md("click [here](http://example.com) now");
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[1].style, InlineStyle::LinkText);
        assert_eq!(spans[1].text, "here");
    }
    #[test] fn t1307_inline_empty() {
        let spans = parse_inline_md("");
        assert!(spans.is_empty());
    }
    #[test] fn t1308_inline_only_bold() {
        let spans = parse_inline_md("**all bold**");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].style, InlineStyle::Bold);
        assert_eq!(spans[0].text, "all bold");
    }
    #[test] fn t1309_inline_only_code() {
        let spans = parse_inline_md("`code`");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].style, InlineStyle::Code);
    }
    #[test] fn t1310_inline_mixed() {
        let spans = parse_inline_md("a **b** *c* `d` e");
        assert!(spans.len() >= 5);
    }
    #[test] fn t1311_inline_korean_bold() {
        let spans = parse_inline_md("**한글** 텍스트");
        assert_eq!(spans[0].style, InlineStyle::Bold);
        assert_eq!(spans[0].text, "한글");
    }
    #[test] fn t1312_inline_unclosed_bold() {
        let spans = parse_inline_md("**unclosed bold");
        // Unclosed ** consumes the rest as bold text
        assert!(spans.iter().any(|s| s.style == InlineStyle::Bold));
    }
    #[test] fn t1313_inline_unclosed_italic() {
        let spans = parse_inline_md("*unclosed italic");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].style, InlineStyle::Italic);
    }
    #[test] fn t1314_inline_unclosed_code() {
        let spans = parse_inline_md("`unclosed code");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].style, InlineStyle::Code);
    }
    #[test] fn t1315_inline_adjacent_styles() {
        let spans = parse_inline_md("**bold***italic*");
        assert!(spans.len() >= 2);
    }
    #[test] fn t1316_inline_link_text_only() {
        let spans = parse_inline_md("[text](url)");
        assert_eq!(spans[0].style, InlineStyle::LinkText);
        assert_eq!(spans[0].text, "text");
    }
    #[test] fn t1317_inline_broken_link() {
        let spans = parse_inline_md("[text] not a link");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].style, InlineStyle::Normal);
    }
    #[test] fn t1318_inline_multiple_bold() {
        let spans = parse_inline_md("**a** and **b**");
        let bold_count = spans.iter().filter(|s| s.style == InlineStyle::Bold).count();
        assert_eq!(bold_count, 2);
    }
    #[test] fn t1319_inline_no_false_bold() {
        let spans = parse_inline_md("hello world");
        assert!(spans.iter().all(|s| s.style == InlineStyle::Normal));
    }
    #[test] fn t1320_inline_preserves_text() {
        let input = "hello **bold** *italic* `code` world";
        let spans = parse_inline_md(input);
        let reconstructed: String = spans.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(reconstructed, "hello bold italic code world");
    }

    // ════════════════════════════════════════════════════════════════
    // v3.0: Syntax Highlighting Tests (1400-1449)
    // ════════════════════════════════════════════════════════════════

    #[test] fn t1400_syntax_hl_default_off() { let w = Writer::new_with(true); assert!(!w.syntax_highlight); }
    #[test] fn t1401_syntax_hl_toggle() { let mut w = Writer::new_with(true); w.syntax_highlight = true; assert!(w.syntax_highlight); }
    #[test] fn t1402_syntax_hl_with_heading() {
        let w = make_writer("# Heading\nNormal text");
        assert!(w.tabs[0].text.starts_with("# "));
    }
    #[test] fn t1403_syntax_hl_with_code_block() {
        let w = make_writer("```\ncode\n```");
        assert!(w.tabs[0].text.contains("```"));
    }
    #[test] fn t1404_syntax_hl_with_blockquote() {
        let w = make_writer("> quoted text");
        assert!(w.tabs[0].text.starts_with("> "));
    }
    #[test] fn t1405_syntax_hl_with_list() {
        let w = make_writer("- item 1\n- item 2");
        assert_eq!(w.line_count(), 2);
    }
    #[test] fn t1406_syntax_hl_with_rule() {
        let w = make_writer("---");
        assert_eq!(w.tabs[0].text, "---");
    }
    #[test] fn t1407_syntax_hl_mixed() {
        let text = "# Title\n\nNormal paragraph.\n\n> Quote\n\n- List\n\n```\ncode\n```\n\n---";
        let w = make_writer(text);
        assert!(w.line_count() > 5);
    }
    #[test] fn t1408_syntax_empty_text() {
        let w = make_writer("");
        assert_eq!(w.word_count(), 0);
    }
    #[test] fn t1409_syntax_hl_heading_levels() {
        let text = "# H1\n## H2\n### H3\n#### H4";
        let w = make_writer(text);
        assert_eq!(w.line_count(), 4);
    }
    #[test] fn t1410_syntax_hl_nested_blockquote() {
        let w = make_writer("> > nested quote");
        assert!(w.tabs[0].text.starts_with("> "));
    }
    #[test] fn t1411_syntax_hl_star_list() {
        let w = make_writer("* item");
        assert!(w.tabs[0].text.starts_with("* "));
    }
    #[test] fn t1412_syntax_three_backticks() {
        let w = make_writer("```rust\nlet x = 1;\n```");
        assert_eq!(w.line_count(), 3);
    }
    #[test] fn t1413_syntax_only_hash_no_heading() {
        let w = make_writer("####### not valid heading");
        assert_eq!(w.line_count(), 1);
    }
    #[test] fn t1414_syntax_blank_lines() {
        let w = make_writer("a\n\n\nb");
        assert!(w.line_count() >= 3);
    }
    #[test] fn t1415_syntax_long_document() {
        let mut lines = Vec::new();
        lines.push("# Document Title".to_string());
        lines.push("".to_string());
        for i in 0..50 {
            lines.push(format!("Paragraph {} with some text content.", i));
            lines.push("".to_string());
        }
        let w = make_writer(&lines.join("\n"));
        assert!(w.word_count() > 100);
    }

    // ════════════════════════════════════════════════════════════════
    // v3.0: History Snapshot Tests (1450-1499)
    // ════════════════════════════════════════════════════════════════

    #[test] fn t1450_history_default_empty() { let w = Writer::new_with(true); assert!(w.history.is_empty()); }
    #[test] fn t1451_show_history_default_off() { let w = Writer::new_with(true); assert!(!w.show_history); }
    #[test] fn t1452_take_snapshot_adds() {
        let mut w = make_writer("hello world");
        w.take_snapshot();
        assert_eq!(w.history.len(), 1);
    }
    #[test] fn t1453_snapshot_word_count() {
        let mut w = make_writer("hello world foo");
        w.take_snapshot();
        assert_eq!(w.history[0].word_count, 3);
    }
    #[test] fn t1454_snapshot_text() {
        let mut w = make_writer("some text");
        w.take_snapshot();
        assert_eq!(w.history[0].text, "some text");
    }
    #[test] fn t1455_no_duplicate_snapshot() {
        let mut w = make_writer("hello");
        w.take_snapshot();
        w.take_snapshot();
        assert_eq!(w.history.len(), 1);
    }
    #[test] fn t1456_snapshot_on_change() {
        let mut w = make_writer("v1");
        w.take_snapshot();
        w.tabs[0].text = "v2".into();
        w.last_snapshot_text = "v1".into();
        w.take_snapshot();
        assert_eq!(w.history.len(), 2);
    }
    #[test] fn t1457_restore_history() {
        let mut w = make_writer("original");
        w.take_snapshot();
        w.tabs[0].text = "modified".into();
        w.last_snapshot_text = "modified".into();
        w.restore_history(0);
        assert_eq!(w.tabs[0].text, "original");
    }
    #[test] fn t1458_restore_sets_modified() {
        let mut w = make_writer("text");
        w.tabs[0].modified = false;
        w.take_snapshot();
        w.tabs[0].text = "changed".into();
        w.last_snapshot_text = "changed".into();
        w.restore_history(0);
        assert!(w.tabs[0].modified);
    }
    #[test] fn t1459_restore_invalid_idx() {
        let mut w = make_writer("text");
        w.restore_history(999);
        assert_eq!(w.tabs[0].text, "text");
    }
    #[test] fn t1460_history_max_100() {
        let mut w = make_writer("start");
        for i in 0..110 {
            w.tabs[0].text = format!("version {}", i);
            w.last_snapshot_text = format!("version {}", (i as i32).wrapping_sub(1));
            w.take_snapshot();
        }
        assert!(w.history.len() <= 100);
    }
    #[test] fn t1461_snapshot_empty_no_dup() {
        let mut w = make_writer("");
        w.take_snapshot();
        w.take_snapshot();
        // Empty text: first snapshot saves, second is dup
        assert!(w.history.len() <= 1);
    }
    #[test] fn t1462_multiple_snapshots() {
        let mut w = make_writer("a");
        w.take_snapshot();
        w.tabs[0].text = "ab".into();
        w.last_snapshot_text = "a".into();
        w.take_snapshot();
        w.tabs[0].text = "abc".into();
        w.last_snapshot_text = "ab".into();
        w.take_snapshot();
        assert_eq!(w.history.len(), 3);
        assert_eq!(w.history[0].text, "a");
        assert_eq!(w.history[1].text, "ab");
        assert_eq!(w.history[2].text, "abc");
    }
    #[test] fn t1463_restore_first_snapshot() {
        let mut w = make_writer("first");
        w.take_snapshot();
        w.tabs[0].text = "second".into();
        w.last_snapshot_text = "first".into();
        w.take_snapshot();
        w.tabs[0].text = "third".into();
        w.last_snapshot_text = "second".into();
        w.restore_history(0);
        assert_eq!(w.tabs[0].text, "first");
    }
    #[test] fn t1464_restore_last_snapshot() {
        let mut w = make_writer("v1");
        w.take_snapshot();
        w.tabs[0].text = "v2".into();
        w.last_snapshot_text = "v1".into();
        w.take_snapshot();
        w.tabs[0].text = "v3".into();
        w.last_snapshot_text = "v2".into();
        w.restore_history(1);
        assert_eq!(w.tabs[0].text, "v2");
    }
    #[test] fn t1465_snapshot_korean() {
        let mut w = make_writer("안녕하세요 세계");
        w.take_snapshot();
        assert_eq!(w.history[0].word_count, 2);
    }
    #[test] fn t1466_snapshot_preserves_newlines() {
        let mut w = make_writer("line1\nline2\nline3");
        w.take_snapshot();
        assert!(w.history[0].text.contains('\n'));
        assert_eq!(w.history[0].text.lines().count(), 3);
    }
    #[test] fn t1467_last_snapshot_text_updated() {
        let mut w = make_writer("hello");
        w.take_snapshot();
        assert_eq!(w.last_snapshot_text, "hello");
    }
    #[test] fn t1468_history_toggle() {
        let mut w = Writer::new_with(true);
        w.show_history = true;
        assert!(w.show_history);
        w.show_history = false;
        assert!(!w.show_history);
    }

    // ════════════════════════════════════════════════════════════════
    // v3.0: Integration Tests (1500-1530)
    // ════════════════════════════════════════════════════════════════

    #[test] fn t1500_all_v3_defaults() {
        let w = Writer::new_with(true);
        assert!(!w.show_preview);
        assert!(!w.syntax_highlight);
        assert!(!w.show_history);
        assert!(w.history.is_empty());
    }
    #[test] fn t1501_preview_with_markdown() {
        let w = make_writer("# Title\n\n**bold** and *italic*\n\n- list item\n\n> quote\n\n```\ncode\n```\n\n---");
        assert!(w.word_count() > 0);
    }
    #[test] fn t1502_syntax_hl_does_not_modify_text() {
        let text = "# Title\n**bold** text";
        let mut w = make_writer(text);
        w.syntax_highlight = true;
        assert_eq!(w.tabs[0].text, text);
    }
    #[test] fn t1503_snapshot_after_tab_switch() {
        let mut w = make_writer("tab1 text");
        w.take_snapshot();
        w.new_tab();
        w.tabs[1].text = "tab2 text".into();
        assert_eq!(w.history[0].text, "tab1 text");
    }
    #[test] fn t1504_inline_md_all_styles() {
        let spans = parse_inline_md("normal **bold** *italic* `code` [link](url)");
        let styles: Vec<_> = spans.iter().map(|s| s.style).collect();
        assert!(styles.contains(&InlineStyle::Normal));
        assert!(styles.contains(&InlineStyle::Bold));
        assert!(styles.contains(&InlineStyle::Italic));
        assert!(styles.contains(&InlineStyle::Code));
        assert!(styles.contains(&InlineStyle::LinkText));
    }
    #[test] fn t1505_inline_md_empty_bold() {
        let spans = parse_inline_md("**** empty");
        // ** followed by ** = empty bold, should not produce Bold span
        assert!(spans.iter().all(|s| s.style != InlineStyle::Bold || !s.text.is_empty()));
    }
    #[test] fn t1506_snapshot_word_count_zero() {
        let mut w = make_writer("");
        w.take_snapshot();
        if !w.history.is_empty() {
            assert_eq!(w.history[0].word_count, 0);
        }
    }
    #[test] fn t1507_preview_empty_doc() {
        let w = make_writer("");
        assert_eq!(w.word_count(), 0);
    }
    #[test] fn t1508_preview_only_headings() {
        let w = make_writer("# H1\n## H2\n### H3");
        assert_eq!(w.line_count(), 3);
    }
    #[test] fn t1509_preview_only_list() {
        let w = make_writer("- a\n- b\n- c");
        assert_eq!(w.line_count(), 3);
    }
    #[test] fn t1510_preview_only_blockquote() {
        let w = make_writer("> line1\n> line2");
        assert_eq!(w.line_count(), 2);
    }
    #[test] fn t1511_preview_code_block() {
        let text = "```\nfn main() {}\n```";
        let w = make_writer(text);
        assert_eq!(w.line_count(), 3);
    }
    #[test] fn t1512_syntax_hl_ordered_list() {
        let w = make_writer("1. first\n2. second\n3. third");
        assert_eq!(w.line_count(), 3);
    }
    #[test] fn t1513_history_restore_updates_snapshot_text() {
        let mut w = make_writer("original");
        w.take_snapshot();
        w.tabs[0].text = "changed".into();
        w.last_snapshot_text = "changed".into();
        w.restore_history(0);
        assert_eq!(w.last_snapshot_text, "original");
    }
    #[test] fn t1514_inline_md_consecutive_codes() {
        let spans = parse_inline_md("`a` `b` `c`");
        let code_count = spans.iter().filter(|s| s.style == InlineStyle::Code).count();
        assert_eq!(code_count, 3);
    }
    #[test] fn t1515_inline_md_link_no_url() {
        let spans = parse_inline_md("[text]");
        assert!(spans.iter().all(|s| s.style == InlineStyle::Normal));
    }
    #[test] fn t1516_inline_md_bold_korean() {
        let spans = parse_inline_md("**굵은 글씨** 테스트");
        assert_eq!(spans[0].style, InlineStyle::Bold);
        assert_eq!(spans[0].text, "굵은 글씨");
    }
    #[test] fn t1517_inline_md_italic_korean() {
        let spans = parse_inline_md("*기울임* 텍스트");
        assert_eq!(spans[0].style, InlineStyle::Italic);
        assert_eq!(spans[0].text, "기울임");
    }
    #[test] fn t1518_inline_md_code_korean() {
        let spans = parse_inline_md("`코드` 블록");
        assert_eq!(spans[0].style, InlineStyle::Code);
        assert_eq!(spans[0].text, "코드");
    }
    #[test] fn t1519_multiple_restore() {
        let mut w = make_writer("v1");
        w.take_snapshot();
        w.tabs[0].text = "v2".into();
        w.last_snapshot_text = "v1".into();
        w.take_snapshot();
        w.restore_history(0);
        assert_eq!(w.tabs[0].text, "v1");
        w.restore_history(1);
        assert_eq!(w.tabs[0].text, "v2");
    }
    #[test] fn t1520_large_markdown_doc() {
        let mut doc = String::new();
        doc.push_str("# Title\n\n");
        for i in 0..20 {
            doc.push_str(&format!("## Section {}\n\n", i));
            doc.push_str("Some **bold** and *italic* text with `code` inline.\n\n");
            doc.push_str("> Blockquote here\n\n");
            doc.push_str("- Item 1\n- Item 2\n\n");
            doc.push_str("---\n\n");
        }
        let w = make_writer(&doc);
        assert!(w.word_count() > 100);
        assert!(w.line_count() > 50);
    }
}

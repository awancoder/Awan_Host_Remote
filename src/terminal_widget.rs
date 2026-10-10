//! A terminal emulator component for gpui-kit, fed by bytes from an SSH
//! channel rather than a local PTY. Design ported from `gpui-terminal`
//! (github.com/zortax/gpui-terminal, MIT OR Apache-2.0): that crate's
//! `TerminalState` (this file's [`TerminalState`]) and `keystroke_to_bytes`
//! already have no PTY dependency at all — they are alacritty_terminal plus
//! plain data. Its `TerminalView`/`render.rs` do assume a spawned local
//! process and use the real `gpui` crate (gpui-kit pins its own `gpui-pre`
//! snapshot, a different package identity), so the widget below is written
//! fresh against `gpui_kit::*` following that crate's structure, not
//! depended on directly.
use std::sync::Arc;

use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line, Point as AlacPoint, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Config, Term, TermMode};
use alacritty_terminal::vte::ansi::{self, Color as AnsiColor, NamedColor};
use alacritty_terminal::event::{Event as AlacrittyEvent, EventListener};
use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::ActiveTheme;
use gpui_kit::component::menu::{ContextMenuExt, PopupMenu, PopupMenuItem};
use gpui_kit::*;
use parking_lot::Mutex;
use tokio::sync::{mpsc, watch};

/// Feeds `Event::PtyWrite` (the terminal's own replies to things like cursor
/// position queries) straight to the SSH output channel. Everything else
/// alacritty can emit (title changes, bell, clipboard, exit) is not wired up
/// yet — a known gap, not a silent fake.
#[derive(Clone)]
struct OutputProxy(mpsc::Sender<Vec<u8>>);

impl EventListener for OutputProxy {
    fn send_event(&self, event: AlacrittyEvent) {
        if let AlacrittyEvent::PtyWrite(data) = event {
            let _ = self.0.try_send(data.into_bytes());
        }
    }
}

struct TermSize {
    cols: usize,
    rows: usize,
}

impl Dimensions for TermSize {
    fn total_lines(&self) -> usize { self.rows }
    fn screen_lines(&self) -> usize { self.rows }
    fn columns(&self) -> usize { self.cols }
    fn last_column(&self) -> Column { Column(self.cols.saturating_sub(1)) }
    fn bottommost_line(&self) -> Line { Line(self.rows as i32 - 1) }
}

/// Thread-safe wrapper around alacritty's `Term`, driven purely by
/// `process_bytes` — there is no PTY read loop here at all, unlike a normal
/// terminal emulator. Ported near-verbatim from `gpui-terminal`'s
/// `terminal.rs`; that file barely touches GPUI types.
struct TerminalState {
    term: Arc<Mutex<Term<OutputProxy>>>,
    parser: ansi::Processor,
}

impl TerminalState {
    fn new(cols: usize, rows: usize, event_proxy: OutputProxy) -> Self {
        let term = Term::new(Config::default(), &TermSize { cols, rows }, event_proxy);
        Self { term: Arc::new(Mutex::new(term)), parser: ansi::Processor::new() }
    }

    fn process_bytes(&mut self, bytes: &[u8]) {
        let mut term = self.term.lock();
        self.parser.advance(&mut *term, bytes);
    }

    // Resizing happens in-place on the shared `Arc<Mutex<Term>>` from the
    // `canvas` paint closure in `Render for TerminalWidget` (it only
    // captures that Arc, not `&mut TerminalState`), so there is no wrapper
    // method here — see `paint_grid`'s caller.

    fn mode(&self) -> TermMode { *self.term.lock().mode() }
}

/// Maps a GPUI keystroke to the byte sequence a terminal application
/// expects, including application-cursor-mode arrow keys. Ported
/// near-verbatim from `gpui-terminal`'s `input.rs::keystroke_to_bytes` —
/// that function is plain data (`gpui::Keystroke`, `TermMode`), no PTY or
/// window coupling.
fn keystroke_to_bytes(keystroke: &Keystroke, mode: TermMode) -> Option<Vec<u8>> {
    match keystroke.key.as_str() {
        "space" => {
            return Some(if keystroke.modifiers.control { b"\x00".to_vec() } else { b" ".to_vec() });
        }
        "enter" => return Some(b"\r".to_vec()),
        "escape" => return Some(b"\x1b".to_vec()),
        "backspace" => return Some(b"\x7f".to_vec()),
        "tab" => return Some(if keystroke.modifiers.shift { b"\x1b[Z".to_vec() } else { b"\t".to_vec() }),
        "up" => return Some(if mode.contains(TermMode::APP_CURSOR) { b"\x1bOA".to_vec() } else { b"\x1b[A".to_vec() }),
        "down" => return Some(if mode.contains(TermMode::APP_CURSOR) { b"\x1bOB".to_vec() } else { b"\x1b[B".to_vec() }),
        "right" => return Some(if mode.contains(TermMode::APP_CURSOR) { b"\x1bOC".to_vec() } else { b"\x1b[C".to_vec() }),
        "left" => return Some(if mode.contains(TermMode::APP_CURSOR) { b"\x1bOD".to_vec() } else { b"\x1b[D".to_vec() }),
        "home" => return Some(b"\x1b[H".to_vec()),
        "end" => return Some(b"\x1b[F".to_vec()),
        "pageup" => return Some(b"\x1b[5~".to_vec()),
        "pagedown" => return Some(b"\x1b[6~".to_vec()),
        "insert" => return Some(b"\x1b[2~".to_vec()),
        "delete" => return Some(b"\x1b[3~".to_vec()),
        "f1" => return Some(b"\x1bOP".to_vec()),
        "f2" => return Some(b"\x1bOQ".to_vec()),
        "f3" => return Some(b"\x1bOR".to_vec()),
        "f4" => return Some(b"\x1bOS".to_vec()),
        "f5" => return Some(b"\x1b[15~".to_vec()),
        "f6" => return Some(b"\x1b[17~".to_vec()),
        "f7" => return Some(b"\x1b[18~".to_vec()),
        "f8" => return Some(b"\x1b[19~".to_vec()),
        "f9" => return Some(b"\x1b[20~".to_vec()),
        "f10" => return Some(b"\x1b[21~".to_vec()),
        "f11" => return Some(b"\x1b[23~".to_vec()),
        "f12" => return Some(b"\x1b[24~".to_vec()),
        _ => {}
    }

    if keystroke.modifiers.control {
        let key = keystroke.key.as_str();
        if key.len() == 1 {
            let ch = key.chars().next().unwrap();
            if ch.is_ascii_alphabetic() {
                return Some(vec![(ch.to_ascii_uppercase() as u8) - b'@']);
            }
            match ch {
                '[' => return Some(b"\x1b".to_vec()),
                '\\' => return Some(b"\x1c".to_vec()),
                ']' => return Some(b"\x1d".to_vec()),
                '^' => return Some(b"\x1e".to_vec()),
                '_' => return Some(b"\x1f".to_vec()),
                '?' => return Some(b"\x7f".to_vec()),
                _ => {}
            }
        }
    }

    if keystroke.modifiers.alt {
        let key = keystroke.key.as_str();
        if key.len() == 1 {
            let ch = key.chars().next().unwrap();
            if ch.is_ascii() { return Some(vec![b'\x1b', ch as u8]); }
        }
    }

    if let Some(key_char) = &keystroke.key_char {
        if !keystroke.modifiers.control && !keystroke.modifiers.alt {
            return Some(key_char.as_bytes().to_vec());
        }
    }

    let key = keystroke.key.as_str();
    if key.len() == 1 {
        let ch = key.chars().next().unwrap();
        if ch.is_ascii() && !keystroke.modifiers.control {
            let ch = if keystroke.modifiers.shift { ch.to_ascii_uppercase() } else { ch };
            return Some(vec![ch as u8]);
        }
        if !keystroke.modifiers.control && !keystroke.modifiers.alt {
            return Some(key.as_bytes().to_vec());
        }
    }
    None
}

/// 16-color ANSI palette, same RGB values used by the previous egui
/// terminal (`terminal.rs`'s `ansi_color`, now retired) so colors look the
/// same as before.
const ANSI_16: [(u8, u8, u8); 16] = [
    (20, 25, 34), (224, 108, 117), (152, 195, 121), (229, 192, 123),
    (97, 175, 239), (198, 120, 221), (86, 182, 194), (220, 223, 228),
    (95, 107, 126), (255, 131, 137), (180, 224, 147), (255, 216, 147),
    (132, 195, 255), (223, 152, 255), (116, 214, 223), (255, 255, 255),
];

fn resolve_color(color: AnsiColor, default: Hsla) -> Hsla {
    match color {
        AnsiColor::Named(NamedColor::Foreground) | AnsiColor::Named(NamedColor::Background) => default,
        AnsiColor::Named(named) => {
            let index = named as usize;
            if index < 16 { let (r, g, b) = ANSI_16[index]; gpui_kit::rgb(((r as u32) << 16) | ((g as u32) << 8) | b as u32).into() }
            else { default }
        }
        AnsiColor::Spec(rgb) => gpui_kit::rgb(((rgb.r as u32) << 16) | ((rgb.g as u32) << 8) | rgb.b as u32).into(),
        AnsiColor::Indexed(index) => {
            if index < 16 {
                let (r, g, b) = ANSI_16[index as usize];
                gpui_kit::rgb(((r as u32) << 16) | ((g as u32) << 8) | b as u32).into()
            } else if index < 232 {
                let n = index - 16;
                let level = |component: u8| if component == 0 { 0u32 } else { 55 + component as u32 * 40 };
                let r = level(n / 36);
                let g = level((n % 36) / 6);
                let b = level(n % 6);
                gpui_kit::rgb((r << 16) | (g << 8) | b).into()
            } else {
                let level = (8 + (index - 232) * 10) as u32;
                gpui_kit::rgb((level << 16) | (level << 8) | level).into()
            }
        }
    }
}

/// The terminal widget: an `Entity<TerminalWidget>` rendered with
/// `TerminalWidget::view(&entity)`. Bytes read from SSH are fed in through
/// [`TerminalWidget::feed`]; bytes the terminal itself produces (keystrokes,
/// query replies) go out through `output_rx` (hand the sender's matching
/// `output` field to whatever forwards them over SSH — see `terminal.rs`).
pub struct TerminalWidget {
    state: TerminalState,
    focus_handle: FocusHandle,
    output: mpsc::Sender<Vec<u8>>,
    resize: watch::Sender<(u16, u16)>,
    font_size: Pixels,
    cell_size: Size<Pixels>,
    /// The canvas's bounds and just-measured cell size, written at the top
    /// of every paint. Mouse handlers run outside the paint closure (which
    /// only hands its geometry to the renderer, not back to `self`), so this
    /// is how they learn where the grid actually is on screen to convert a
    /// pixel position into a row/column.
    layout: Arc<Mutex<(Bounds<Pixels>, Size<Pixels>)>>,
    /// Whether the left mouse button is down and dragging out a selection —
    /// GPUI calls `on_mouse_move` for every pointer move, not just drags
    /// started on this element, so this is what tells the handler whether
    /// to extend the selection or ignore the move.
    selecting: bool,
    /// Right-click menu labels `[copy, paste]`; the widget has no i18n of its
    /// own, so the app sets them in its current language.
    menu_labels: [&'static str; 2],
}

impl TerminalWidget {
    pub fn new(
        cols: usize,
        rows: usize,
        output: mpsc::Sender<Vec<u8>>,
        resize: watch::Sender<(u16, u16)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let _ = window;
        let state = TerminalState::new(cols, rows, OutputProxy(output.clone()));
        let font_size = px(14.0);
        let cell_size = size(px(8.0), px(18.0));
        Self {
            state, focus_handle: cx.focus_handle(), output, resize, font_size, cell_size,
            layout: Arc::new(Mutex::new((Bounds::default(), cell_size))),
            selecting: false,
            menu_labels: ["Copy", "Paste"],
        }
    }

    pub fn set_menu_labels(&mut self, copy: &'static str, paste: &'static str) {
        self.menu_labels = [copy, paste];
    }

    fn has_selection(&self) -> bool {
        self.state.term.lock().selection_to_string().is_some_and(|text| !text.is_empty())
    }

    /// Copies the selected text to the clipboard and clears the selection.
    /// Returns `false` when nothing is selected.
    fn copy_selection(&mut self, cx: &mut Context<Self>) -> bool {
        let mut term = self.state.term.lock();
        let Some(text) = term.selection_to_string().filter(|text| !text.is_empty()) else { return false; };
        term.selection = None;
        drop(term);
        cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(text));
        cx.notify();
        true
    }

    /// Sends the clipboard text to the shell, wrapped in bracketed-paste
    /// markers when the running program asked for them.
    fn paste_clipboard(&mut self, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else { return; };
        let bracketed = self.state.mode().contains(TermMode::BRACKETED_PASTE);
        let mut bytes = Vec::with_capacity(text.len() + 12);
        if bracketed { bytes.extend_from_slice(b"\x1b[200~"); }
        bytes.extend_from_slice(text.as_bytes());
        if bracketed { bytes.extend_from_slice(b"\x1b[201~"); }
        let _ = self.output.try_send(bytes);
    }

    pub fn focus_handle(&self) -> &FocusHandle { &self.focus_handle }

    /// Whether a full-screen program (vim, htop, less, ...) currently owns
    /// the screen, i.e. typed text would go to it rather than to the shell.
    pub fn in_alternate_screen(&self) -> bool { self.state.mode().contains(TermMode::ALT_SCREEN) }

    /// Sends bytes to the remote shell exactly as if they were typed.
    /// Returns `false` if the input queue is full or the session is gone.
    pub fn send_input(&self, bytes: Vec<u8>) -> bool { self.output.try_send(bytes).is_ok() }

    /// Bytes read from the SSH channel (`ssh::Event::Output`).
    pub fn feed(&mut self, bytes: &[u8], cx: &mut Context<Self>) {
        self.state.process_bytes(bytes);
        cx.notify();
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let ctrl = event.keystroke.modifiers.control;
        let key = event.keystroke.key.as_str();
        // Typing while scrolled back into history jumps back to the live
        // view, same as most terminals — otherwise the new input/output
        // would land off-screen above where the user is looking.
        {
            let mut term = self.state.term.lock();
            if term.grid().display_offset() != 0 {
                term.scroll_display(Scroll::Bottom);
            }
        }
        // Ctrl+C copies when there's an actual selection (clearing it
        // afterward, same as most terminal apps), and only falls through to
        // sending SIGINT when there is none — Ctrl+V always pastes rather
        // than being sent through as a literal control byte.
        if ctrl && key == "c" {
            if self.copy_selection(cx) { return; }
        } else if ctrl && key == "v" {
            self.paste_clipboard(cx);
            return;
        }
        if let Some(bytes) = keystroke_to_bytes(&event.keystroke, self.state.mode()) {
            let _ = self.output.try_send(bytes);
        }
    }

    fn on_mouse_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus_handle, cx);
        let (bounds, cell) = *self.layout.lock();
        let mut term = self.state.term.lock();
        let (cols, rows) = (term.columns(), term.screen_lines());
        let display_offset = term.grid().display_offset() as i32;
        let (point, side) = grid_point(bounds, cell, cols, rows, display_offset, event.position);
        term.selection = Some(Selection::new(SelectionType::Simple, point, side));
        drop(term);
        self.selecting = true;
        cx.notify();
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.selecting { return; }
        let (bounds, cell) = *self.layout.lock();
        let mut term = self.state.term.lock();
        let (cols, rows) = (term.columns(), term.screen_lines());
        let display_offset = term.grid().display_offset() as i32;
        let (point, side) = grid_point(bounds, cell, cols, rows, display_offset, event.position);
        if let Some(selection) = &mut term.selection { selection.update(point, side); }
        drop(term);
        cx.notify();
    }

    fn on_mouse_up(&mut self, _event: &MouseUpEvent, _window: &mut Window, _cx: &mut Context<Self>) {
        self.selecting = false;
    }

    /// Scrolls into alacritty's own scrollback buffer (up to the 10,000
    /// lines `Config::default().scrolling_history` keeps) rather than any
    /// GPUI-level scroll container — the canvas always fills its parent, so
    /// there is nothing for a normal `.overflow_y_scroll()` to act on.
    fn on_scroll_wheel(&mut self, event: &ScrollWheelEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let line_height = self.layout.lock().1.height;
        if line_height <= px(0.0) { return; }
        let lines = (event.delta.pixel_delta(line_height).y / line_height).round() as i32;
        if lines == 0 { return; }
        let mut term = self.state.term.lock();
        term.scroll_display(Scroll::Delta(lines));
        drop(term);
        cx.notify();
    }
}

/// Builds the monospace `Font` to shape terminal text with, from the theme's
/// own installed-font detection (`cx.theme().mono_font_family`, tried in
/// order against "Cascadia Mono" / "Consolas" / "Courier New" — see
/// `gpui_kit::component`'s `mono_font` module) rather than the CSS-style
/// generic name `"monospace"`, which GPUI's font matcher does not resolve to
/// an actual installed monospace font and was the cause of the loose,
/// mismatched-width look.
fn mono_font(cx: &App, bold: bool) -> Font {
    Font {
        family: cx.theme().mono_font_family.clone(),
        features: FontFeatures::default(),
        fallbacks: Some(FontFallbacks::from_fonts(vec!["Consolas".into(), "Courier New".into()])),
        weight: if bold { FontWeight::BOLD } else { FontWeight::NORMAL },
        style: FontStyle::Normal,
    }
}

impl Render for TerminalWidget {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let output = self.output.clone();
        let resize = self.resize.clone();
        let term = self.state.term.clone();
        let cell_size = self.cell_size;
        let font_size = self.font_size;
        let layout = self.layout.clone();
        let entity = cx.entity();
        let has_selection = self.has_selection();
        let [copy_label, paste_label] = self.menu_labels;

        div()
            .size_full()
            .bg(gpui_kit::black())
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_scroll_wheel(cx.listener(Self::on_scroll_wheel))
            .child(
                canvas(
                    move |_bounds, window, cx| {
                        let mut measured = cell_size;
                        let font = mono_font(cx, false);
                        let run = TextRun { len: "M".len(), font, color: gpui_kit::black(), background_color: None, underline: None, strikethrough: None };
                        let shaped = window.text_system().shape_line("M".into(), font_size, &[run], None);
                        if shaped.width > px(0.0) { measured.width = shaped.width; }
                        // A tight multiplier (not the ~1.3 of normal prose line spacing)
                        // is what makes rows read as a terminal grid, not loose text.
                        let line_height = (shaped.ascent + shaped.descent).ceil();
                        if line_height > px(0.0) { measured.height = line_height * 1.15; }
                        measured
                    },
                    move |bounds, measured_cell: Size<Pixels>, window, cx| {
                        let cols = ((bounds.size.width / measured_cell.width) as usize).max(1);
                        let rows = ((bounds.size.height / measured_cell.height) as usize).max(1);
                        *layout.lock() = (bounds, measured_cell);
                        let mut term = term.lock();
                        if cols != term.columns() || rows != term.screen_lines() {
                            let _ = resize.send((rows as u16, cols as u16));
                            term.resize(TermSize { cols, rows });
                        }
                        paint_grid(&term, bounds, measured_cell, font_size, window, cx);
                        let _ = &output;
                    },
                )
                .size_full(),
            )
            // Right-click keeps the current selection (only the left button
            // starts a new one), so Copy acts on what was just selected.
            .context_menu(move |menu: PopupMenu, _, _| {
                menu.item(PopupMenuItem::new(copy_label).icon(Lucide::Copy).disabled(!has_selection).on_click({
                        let entity = entity.clone();
                        move |_, _, cx| entity.update(cx, |this, cx| { this.copy_selection(cx); })
                    }))
                    .item(PopupMenuItem::new(paste_label).icon(Lucide::ClipboardPaste).on_click({
                        let entity = entity.clone();
                        move |_, window, cx| entity.update(cx, |this, cx| {
                            window.focus(&this.focus_handle, cx);
                            this.paste_clipboard(cx);
                        })
                    }))
            })
    }
}

/// Converts a window-space pixel position into a grid cell + which half of
/// it ([`Side`]) the point fell in — `alacritty_terminal`'s selection
/// tracks both, so a drag anchored at the right edge of a cell does not
/// include that cell the way one anchored at its left edge would.
///
/// `display_offset` shifts the result into scrollback the same way
/// `paint_grid` shifts what it reads from the grid — without it, selecting
/// text while scrolled back would highlight the live rows underneath
/// instead of whatever history is actually on screen.
fn grid_point(bounds: Bounds<Pixels>, cell: Size<Pixels>, cols: usize, rows: usize, display_offset: i32, position: Point<Pixels>) -> (AlacPoint, Side) {
    let rel_x = (position.x - bounds.origin.x).max(px(0.));
    let rel_y = (position.y - bounds.origin.y).max(px(0.));
    let col = ((rel_x / cell.width) as usize).min(cols.saturating_sub(1));
    let row = ((rel_y / cell.height) as i32).min(rows.saturating_sub(1) as i32).max(0);
    let within = rel_x - cell.width * col as f32;
    let side = if within < cell.width / 2. { Side::Left } else { Side::Right };
    (AlacPoint::new(Line(row - display_offset), Column(col)), side)
}

fn paint_grid(term: &Term<OutputProxy>, bounds: Bounds<Pixels>, cell: Size<Pixels>, font_size: Pixels, window: &mut Window, cx: &mut App) {
    let grid = term.grid();
    let default_bg: Hsla = gpui_kit::black();
    window.paint_quad(quad(bounds, px(0.0), default_bg, Edges::default(), transparent_black(), Default::default()));

    // `to_range` only exists on a selection, not the other way around, so
    // this has to be read back out through the term it was built against.
    let selection = term.selection.as_ref().and_then(|selection| selection.to_range(term));

    let regular_font = mono_font(cx, false);
    let bold_font = mono_font(cx, true);
    let rows = grid.screen_lines();
    let cols = grid.columns();
    // Scrolled back into history, the viewport's top row maps to a negative
    // `Line` further back in the grid — without this shift, every row read
    // here would stay pinned to the live screen regardless of scroll
    // position (the bug: the scrollbar thumb moved but the text never did).
    let display_offset = grid.display_offset() as i32;
    for row in 0..rows {
        for col in 0..cols {
            let point = AlacPoint::new(Line(row as i32 - display_offset), Column(col));
            let cell_data = &grid[point];
            let x = bounds.origin.x + cell.width * (col as f32);
            let y = bounds.origin.y + cell.height * (row as f32);
            if selection.is_some_and(|range| range.contains(point)) {
                let highlight = Bounds { origin: Point { x, y }, size: cell };
                window.paint_quad(quad(highlight, px(0.0), gpui_kit::white().opacity(0.25), Edges::default(), transparent_black(), Default::default()));
            }
            if cell_data.c == ' ' || cell_data.c == '\0' { continue; }
            let fg = resolve_color(cell_data.fg, gpui_kit::white());
            let bold = cell_data.flags.contains(Flags::BOLD);
            let text = cell_data.c.to_string();
            let font = if bold { bold_font.clone() } else { regular_font.clone() };
            let run = TextRun { len: text.len(), font, color: fg, background_color: None, underline: None, strikethrough: None };
            let shaped = window.text_system().shape_line(text.into(), font_size, &[run], None);
            let _ = shaped.paint(Point { x, y }, cell.height, TextAlign::default(), None, window, cx);
        }
    }

    // The cursor's own `line` is always in live-screen coordinates (it can't
    // exist in history), so it needs the opposite shift from the content
    // loop above to land in the right on-screen row — and is hidden
    // entirely once that row has scrolled out of the viewport.
    let cursor = grid.cursor.point;
    let cursor_row = cursor.line.0 + display_offset;
    if cursor_row >= 0 && (cursor_row as usize) < rows && cursor.column.0 < cols {
        let x = bounds.origin.x + cell.width * (cursor.column.0 as f32);
        let y = bounds.origin.y + cell.height * (cursor_row as f32);
        let cursor_bounds = Bounds { origin: Point { x, y }, size: cell };
        window.paint_quad(quad(cursor_bounds, px(0.0), transparent_black(), Edges::all(px(1.0)), gpui_kit::white(), Default::default()));
    }

    // Visual scrollback indicator — the grid always fills the canvas, so
    // there is no normal layout overflow for a GPUI scrollbar to attach to;
    // this is drawn by hand from alacritty's own `history_size`/
    // `display_offset` (0 = live/bottom, up to `history_size` at the very
    // top of the 10,000-line scrollback `Config::default()` keeps).
    let history = grid.history_size();
    if history > 0 {
        let total_lines = (history + rows) as f32;
        let bar_width = px(5.0);
        let track = Bounds {
            origin: Point { x: bounds.origin.x + bounds.size.width - bar_width, y: bounds.origin.y },
            size: size(bar_width, bounds.size.height),
        };
        window.paint_quad(quad(track, px(0.0), gpui_kit::white().opacity(0.06), Edges::default(), transparent_black(), Default::default()));
        let thumb_height = (bounds.size.height * (rows as f32 / total_lines)).max(px(16.0)).min(bounds.size.height);
        let lines_above = (history - grid.display_offset()) as f32;
        let max_thumb_y = bounds.size.height - thumb_height;
        let thumb_y = (bounds.size.height * (lines_above / total_lines)).min(max_thumb_y).max(px(0.0));
        let thumb = Bounds {
            origin: Point { x: bounds.origin.x + bounds.size.width - bar_width, y: bounds.origin.y + thumb_y },
            size: size(bar_width, thumb_height),
        };
        window.paint_quad(quad(thumb, px(2.5), gpui_kit::white().opacity(0.35), Edges::default(), transparent_black(), Default::default()));
    }
}

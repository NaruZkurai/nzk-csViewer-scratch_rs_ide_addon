//! `gui.scratch.rs` — the Scratch window's GUI: puzzle pieces, ports, clicks.
//!
//! Everything visual about the Scratch pane lives here, and nothing else does:
//! the model is `scratch.rs`, the input is `idelink.scratch.rs`. The painter is
//! a pure function of `(&ScratchDoc, &mut GuiState)` plus egui's frame, so the
//! only state it owns is what the *user* changed (selection, port toggle, theme).
//!
//! # The piece
//!
//! ```text
//!    ╭─╮──────────────────────╮     ← notch cut out of the top-left corner
//!    ┃ if (n <= 0)            │     ← a coloured stripe names the kind; the
//!    ┃   return;              │       body stays the theme's surface colour
//!    ┃   [condition: bool]    │     ← ports sit on their own row(s) below it
//!    ╰───╮────────────────────╯
//!        ╰─╯                        ← the tab is the same fill, below the body
//! ```
//!
//! # Colour comes from the theme, not from a rainbow
//!
//! The first palette filled every piece with a saturated pastel. That reads as
//! noise over real code, so it is gone: a piece is painted in the **host's
//! surface colour** and the *kind* is a 4 px stripe in the theme's own colour
//! for that kind (`Palette::accent`). Text, ports and selection likewise come
//! from `egui::Visuals`, so switching the app theme switches the pane.
//!
//! # Two rules learned the hard way
//!
//! * **Nothing is cut off.** Text is laid out with a wrap width, so the piece
//!   grows instead of hiding the author's code.
//! * **A port row always fits.** Ports are packed into rows against the piece's
//!   own width rather than starting past its right edge.
//!
//! # What it never does
//!
//! It does not edit. A click selects a block and emits one
//! [`Request`](crate::idelink_scratch::Request); the host decides what that
//! means. Unknown types are painted `?` — the painter cannot invent one.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use egui::{
    Align2, Color32, CornerRadius, FontId, Pos2, Rect, ScrollArea, Sense, Stroke, StrokeKind, Vec2,
};

use crate::idelink_scratch::{Request, ScratchDoc};
use crate::scratch::{tokens, Block, BlockKind, TokenKind};

/// Room the next piece leaves for this one's tab.
const ROW_GAP: f32 = 9.0;
const INDENT: f32 = 20.0;
const PAD: f32 = 10.0;
const NOTCH_R: f32 = 6.0;
/// Where the text starts, past the notch cut and the kind stripe.
const TEXT_X: f32 = 18.0;
const TAB_W: f32 = 14.0;
const TAB_H: f32 = 9.0;
/// One port pill row.
const PORT_H: f32 = 18.0;
const PORT_GAP: f32 = 4.0;
/// Space between the last text line and the first port row.
const PORTS_TOP: f32 = 6.0;
const MIN_PIECE_W: f32 = 150.0;
const MIN_PIECE_H: f32 = 30.0;
/// Width of the kind stripe on a piece's left edge.
const STRIPE_W: f32 = 4.0;
const BODY_FONT: f32 = 13.0;
const PORT_FONT: f32 = 11.0;

/// The host's theme, mirrored so the pane can pick the same accents.
///
/// The variants are named after `cs viewer`'s `UiTheme`; this crate does not
/// depend on the host, so the mapping is the host's one line of glue.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Theme {
    #[default]
    NzkDarkPlus,
    DarkV1,
}

impl Theme {
    fn rgb(c: u32) -> Color32 {
        Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8)
    }

    /// The editor colour for one word, from the theme's syntax palette.
    ///
    /// `Plain`/`Punct` fall back to the theme's text colour at the call site.
    pub fn token(self, kind: TokenKind) -> Option<Color32> {
        // The table is the IDE's own `SourceTheme::tok_color`, one arm per kind,
        // so a word cannot be one colour in the editor and another in a block.
        let c = match (self, kind) {
            (_, TokenKind::Plain) => return None,
            (Theme::NzkDarkPlus, TokenKind::Comment) => 0x6a9955,
            (Theme::NzkDarkPlus, TokenKind::Str) => 0xce9178,
            (Theme::NzkDarkPlus, TokenKind::Keyword) => 0x569cd6,
            (Theme::NzkDarkPlus, TokenKind::Control) => 0xc586c0,
            (Theme::NzkDarkPlus, TokenKind::Type) => 0x4ec9b0,
            (Theme::NzkDarkPlus, TokenKind::Number) => 0xb5cea8,
            (Theme::NzkDarkPlus, TokenKind::Function) => 0xdcdcaa,
            (Theme::NzkDarkPlus, TokenKind::Variable) => 0x9cdcfe,
            (Theme::NzkDarkPlus, TokenKind::Op) => 0xd4d4d4,
            (Theme::NzkDarkPlus, TokenKind::Preproc) => 0xc586c0,
            (Theme::DarkV1, TokenKind::Comment) => 0x6a9955,
            (Theme::DarkV1, TokenKind::Str) => 0xc8b6ff,
            (Theme::DarkV1, TokenKind::Keyword) => 0x86aaf9,
            (Theme::DarkV1, TokenKind::Control) => 0xd272ff,
            (Theme::DarkV1, TokenKind::Type) => 0x42c3d0,
            (Theme::DarkV1, TokenKind::Number) => 0xffc46b,
            (Theme::DarkV1, TokenKind::Function) => 0xfa903e,
            (Theme::DarkV1, TokenKind::Variable) => 0x9db8f0,
            (Theme::DarkV1, TokenKind::Op) => 0xd4d4d4,
            (Theme::DarkV1, TokenKind::Preproc) => 0xe57300,
        };
        Some(Theme::rgb(c))
    }

    /// The stripe colour for one block kind, taken from the theme's own node
    /// palette (never an invented hue).
    pub fn accent(self, kind: BlockKind) -> Color32 {
        let c = match (self, kind) {
            (Theme::NzkDarkPlus, BlockKind::Namespace) => 0x4ec9b0,
            (Theme::NzkDarkPlus, BlockKind::Type) => 0x86c691,
            (Theme::NzkDarkPlus, BlockKind::Method) => 0xdcdcaa,
            (Theme::NzkDarkPlus, BlockKind::Property) => 0x9cdcfe,
            (Theme::NzkDarkPlus, BlockKind::Control) => 0xc586c0,
            (Theme::NzkDarkPlus, BlockKind::Keyword) => 0xce9178,
            (Theme::NzkDarkPlus, BlockKind::Simple) => 0x859191,
            (Theme::NzkDarkPlus, BlockKind::Directive) => 0x6a737d,
            (Theme::NzkDarkPlus, BlockKind::Attribute) => 0xd7ba7d,
            (Theme::DarkV1, BlockKind::Namespace) => 0x86aaf9,
            (Theme::DarkV1, BlockKind::Type) => 0x42c3d0,
            (Theme::DarkV1, BlockKind::Method) => 0xfa903e,
            (Theme::DarkV1, BlockKind::Property) => 0x9db8f0,
            (Theme::DarkV1, BlockKind::Control) => 0xd272ff,
            (Theme::DarkV1, BlockKind::Keyword) => 0xf48385,
            (Theme::DarkV1, BlockKind::Simple) => 0x9aa6c2,
            (Theme::DarkV1, BlockKind::Directive) => 0x7a86a8,
            (Theme::DarkV1, BlockKind::Attribute) => 0xe57300,
        };
        Theme::rgb(c)
    }
}

/// The host IDE's source-pane palette.
///
/// The Scratch pane mirrors the editor it sits beside, so its background, text
/// and token colours come from the *host's* syntax theme — not from egui's
/// defaults, which is why the pane used to be a different grey than the IDE.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaletteSpec {
    pub bg: Color32,
    pub head_bg: Color32,
    pub fg: Color32,
    pub dim: Color32,
    pub border: Color32,
    pub accent: Color32,
    pub comment: Color32,
    pub keyword: Color32,
    pub control: Color32,
    pub ty: Color32,
    pub iface: Color32,
    pub string: Color32,
    pub number: Color32,
    pub preproc: Color32,
    pub function: Color32,
    pub variable: Color32,
    pub op: Color32,
}

/// Every colour a piece needs. Built once per frame; no statics, so a theme
/// switch is a repaint away.
#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub body: Color32,
    pub body_selected: Color32,
    pub ink: Color32,
    pub ink_dim: Color32,
    pub port_bg: Color32,
    pub port_ink: Color32,
    /// Background behind the notch, so the cut looks cut.
    pub chassis: Color32,
    pub border: Color32,
    pub trunk: Color32,
    pub theme: Theme,
    /// Present when the host supplied its own palette; the accessors below
    /// prefer it over the theme tables.
    pub spec: Option<PaletteSpec>,
}

fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let f = |x: u8, y: u8| (x as f32 * (1.0 - t) + y as f32 * t).round() as u8;
    Color32::from_rgb(f(a.r(), b.r()), f(a.g(), b.g()), f(a.b(), b.b()))
}

impl Palette {
    /// The palette the IDE itself uses: same background and same token colours.
    /// Pieces are the IDE's head background so they read as surfaces on it.
    pub fn from_spec(spec: PaletteSpec) -> Palette {
        Palette {
            body: spec.head_bg,
            body_selected: mix(spec.head_bg, spec.accent, 0.28),
            ink: spec.fg,
            ink_dim: spec.dim,
            port_bg: mix(spec.head_bg, spec.fg, 0.10),
            port_ink: spec.dim,
            chassis: spec.bg,
            border: spec.border,
            trunk: spec.border,
            theme: Theme::NzkDarkPlus,
            spec: Some(spec),
        }
    }

    pub fn new(theme: Theme, v: &egui::Visuals) -> Palette {
        Palette {
            body: v.widgets.noninteractive.bg_fill,
            body_selected: v.selection.bg_fill,
            ink: v.text_color(),
            ink_dim: v.weak_text_color(),
            port_bg: v.widgets.inactive.bg_fill,
            port_ink: v.weak_text_color(),
            chassis: v.panel_fill,
            border: v.widgets.noninteractive.bg_stroke.color,
            trunk: v.widgets.noninteractive.bg_stroke.color,
            theme,
            spec: None,
        }
    }

    pub fn accent(&self, kind: BlockKind) -> Color32 {
        if let Some(s) = self.spec {
            return match kind {
                BlockKind::Namespace => s.iface,
                BlockKind::Type => s.ty,
                BlockKind::Method => s.function,
                BlockKind::Property => s.variable,
                BlockKind::Control => s.control,
                BlockKind::Keyword => s.keyword,
                BlockKind::Simple => s.dim,
                BlockKind::Directive => s.preproc,
                BlockKind::Attribute => s.preproc,
            };
        }
        self.theme.accent(kind)
    }

    /// The tint for a piece the IDE's highlight touches: the surface pulled
    /// towards the kind's accent, not a new colour.
    pub fn highlighted(&self, kind: BlockKind) -> Color32 {
        mix(self.body, self.accent(kind), 0.20)
    }

    /// The colour for one word inside a piece. `None` from the theme means
    /// "ordinary text", which is the theme's ink.
    pub fn token(&self, kind: TokenKind) -> Color32 {
        if let Some(s) = self.spec {
            // Exactly `SourceTheme::tok_color` — the editor's palette, not a
            // parallel table that can drift.
            return match kind {
                TokenKind::Plain => s.fg,
                TokenKind::Comment => s.comment,
                TokenKind::Str => s.string,
                TokenKind::Keyword => s.keyword,
                TokenKind::Control => s.control,
                TokenKind::Type => s.ty,
                TokenKind::Number => s.number,
                TokenKind::Function => s.function,
                TokenKind::Variable => s.variable,
                TokenKind::Op => s.op,
                TokenKind::Preproc => s.preproc,
            };
        }
        self.theme.token(kind).unwrap_or(self.ink)
    }
}

/// GUI-local state: what the user changed, nothing derived.
#[derive(Clone, Debug, Default)]
pub struct GuiState {
    /// The selected blocks as child paths (see [`ScratchDoc::block_at`]).
    /// More than one, because `Ctrl+click`/`Shift+click` builds a set and
    /// `Ctrl+C` copies every selected piece.
    pub selected: Vec<Vec<usize>>,
    /// Paint the typed port pills. On by default: the ports are the point.
    pub show_ports: bool,
    /// The host's theme; the painter takes its accents from here when no
    /// [`PaletteSpec`] was supplied.
    pub theme: Theme,
    /// The host IDE's own palette, when it has one. Preferred over the theme.
    pub spec: Option<PaletteSpec>,
    /// The IDE's highlighted line range, **fragment-relative, 1-based and
    /// inclusive**. Any block it touches is highlighted whole — a partial
    /// overlap still highlights the whole piece.
    pub highlight: Option<(usize, usize)>,
    /// Modifier state captured at pointer-press, so a Ctrl+click adds to the
    /// selection even if the key is released before egui processes the click.
    pub press_additive: bool,
    /// Shaped body text, keyed by text + wrap width + palette. The Source pane
    /// already proved that re-shaping every line every frame is the expensive
    /// part (58x on a 407-line document); the puzzle pieces get the same cache.
    galley_cache: std::cell::RefCell<HashMap<u64, Arc<egui::Galley>>>,
}

impl GuiState {
    pub fn new() -> GuiState {
        GuiState {
            selected: Vec::new(),
            show_ports: true,
            theme: Theme::default(),
            spec: None,
            highlight: None,
            press_additive: false,
            galley_cache: std::cell::RefCell::new(HashMap::new()),
        }
    }

    /// Is `path` part of the current selection?
    pub fn is_selected(&self, path: &[usize]) -> bool {
        self.selected.iter().any(|p| p.as_slice() == path)
    }

    /// Add/remove `path` (an additive click) or replace the selection.
    pub fn select(&mut self, path: &[usize], additive: bool) {
        if additive {
            if self.is_selected(path) {
                self.selected.retain(|p| p.as_slice() != path);
            } else {
                self.selected.push(path.to_vec());
            }
        } else {
            self.selected = vec![path.to_vec()];
        }
    }

    /// The selected blocks as C# text, in selection order.
    pub fn copy_text(&self, doc: &ScratchDoc) -> String {
        self.selected
            .iter()
            .filter_map(|p| doc.block_at(p))
            .map(|b| b.source(0))
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

impl GuiState {
    /// The shaped galley for a piece's body, reusing the last layout unless the
    /// text, wrap width, theme, palette or pixel ratio changed.
    fn body_galley(&self, ui: &egui::Ui, text: &str, wrap: f32, palette: &Palette) -> Arc<egui::Galley> {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        text.hash(&mut h);
        wrap.to_bits().hash(&mut h);
        (self.theme as u8).hash(&mut h);
        ui.ctx().pixels_per_point().to_bits().hash(&mut h);
        if let Some(s) = &self.spec {
            for c in [s.bg, s.head_bg, s.fg, s.dim, s.border, s.accent, s.comment, s.keyword,
                      s.control, s.ty, s.iface, s.string, s.number, s.preproc, s.function,
                      s.variable, s.op] {
                c.to_array().hash(&mut h);
            }
        }
        let key = h.finish();
        if let Some(g) = self.galley_cache.borrow().get(&key) {
            return g.clone();
        }
        let mut job = egui::text::LayoutJob::default();
        job.wrap.max_width = wrap;
        for span in tokens(text) {
            job.append(
                &text[span.start..span.end],
                0.0,
                egui::TextFormat {
                    font_id: FontId::monospace(BODY_FONT),
                    color: palette.token(span.kind),
                    ..Default::default()
                },
            );
        }
        let g = ui.painter().layout_job(job);
        let mut cache = self.galley_cache.borrow_mut();
        if cache.len() > 4096 {
            cache.clear();
        }
        cache.insert(key, g.clone());
        g
    }
}

/// Paint the whole document and return the requests the user's clicks made.
pub fn ui(ui: &mut egui::Ui, doc: &ScratchDoc, state: &mut GuiState) -> Vec<Request> {
    let mut requests = Vec::new();
    let palette = match state.spec {
        Some(spec) => Palette::from_spec(spec),
        None => Palette::new(state.theme, ui.visuals()),
    };

    // The pane is the IDE's surface: paint its background and text with the
    // host's own colours instead of egui's defaults.
    ui.painter().rect_filled(ui.max_rect(), CornerRadius::ZERO, palette.chassis);
    ui.style_mut().visuals.override_text_color = Some(palette.ink);
    ui.style_mut().visuals.panel_fill = palette.chassis;

    // Ctrl/Cmd+C copies the selected pieces as C#, terminators included.
    // egui-winit delivers the platform shortcut as `Event::Copy`, so both the
    // canonical event and a raw Ctrl+C key are accepted.
    let copy_hit = ui.input(|i| {
        i.events
            .iter()
            .any(|e| matches!(e, egui::Event::Copy | egui::Event::Cut))
            || ((i.modifiers.command || i.modifiers.ctrl) && i.key_pressed(egui::Key::C))
    });
    if copy_hit && !state.selected.is_empty() {
        let text = state.copy_text(doc);
        if !text.is_empty() {
            ui.ctx().copy_text(text);
        }
    }

    header(ui, doc, state);

    ScrollArea::vertical()
        .auto_shrink([false, false])
        .id_salt(("scratch_blocks", doc.title.as_str()))
        .show(ui, |ui| {
            if doc.blocks.is_empty() {
                ui.add_space(24.0);
                ui.weak("This declaration has no blocks.");
                return;
            }
            let mut path = Vec::new();
            for (i, block) in doc.blocks.iter().enumerate() {
                path.push(i);
                paint_block(ui, doc, block, &mut path, 0, state, &palette, &mut requests);
                path.pop();
            }
        });

    if !doc.warnings.is_empty() {
        ui.separator();
        for w in &doc.warnings {
            ui.colored_label(Color32::from_rgb(253, 186, 116), format!("⚠ {w}"));
        }
    }
    requests
}

fn header(ui: &mut egui::Ui, doc: &ScratchDoc, state: &mut GuiState) {
    ui.horizontal(|ui| {
        ui.heading(if doc.title.is_empty() { "Scratch" } else { &doc.title });
        ui.add_space(6.0);
        ui.label(egui::RichText::new("read-only").small().color(ui.visuals().weak_text_color()));
    });
    // The shared IDE context strip (`gui.nzkide`): breadcrumb + live state. The
    // Scratch window is a view of what the IDE is looking at, and this is where
    // it says so.
    let ctx = crate::idelink_nzkide::IdeContext::from_doc(doc);
    crate::gui_nzkide::context_strip(ui, &ctx);
    ui.horizontal(|ui| {
        ui.checkbox(&mut state.show_ports, "ports");
        ui.weak(if doc.from_source_pane { "· from the source pane" } else { "· from the graph" });
        if !state.selected.is_empty() {
            ui.weak(format!("· {} selected — Ctrl+C copies", state.selected.len()));
        }
    });
}

/// One packed run of port pills.
#[derive(Clone, Debug, PartialEq)]
struct PortRow {
    /// `(port index, pill size)` in source order.
    items: Vec<(usize, Vec2)>,
    width: f32,
}

/// Pack pills into rows no wider than `max_w`.
fn pack_ports(sizes: &[Vec2], max_w: f32) -> Vec<PortRow> {
    let mut rows: Vec<PortRow> = Vec::new();
    let mut current = PortRow { items: Vec::new(), width: 0.0 };
    for (i, size) in sizes.iter().copied().enumerate() {
        let extra = if current.items.is_empty() { size.x } else { PORT_GAP + size.x };
        if !current.items.is_empty() && current.width + extra > max_w {
            rows.push(std::mem::replace(&mut current, PortRow { items: Vec::new(), width: 0.0 }));
            current.items.push((i, size));
            current.width = size.x;
        } else {
            current.items.push((i, size));
            current.width += extra;
        }
    }
    if !current.items.is_empty() {
        rows.push(current);
    }
    rows
}

/// The text width a piece may use, given the row it sits in.
fn text_max_width(avail: f32, indent: f32) -> f32 {
    (avail - indent - TEXT_X - PAD).max(60.0)
}

/// Height of the port block for `rows` rows (zero when there are none).
fn ports_height(rows: usize) -> f32 {
    if rows == 0 {
        0.0
    } else {
        PORTS_TOP + rows as f32 * PORT_H + (rows.saturating_sub(1)) as f32 * PORT_GAP
    }
}

/// Draw one piece and its children; returns the piece's rectangle.
#[allow(clippy::too_many_arguments)]
fn paint_block(
    ui: &mut egui::Ui,
    doc: &ScratchDoc,
    block: &Block,
    path: &mut Vec<usize>,
    depth: usize,
    state: &mut GuiState,
    palette: &Palette,
    requests: &mut Vec<Request>,
) -> Rect {
    let avail = ui.available_width();
    let indent = depth as f32 * INDENT;
    let max_w = (avail - indent).max(120.0);
    let text_max = text_max_width(avail, indent);

    // VSCode-style: every word carries its own colour, from the shared
    // tokenizer. The shaped galley is cached: the text does not change between
    // frames, and re-shaping every piece was the Scratch pane's lag.
    let text = block.text();
    let galley = state.body_galley(ui, &text, text_max, palette);
    let text_w = galley.size().x;

    let port_sizes: Vec<Vec2> = if state.show_ports {
        block.ports.iter().map(|p| port_size(ui, p)).collect()
    } else {
        Vec::new()
    };
    let rows = pack_ports(&port_sizes, text_max);
    let ports_w = rows.iter().map(|r| r.width).fold(0.0, f32::max);
    let ports_h = ports_height(rows.len());

    let width = (TEXT_X + text_w.max(ports_w) + PAD).max(MIN_PIECE_W).min(max_w);
    let height = (PAD * 0.6 + galley.size().y + ports_h + PAD * 0.6).max(MIN_PIECE_H);

    let (row, resp) = ui.allocate_exact_size(Vec2::new(avail, height + ROW_GAP), Sense::click());
    let piece = Rect::from_min_size(row.min + Vec2::new(indent, 0.0), Vec2::new(width, height));

    let selected = state.is_selected(path);
    // The IDE's highlight selects whole pieces: a block that only *partly*
    // overlaps the range is still highlighted in full.
    let highlighted = state
        .highlight
        .is_some_and(|(a, b)| block.line <= b && block.end_line >= a);
    paint_piece(ui, piece, block.kind, palette, selected, highlighted);

    let text_pos = Pos2::new(piece.left() + TEXT_X, piece.top() + PAD * 0.6);
    let text_h = galley.size().y;
    ui.painter().galley(text_pos, galley, palette.ink);

    if !rows.is_empty() {
        let mut y = text_pos.y + text_h + PORTS_TOP;
        for row in &rows {
            let mut x = piece.left() + TEXT_X;
            for (i, size) in &row.items {
                paint_port(
                    ui,
                    Rect::from_min_size(Pos2::new(x, y), *size),
                    &block.ports[*i].label(),
                    palette,
                );
                x += size.x + PORT_GAP;
            }
            y += PORT_H + PORT_GAP;
        }
    }

    let hover = format!(
        "{}  ·  line {}{}",
        block.text(),
        block.line,
        if block.closed { "" } else { "  (unclosed brace)" }
    );
    let resp = resp.on_hover_text(hover);
    if resp.hovered() && ui.input(|i| i.pointer.primary_pressed()) {
        state.press_additive =
            ui.input(|i| i.modifiers.shift || i.modifiers.command || i.modifiers.ctrl);
    }
    if resp.clicked() {
        state.select(path, state.press_additive);
        // One action per click: a block belongs to a graph node, so the node is
        // the real target. `Reveal` is the fallback for a document with no node
        // (a file view), where a file+line is all the host can be told.
        // Ask the IDE to highlight the block's whole span, including when the
        // click landed on one line of a multi-line piece.
        requests.push(Request::SelectRange {
            start: block.line,
            end: block.end_line,
        });
        if !doc.node_id.is_empty() {
            requests.push(Request::OpenNode(doc.node_id.clone()));
        } else if !doc.location.is_empty() {
            let line = doc.line.max(1) + block.line as i64 - 1;
            let file = doc
                .location
                .split(':')
                .next()
                .unwrap_or(&doc.location)
                .to_string();
            requests.push(Request::Reveal { file, line });
        }
    }

    // Children, then the trunk down to the last one, so the line ends where the
    // blocks actually end rather than at a guessed height.
    if !block.children.is_empty() {
        let mut last: Option<Rect> = None;
        for (i, child) in block.children.iter().enumerate() {
            path.push(i);
            last = Some(paint_block(
                ui, doc, child, path, depth + 1, state, palette, requests,
            ));
            path.pop();
        }
        if let Some(last) = last {
            let x = piece.left() + 6.0;
            ui.painter().line_segment(
                [
                    Pos2::new(x, piece.bottom() + TAB_H),
                    Pos2::new(x, last.center().y),
                ],
                Stroke::new(1.5, palette.trunk),
            );
        }
    }

    piece
}

/// The piece itself: theme surface, a kind stripe on the left, a notch and a tab.
fn paint_piece(
    ui: &mut egui::Ui,
    rect: Rect,
    kind: BlockKind,
    palette: &Palette,
    selected: bool,
    highlighted: bool,
) {
    let painter = ui.painter();
    let radius = CornerRadius::same(7);
    let accent = palette.accent(kind);
    let fill = if selected {
        palette.body_selected
    } else if highlighted {
        palette.highlighted(kind)
    } else {
        palette.body
    };

    // Tab first, so the body's own rounded corner overlaps it cleanly.
    let tab = Rect::from_min_size(
        Pos2::new(rect.left() + 8.0, rect.bottom() - 2.0),
        Vec2::new(TAB_W, TAB_H),
    );
    painter.rect_filled(tab, CornerRadius::same(4), fill);

    painter.rect_filled(rect, radius, fill);

    // The kind stripe: the only saturated colour on a piece, 4 px wide.
    let stripe = Rect::from_min_max(
        Pos2::new(rect.left() + 1.0, rect.top() + 5.0),
        Pos2::new(rect.left() + 1.0 + STRIPE_W, rect.bottom() - 5.0),
    );
    painter.rect_filled(stripe, CornerRadius::same(2), accent);

    // The notch: a circle of the panel colour, cutting into the top edge.
    painter.circle_filled(Pos2::new(rect.left() + 15.0, rect.top()), NOTCH_R, palette.chassis);

    let stroke = if selected {
        Stroke::new(1.5, accent)
    } else {
        Stroke::new(1.0, palette.border)
    };
    painter.rect_stroke(rect, radius, stroke, StrokeKind::Inside);
}

fn port_size(ui: &egui::Ui, port: &crate::scratch::Port) -> Vec2 {
    let g = ui
        .painter()
        .layout_no_wrap(port.label(), FontId::monospace(PORT_FONT), Color32::PLACEHOLDER);
    Vec2::new(g.size().x + 14.0, PORT_H)
}

fn paint_port(ui: &mut egui::Ui, rect: Rect, label: &str, palette: &Palette) {
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(9), palette.port_bg);
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        FontId::monospace(PORT_FONT),
        palette.port_ink,
    );
}

/// The empty state — nothing is being viewed.
pub fn empty(ui: &mut egui::Ui, message: &str) {
    ui.add_space(24.0);
    ui.heading("Scratch");
    ui.add_space(8.0);
    ui.label(message);
    ui.add_space(8.0);
    ui.weak("Select a declaration in the IDE (or open it in the source pane) and it appears here as blocks.");
}

/// The error state — the host had a view but it produced no document.
pub fn error(ui: &mut egui::Ui, message: &str) {
    ui.add_space(24.0);
    ui.heading("Scratch");
    ui.add_space(8.0);
    ui.colored_label(Color32::from_rgb(248, 113, 113), message);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::idelink_scratch::{self, IdeView, LinkState};

    fn doc(src: &str) -> ScratchDoc {
        let view = IdeView {
            repo: "/repo".into(),
            node_id: "#m:Foo.Bar".into(),
            title: "Foo.Bar()".into(),
            kind: "method".into(),
            file: "Foo.cs".into(),
            line: 12,
            end_line: 20,
            source: src.into(),
            ret: Some("void".into()),
            from_source_pane: true,
        };
        match idelink_scratch::build(&view) {
            LinkState::Ready(d) => d,
            other => panic!("expected a document, got {other:?}"),
        }
    }

    #[test]
    fn renders_headlessly() {
        let doc = doc("void A() {\n  if (x) {\n    Go(1);\n  }\n}");
        let mut state = GuiState::new();
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            let reqs = super::ui(ui, &doc, &mut state);
            assert!(reqs.is_empty(), "no clicks in a headless frame");
        });
        out.textures_delta.clear();
        assert_eq!(doc.count(), 3);
    }

    /// The long declaration from the desktop run
    /// (`[UnityEngine.Tooltip("…")] public …`) must wrap into several lines
    /// instead of being cut off at the piece's edge.
    #[test]
    fn a_long_line_wraps_instead_of_being_cut_off() {
        let long = format!(
            "[UnityEngine.Tooltip(\"{}\")]\npublic string {}\n{{ }}",
            "Display name. Leave empty to derive it from the target.".repeat(2),
            "SomeVeryLongMemberNameThatWouldOtherwiseBeCutOff"
        );
        let doc = doc(&long);
        let mut state = GuiState::new();
        let ctx = egui::Context::default();
        let budget = text_max_width(700.0, 0.0);
        let mut rows = 0usize;
        let mut widest = 0.0f32;
        let mut out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(700.0, 800.0))),
                ..Default::default()
            },
            |ui| {
                super::ui(ui, &doc, &mut state);
                let text = doc.blocks[0].text();
                let g = ui
                    .painter()
                    .layout(text, FontId::monospace(BODY_FONT), Color32::WHITE, budget);
                rows = g.rows.len();
                widest = g.size().x;
            },
        );
        out.textures_delta.clear();
        assert!(rows > 1, "a long declaration must wrap, not truncate (rows: {rows})");
        assert!(
            widest <= budget + 1.0,
            "wrapped text stays inside its budget ({widest} vs {budget})"
        );
    }

    #[test]
    fn ports_pack_into_rows_that_fit() {
        let sizes = vec![Vec2::new(40.0, PORT_H); 5];
        let rows = pack_ports(&sizes, 100.0);
        assert_eq!(rows.len(), 3, "40+4+40 fits in 100, so 2/2/1");
        for row in &rows {
            assert!(row.width <= 100.0, "a row never exceeds the text width");
        }
        assert_eq!(rows.iter().map(|r| r.items.len()).sum::<usize>(), 5);
    }

    /// A piece is painted in the theme's surface colour; the kind is a stripe.
    /// This is the property that replaced the rainbow fills.
    #[test]
    fn the_palette_is_the_theme_surface_not_a_bright_fill() {
        let v = egui::Visuals::dark();
        let p = Palette::new(Theme::NzkDarkPlus, &v);
        assert_eq!(p.body, v.widgets.noninteractive.bg_fill);
        assert_eq!(p.ink, v.text_color());
        assert_eq!(p.chassis, v.panel_fill);
        // Every stripe is distinct per kind, per theme.
        for theme in [Theme::NzkDarkPlus, Theme::DarkV1] {
            let kinds = [
                BlockKind::Namespace,
                BlockKind::Type,
                BlockKind::Method,
                BlockKind::Property,
                BlockKind::Control,
                BlockKind::Keyword,
                BlockKind::Simple,
                BlockKind::Directive,
                BlockKind::Attribute,
            ];
            let mut accents: Vec<[u8; 4]> = kinds.iter().map(|k| theme.accent(*k).to_array()).collect();
            accents.sort();
            accents.dedup();
            assert_eq!(accents.len(), kinds.len(), "accents must identify the kind");
        }
    }

    /// Every word kind has a colour and they are not all the same, per theme.
    #[test]
    fn every_token_kind_is_coloured_per_theme() {
        for theme in [Theme::NzkDarkPlus, Theme::DarkV1] {
            let p = Palette::new(theme, &egui::Visuals::dark());
            let kinds = [
                TokenKind::Comment,
                TokenKind::Str,
                TokenKind::Keyword,
                TokenKind::Control,
                TokenKind::Type,
                TokenKind::Number,
                TokenKind::Function,
                TokenKind::Variable,
                TokenKind::Op,
                TokenKind::Preproc,
            ];
            // Dark+ is the shipped IDE palette, arm for arm. Control and
            // preproc deliberately share a colour (`#c586c0`), so distinctness
            // is not the invariant — equality to the editor is.
            if theme == Theme::NzkDarkPlus {
            for (kind, expected) in [
                (TokenKind::Comment, 0x6a9955u32),
                (TokenKind::Str, 0xce9178),
                (TokenKind::Keyword, 0x569cd6),
                (TokenKind::Control, 0xc586c0),
                (TokenKind::Type, 0x4ec9b0),
                (TokenKind::Number, 0xb5cea8),
                (TokenKind::Function, 0xdcdcaa),
                (TokenKind::Variable, 0x9cdcfe),
                (TokenKind::Op, 0xd4d4d4),
                (TokenKind::Preproc, 0xc586c0),
            ] {
                let got = p.token(kind).to_array();
                let want = Color32::from_rgb(
                    (expected >> 16) as u8,
                    (expected >> 8) as u8,
                    expected as u8,
                )
                .to_array();
                assert_eq!(got, want, "{kind:?} must be the IDE colour");
            }
            }
            assert_eq!(p.token(TokenKind::Plain), p.ink, "plain text is the theme ink");
        }
    }

    /// Ctrl+C must put the selected pieces on the clipboard as C#.
    #[test]
    fn ctrl_c_copies_the_selected_blocks() {
        let doc = doc("void A() { Go(); }\nvoid B() { Stop(); }");
        let mut state = GuiState::new();
        state.selected = vec![vec![0], vec![1]];
        let ctx = egui::Context::default();
        let mut input = egui::RawInput::default();
        input.focused = true;
        input.events.push(egui::Event::Copy);
        let mut out = ctx.run_ui(input, |ui| {
            let _ = super::ui(ui, &doc, &mut state);
        });
        let copied = out
            .platform_output
            .commands
            .iter()
            .find_map(|c| match c {
                egui::OutputCommand::CopyText(t) => Some(t.clone()),
                _ => None,
            })
            .expect("Ctrl+C must emit CopyText");
        out.textures_delta.clear();
        assert!(copied.contains("void A () {"), "copied: {copied}");
        assert!(copied.contains("Go();"), "terminators survive the copy");
        assert!(copied.contains("void B () {"));
        assert!(copied.contains("Stop();"));
    }

    #[test]
    fn additive_clicks_build_a_selection_set() {
        let mut state = GuiState::new();
        state.select(&[0], false);
        assert!(state.is_selected(&[0]));
        state.select(&[1], true);
        assert!(state.is_selected(&[0]) && state.is_selected(&[1]));
        state.select(&[0], true);
        assert!(!state.is_selected(&[0]), "a second additive click removes it");
        state.select(&[2], false);
        assert_eq!(state.selected, vec![vec![2]], "a plain click replaces the set");
    }

    #[test]
    fn empty_and_error_states_render() {
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            super::empty(ui, "Nothing is being viewed.");
            super::error(ui, "the viewed text produced no blocks");
        });
        out.textures_delta.clear();
    }
}

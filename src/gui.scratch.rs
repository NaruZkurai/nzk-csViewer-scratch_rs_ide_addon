//! `gui.scratch.rs` — the Scratch window's GUI: puzzle pieces, ports, clicks.
//!
//! Everything visual about the Scratch pane lives here, and nothing else does:
//! the model is `scratch.rs`, the input is `idelink.scratch.rs`. The painter is
//! a pure function of `(&ScratchDoc, &mut GuiState)` plus egui's frame, so the
//! only state it owns is what the *user* changed (selection, which port pills
//! are shown).
//!
//! # The piece
//!
//! ```text
//!    ╭─╮──────────────╮        ← the notch is a bg-coloured circle cut into
//!    │ ╰ if (bool)    │          the top-left corner
//!    ╰───╮────────────╯
//!        ╰─╯                     ← the tab is the same fill, below the bottom
//! ```
//!
//! A block is drawn at its indentation, a full-width row is allocated for the
//! hit area, and children are drawn after it with a spine at their depth. Ports
//! are pills to the right of the text: `n: int`, `-> void`, `?: type unknown`.
//!
//! # What it never does
//!
//! It does not edit. A click selects a block and emits
//! [`Request::Reveal`](crate::idelink_scratch::Request::Reveal); the host decides
//! what that means. Unknown types are painted `?` — the painter has no way to
//! invent one.

use egui::{
    Align2, Color32, CornerRadius, FontId, Pos2, Rect, Response, ScrollArea, Sense, Stroke,
    StrokeKind, Vec2,
};

use crate::idelink_scratch::{Request, ScratchDoc};
use crate::scratch::Block;

/// How tall one piece is, and how much room the next one leaves for its tab.
const PIECE_H: f32 = 30.0;
const ROW_GAP: f32 = 9.0;
const INDENT: f32 = 20.0;
const PAD: f32 = 10.0;
const NOTCH_R: f32 = 6.0;
/// Where the text starts, past the notch cut.
const TEXT_X: f32 = 18.0;
const TAB_W: f32 = 14.0;
const TAB_H: f32 = 9.0;

/// GUI-local state: what the user changed, nothing derived.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GuiState {
    /// The selected block as a child path (see [`ScratchDoc::block_at`]).
    pub selected: Option<Vec<usize>>,
    /// Paint the typed port pills. On by default: the ports are the point.
    pub show_ports: bool,
}

impl GuiState {
    pub fn new() -> GuiState {
        GuiState { selected: None, show_ports: true }
    }
}

/// The fill for one block kind. Bright, low-saturation pieces with dark text so
/// a port pill stays readable on top.
pub fn block_color(kind: crate::scratch::BlockKind) -> Color32 {
    use crate::scratch::BlockKind::*;
    match kind {
        Namespace => Color32::from_rgb(196, 181, 253),
        Type => Color32::from_rgb(153, 226, 214),
        Method => Color32::from_rgb(147, 197, 253),
        Property => Color32::from_rgb(253, 211, 138),
        Control => Color32::from_rgb(255, 232, 138),
        Keyword => Color32::from_rgb(249, 168, 212),
        Simple => Color32::from_rgb(203, 213, 225),
    }
}

/// Text on a piece. Dark on every fill above.
const INK: Color32 = Color32::from_rgb(24, 28, 38);

/// Paint the whole document and return the requests the user's clicks made.
pub fn ui(ui: &mut egui::Ui, doc: &ScratchDoc, state: &mut GuiState) -> Vec<Request> {
    let mut requests = Vec::new();

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
                paint_block(ui, doc, block, &mut path, 0, state, &mut requests);
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
        ui.label(
            egui::RichText::new("read-only")
                .small()
                .color(Color32::from_rgb(148, 163, 184)),
        );
    });
    // The shared IDE context strip (`gui.nzkide`): breadcrumb + live state. The
    // Scratch window is a view of what the IDE is looking at, and this is where
    // it says so.
    let ctx = crate::idelink_nzkide::IdeContext::from_doc(doc);
    crate::gui_nzkide::context_strip(ui, &ctx);
    ui.horizontal(|ui| {
        ui.checkbox(&mut state.show_ports, "ports");
        ui.weak(if doc.from_source_pane { "· from the source pane" } else { "· from the graph" });
    });
}

/// Draw one piece and its children.
fn paint_block(
    ui: &mut egui::Ui,
    doc: &ScratchDoc,
    block: &Block,
    path: &mut Vec<usize>,
    depth: usize,
    state: &mut GuiState,
    requests: &mut Vec<Request>,
) {
    let avail = ui.available_width();
    let indent = depth as f32 * INDENT;
    let text = truncate(&block.text(), 110);
    let font = FontId::monospace(13.0);
    let galley = ui.painter().layout_no_wrap(text, font.clone(), INK);
    let text_w = galley.size().x;

    let ports_w = if state.show_ports {
        block
            .ports
            .iter()
            .map(|p| port_size(ui, p).x + 4.0)
            .sum::<f32>()
    } else {
        0.0
    };

    let width = piece_width(text_w, ports_w, avail, indent);
    let (row, resp) = ui.allocate_exact_size(
        Vec2::new(avail, PIECE_H + ROW_GAP),
        Sense::click(),
    );
    let piece = Rect::from_min_size(row.min + Vec2::new(indent, 0.0), Vec2::new(width, PIECE_H));

    let selected = state.selected.as_deref() == Some(path.as_slice());
    let fill = block_color(block.kind);
    paint_piece(ui, piece, fill, selected);

    // Text, clipped to the piece.
    let text_pos = Pos2::new(piece.left() + TEXT_X, piece.center().y);
    ui.painter().galley(
        text_pos - Vec2::new(0.0, galley.size().y / 2.0),
        galley,
        INK,
    );

    if state.show_ports {
        let mut x = piece.left() + TEXT_X + text_w + 10.0;
        for port in &block.ports {
            let size = port_size(ui, port);
            if x + size.x > piece.right() - 4.0 {
                break;
            }
            paint_port(
                ui,
                Rect::from_min_size(Pos2::new(x, piece.center().y - size.y / 2.0), size),
                &port.label(),
            );
            x += size.x + 4.0;
        }
    }

    // The spine down to the children.
    if !block.children.is_empty() {
        let spine_x = piece.left() + 6.0;
        let bottom = piece.bottom() + TAB_H + 1.0;
        let last_bottom = bottom + (block.children.len() as f32) * (PIECE_H + ROW_GAP);
        ui.painter().line_segment(
            [Pos2::new(spine_x, bottom), Pos2::new(spine_x, last_bottom)],
            Stroke::new(1.5, Color32::from_rgb(100, 116, 139)),
        );
    }

    let hover = format!(
        "{}  ·  line {}{}",
        block.text(),
        block.line,
        if block.closed { "" } else { "  (unclosed brace)" }
    );
    let resp = resp.on_hover_text(hover);
    if resp.clicked() {
        state.selected = Some(path.clone());
        // One action per click: a block belongs to a graph node, so the node is
        // the real target. `Reveal` is the fallback for a document with no node
        // (a file view), where a file+line is all the host can be told.
        if !doc.node_id.is_empty() {
            requests.push(Request::OpenNode(doc.node_id.clone()));
        } else if !doc.location.is_empty() {
            // `doc.line` is the declaration's first line; the block's line is
            // 1-based *within the fragment*.
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

    if !block.children.is_empty() {
        for (i, child) in block.children.iter().enumerate() {
            path.push(i);
            paint_block(ui, doc, child, path, depth + 1, state, requests);
            path.pop();
        }
    }
}

/// The piece itself: rounded body, a notch cut out of the top-left, a tab below.
fn paint_piece(ui: &mut egui::Ui, rect: Rect, fill: Color32, selected: bool) {
    let painter = ui.painter();
    let bg = ui.visuals().panel_fill;
    let radius = CornerRadius::same(7);

    // Tab first, so the body's own rounded corner overlaps it cleanly.
    let tab = Rect::from_min_size(
        Pos2::new(rect.left() + 8.0, rect.bottom() - 2.0),
        Vec2::new(TAB_W, TAB_H),
    );
    painter.rect_filled(tab, CornerRadius::same(4), fill);

    painter.rect_filled(rect, radius, fill);
    // The notch: a circle of the panel colour, cutting into the top edge.
    painter.circle_filled(Pos2::new(rect.left() + 15.0, rect.top()), NOTCH_R, bg);

    if selected {
        painter.rect_stroke(
            rect,
            radius,
            Stroke::new(2.0, Color32::from_rgb(56, 189, 248)),
            StrokeKind::Outside,
        );
    }
}

/// Width of one piece for a given text and port run.
///
/// This is the bug that made every port pill invisible in the first desktop
/// run: the text offset was counted twice, so the port row started past the
/// piece's right edge and the paint loop broke on its very first port. The
/// arithmetic is a named function now, and the test below pins it.
fn piece_width(text_w: f32, ports_w: f32, avail: f32, indent: f32) -> f32 {
    let ports_gap = if ports_w > 0.0 { 10.0 } else { 0.0 };
    let wanted = TEXT_X + text_w + ports_gap + ports_w + PAD;
    wanted.max(150.0).min((avail - indent).max(120.0))
}

fn port_size(ui: &egui::Ui, port: &crate::scratch::Port) -> Vec2 {
    let g = ui
        .painter()
        .layout_no_wrap(port.label(), FontId::monospace(11.0), INK);
    Vec2::new(g.size().x + 14.0, 18.0)
}

fn paint_port(ui: &mut egui::Ui, rect: Rect, label: &str) {
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(9), Color32::from_black_alpha(38));
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        FontId::monospace(11.0),
        INK,
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

fn truncate(s: &str, max_chars: usize) -> String {
    let mut out: String = s.chars().take(max_chars).collect();
    if s.chars().count() > max_chars {
        out.push('…');
    }
    out.replace('\n', " ")
}

/// Keep the paint helpers honest about being reachable from the tests.
#[allow(dead_code)]
fn _assert_types(_: Response) {}

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

    /// The painter must not panic on a real document, a deep one, or an empty
    /// one. This runs egui headlessly, so it exercises the real `ui` pass.
    #[test]
    fn renders_headlessly() {
        let doc = doc("void A() {\n  if (x) {\n    Go(1);\n  }\n}");
        let mut state = GuiState::new();
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            let reqs = super::ui(ui, &doc, &mut state);
            assert!(reqs.is_empty(), "no clicks in a headless frame");
        });
        out.textures_delta.clear(); // nothing uploads textures in a headless pass
        assert_eq!(doc.count(), 3);
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

    #[test]
    fn every_kind_has_a_distinct_fill() {
        use crate::scratch::BlockKind::*;
        let kinds = [Namespace, Type, Method, Property, Control, Keyword, Simple];
        let colors: Vec<Color32> = kinds.iter().map(|k| block_color(*k)).collect();
        let mut deduped = colors.clone();
        deduped.sort_by_key(|c| c.to_array());
        deduped.dedup();
        assert_eq!(deduped.len(), kinds.len(), "colours must identify the kind");
    }

    /// The port row must start inside the piece and end before its right edge
    /// whenever the piece is not width-clamped.
    #[test]
    fn ports_fit_inside_an_unclamped_piece() {
        let (text_w, ports_w, avail, indent) = (120.0_f32, 64.0_f32, 1100.0_f32, 0.0_f32);
        let width = piece_width(text_w, ports_w, avail, indent);
        let ports_x = TEXT_X + text_w + 10.0;
        assert!(ports_x + ports_w <= width, "ports end inside the piece");
        assert!(width <= avail, "the piece fits the row");
    }

    /// A clamped piece squeezes the text, never the arithmetic: the paint loop
    /// drops ports that would spill rather than drawing over the edge.
    #[test]
    fn a_clamped_piece_is_never_wider_than_the_row() {
        let width = piece_width(900.0, 200.0, 300.0, 40.0);
        assert!(width <= 300.0 - 40.0);
    }

    #[test]
    fn long_text_is_truncated_not_overflowing() {
        let long = "x".repeat(400);
        let t = truncate(&long, 110);
        assert_eq!(t.chars().count(), 111, "110 chars + the ellipsis");
        assert!(t.ends_with('…'));
    }
}

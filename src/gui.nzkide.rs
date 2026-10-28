//! `gui.nzkide.rs` — the IDE window's chrome, as far as the Scratch pane needs it.
//!
//! A thin VSCode-style context strip: breadcrumb on the left, live state on the
//! right. It is deliberately small — this is the *shared* strip a future IDE
//! window and the Scratch window both paint, so it has no window of its own and
//! no state. Everything it shows comes from
//! [`IdeContext`](crate::idelink_nzkide::IdeContext).
//!
//! ```text
//! vrcCS  ▸  Foo.cs:3-9  ▸  Foo.Bar()        source pane · method · 2 blocks   Ctrl+Alt+S
//! ```

use egui::{Align, Color32, Layout, RichText};

use crate::idelink_nzkide::IdeContext;

/// The chord that opens the Scratch window. One spelling, used by the strip and
/// quoted in the host's shortcut table.
pub const OPEN_CHORD: &str = "Ctrl+Alt+S";

/// Paint the context strip and its separator.
pub fn context_strip(ui: &mut egui::Ui, ctx: &IdeContext) {
    ui.horizontal(|ui| {
        let crumb = ctx.breadcrumb();
        if crumb.is_empty() {
            ui.weak("(no IDE context)");
        } else {
            // Truncate rather than run under the right-hand status on a narrow
            // window (the pop-out is 1100 px and the breadcrumb is long).
            ui.add(
                egui::Label::new(
                    RichText::new(crumb)
                        .monospace()
                        .size(11.5)
                        .color(Color32::from_rgb(148, 163, 184)),
                )
                .truncate(),
            );
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(
                RichText::new(OPEN_CHORD)
                    .monospace()
                    .size(10.5)
                    .color(Color32::from_rgb(100, 116, 139)),
            );
            ui.label(
                RichText::new("scratch")
                    .small()
                    .color(Color32::from_rgb(56, 189, 248)),
            );
            ui.weak(ctx.status());
        });
    });
    ui.separator();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::idelink_nzkide::Surface;

    fn ctx() -> IdeContext {
        IdeContext {
            repo: "vrcCS".into(),
            file: "Foo.cs:3-9".into(),
            symbol: "Foo.Bar()".into(),
            kind: "method".into(),
            blocks: 2,
            surface: Surface::SourcePane,
        }
    }

    #[test]
    fn the_strip_renders_headlessly() {
        let ctx = ctx();
        let mut out = egui::Context::default().run_ui(egui::RawInput::default(), |ui| {
            context_strip(ui, &ctx);
            context_strip(ui, &IdeContext::nothing("vrcCS"));
        });
        out.textures_delta.clear();
    }

    #[test]
    fn the_chord_is_spelled_once() {
        assert_eq!(OPEN_CHORD, "Ctrl+Alt+S");
    }
}

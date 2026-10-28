//! `idelink.nzkide.rs` — the link to the IDE window's context.
//!
//! The Scratch pane is not a stand-alone toy: it is a *view of what the IDE is
//! looking at*. This module is that fact as data — [`IdeContext`] — plus the one
//! pure formatting rule the IDE window and the Scratch window share: the
//! breadcrumb.
//!
//! Kept separate from `idelink.scratch.rs` on purpose:
//!
//! * `idelink.scratch.rs` answers "turn this C# into blocks" (language link).
//! * `idelink.nzkide.rs` answers "what is the IDE showing, and where" (window
//!   link). A future `gui.nzkide` IDE window paints from this and nothing else.
//!
//! Pure data: no `egui`, no host types, so the breadcrumb is unit-tested.

use crate::idelink_scratch::ScratchDoc;

/// Which IDE surface supplied the block document.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Surface {
    /// The source pane's loaded document — the primary truth.
    SourcePane,
    /// The graph selection, when the pane has nothing open.
    GraphSelection,
    /// Nothing is being viewed.
    Nothing,
}

impl Surface {
    pub fn label(self) -> &'static str {
        match self {
            Surface::SourcePane => "source pane",
            Surface::GraphSelection => "graph selection",
            Surface::Nothing => "nothing",
        }
    }
}

/// The IDE context the Scratch window advertises.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IdeContext {
    pub repo: String,
    pub file: String,
    pub symbol: String,
    pub kind: String,
    /// Blocks in the current document, for the strip's counter.
    pub blocks: usize,
    pub surface: Surface,
}

impl Default for Surface {
    fn default() -> Self {
        Surface::Nothing
    }
}

impl IdeContext {
    /// Build the context from the document the Scratch pane is showing.
    pub fn from_doc(doc: &ScratchDoc) -> IdeContext {
        IdeContext {
            repo: doc.repo.clone(),
            file: doc.location.clone(),
            symbol: if doc.title.is_empty() { doc.node_id.clone() } else { doc.title.clone() },
            kind: doc.kind.clone(),
            blocks: doc.count(),
            surface: if doc.from_source_pane {
                Surface::SourcePane
            } else {
                Surface::GraphSelection
            },
        }
    }

    /// The empty context: the IDE is showing nothing the pane can describe.
    pub fn nothing(repo: &str) -> IdeContext {
        IdeContext {
            repo: repo.to_string(),
            surface: Surface::Nothing,
            ..IdeContext::default()
        }
    }

    /// `repo ▸ file ▸ symbol`, dropping the parts that are empty — never leaving
    /// a dangling separator.
    pub fn breadcrumb(&self) -> String {
        let mut parts: Vec<&str> = Vec::new();
        if !self.repo.is_empty() {
            parts.push(&self.repo);
        }
        if !self.file.is_empty() {
            parts.push(&self.file);
        }
        if !self.symbol.is_empty() {
            parts.push(&self.symbol);
        }
        parts.join("  ▸  ")
    }

    /// The right-hand status: `source pane · method · 12 blocks`.
    pub fn status(&self) -> String {
        let mut parts = vec![self.surface.label()];
        if !self.kind.is_empty() {
            parts.push(&self.kind);
        }
        let blocks = format!("{} blocks", self.blocks);
        parts.push(&blocks);
        parts.join(" · ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::idelink_scratch::{self, IdeView, LinkState};

    fn doc() -> ScratchDoc {
        let v = IdeView {
            repo: "vrcCS".into(),
            node_id: "#m:Foo.Bar".into(),
            title: "Foo.Bar()".into(),
            kind: "method".into(),
            file: "Foo.cs".into(),
            line: 3,
            end_line: 9,
            source: "void Bar() { Go(); }".into(),
            ret: Some("void".into()),
            from_source_pane: true,
        };
        match idelink_scratch::build(&v) {
            LinkState::Ready(d) => d,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn the_context_follows_the_document() {
        let ctx = IdeContext::from_doc(&doc());
        assert_eq!(ctx.surface, Surface::SourcePane);
        assert_eq!(ctx.kind, "method");
        assert_eq!(ctx.blocks, 2);
        assert_eq!(ctx.breadcrumb(), "vrcCS  ▸  Foo.cs:3-9  ▸  Foo.Bar()");
    }

    #[test]
    fn a_graph_fallback_says_so() {
        let mut d = doc();
        d.from_source_pane = false;
        assert_eq!(IdeContext::from_doc(&d).surface, Surface::GraphSelection);
    }

    #[test]
    fn breadcrumb_never_has_dangling_separators() {
        let mut ctx = IdeContext::from_doc(&doc());
        ctx.repo.clear();
        assert_eq!(ctx.breadcrumb(), "Foo.cs:3-9  ▸  Foo.Bar()");
        ctx.file.clear();
        assert_eq!(ctx.breadcrumb(), "Foo.Bar()");
        ctx.symbol.clear();
        assert_eq!(ctx.breadcrumb(), "");
    }

    #[test]
    fn nothing_is_named_as_nothing() {
        let ctx = IdeContext::nothing("vrcCS");
        assert_eq!(ctx.surface, Surface::Nothing);
        assert_eq!(ctx.status(), "nothing · 0 blocks");
        assert_eq!(ctx.breadcrumb(), "vrcCS");
    }
}

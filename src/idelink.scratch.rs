//! `idelink.scratch.rs` — the language link: what the IDE is viewing → blocks.
//!
//! This is the only place that knows how a host's *viewed code* becomes a
//! [`ScratchDoc`], and the only place that defines the data the host and the
//! Scratch pane exchange. It is pure data + one pure function: no `egui`, no
//! engine, no I/O, so the mapping is tested without a window.
//!
//! # The contract
//!
//! ```text
//! host                          this crate                 gui.scratch.rs
//! ────                          ──────────                 ──────────────
//! GraphView selection   ──▶  IdeView  ──▶ build() ──▶ ScratchDoc ──▶ blocks
//! Source pane document  ──▶                                   │
//!                                                             ▼
//! host action           ◀──  Request (Reveal / OpenNode) ◀── click
//! ```
//!
//! `IdeView` is *what the user is looking at*, not a query: the host fills it
//! from whichever surface owns the current code (the source pane's document
//! first, the graph selection as the fallback) and the pane can only ever show
//! that.
//!
//! # Why `EditIntent` exists but does nothing
//!
//! Editing is the goal, not this milestone. [`EditIntent`] is the shape a later
//! write-back will carry — which block, what text — so the GUI and the host can
//! agree on it now, without this crate pretending it applies edits. Nothing
//! reads it yet; it is data.

use crate::scratch::{self, Block, Parsed, Signature};

/// What the IDE is currently viewing.
///
/// The host owns this; the crate never invents one. An `IdeView` with an empty
/// `source` is meaningful ("nothing to show"), which is why [`build`] answers
/// [`LinkState::Empty`] rather than a document with no blocks.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IdeView {
    /// Repository/corpus root, for the IDE context strip.
    pub repo: String,
    /// The graph node id, when the view came from a node (empty otherwise).
    pub node_id: String,
    /// The declaration's display title (`Foo.Bar(int)` or the document title).
    pub title: String,
    /// The host's node kind vocabulary (`method`, `class`, `field`, …), used for
    /// the header colour only. Never re-interpreted here.
    pub kind: String,
    /// Declaration file, repo-relative.
    pub file: String,
    /// 1-based declaration line.
    pub line: i64,
    /// 1-based last declaration line, `0` when the host does not know it.
    pub end_line: i64,
    /// The declaration text itself.
    pub source: String,
    /// The declaration's declared type (a method's return type), when known.
    pub ret: Option<String>,
    /// True when the host took this from the source pane's loaded document
    /// rather than from the graph selection. Shown verbatim in the strip.
    pub from_source_pane: bool,
}

impl IdeView {
    /// `file:line` or `file:line-end_line` — the subtitle every surface shows.
    pub fn location(&self) -> String {
        if self.file.is_empty() {
            return String::new();
        }
        if self.end_line > self.line {
            format!("{}:{}-{}", self.file, self.line, self.end_line)
        } else if self.line > 0 {
            format!("{}:{}", self.file, self.line)
        } else {
            self.file.clone()
        }
    }
}

/// A ready-to-paint block document.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScratchDoc {
    /// Copied from the view so the painter needs one object, not two.
    pub title: String,
    pub kind: String,
    pub location: String,
    pub repo: String,
    /// True when the IDE's source pane supplied the text.
    pub from_source_pane: bool,
    pub node_id: String,
    /// 1-based declaration line, for `Reveal`.
    pub line: i64,
    pub blocks: Vec<Block>,
    /// Real parse defects, shown in the footer. Empty is normal.
    pub warnings: Vec<String>,
}

impl ScratchDoc {
    /// Blocks in the document, all depths.
    pub fn count(&self) -> usize {
        self.blocks.iter().map(Block::count).sum()
    }

    /// The block at `path` (`[2, 0]` = first child of the third root block).
    pub fn block_at(&self, path: &[usize]) -> Option<&Block> {
        block_at(&self.blocks, path)
    }

    /// The 1-based source line a block starts on.
    pub fn line_of(&self, path: &[usize]) -> Option<usize> {
        self.block_at(path).map(|b| b.line)
    }
}

/// Resolve a path of child indices against a block forest.
pub fn block_at<'a>(blocks: &'a [Block], path: &[usize]) -> Option<&'a Block> {
    let (&first, rest) = path.split_first()?;
    let block = blocks.get(first)?;
    if rest.is_empty() {
        Some(block)
    } else {
        block_at(&block.children, rest)
    }
}

/// The answer to "show me this C# as blocks".
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkState {
    /// Nothing is being viewed (no selection, no document). The honest default.
    Empty,
    /// A document with blocks.
    Ready(ScratchDoc),
    /// The host had a view but the text could not be turned into one.
    Error(String),
}

/// Turn the viewed code into blocks.
///
/// The only failure it reports is an empty source: everything else is a real
/// (possibly empty) tree plus [`ScratchDoc::warnings`], because a declaration
/// with no recognisable body is still a declaration.
pub fn build(view: &IdeView) -> LinkState {
    if view.source.trim().is_empty() {
        return LinkState::Empty;
    }
    if view.node_id.is_empty() && view.title.is_empty() && view.file.is_empty() {
        return LinkState::Error("the view names no node, title or file".to_string());
    }
    let sig = Signature { ret: view.ret.clone() };
    let Parsed { blocks, warnings } = scratch::parse(&view.source, &sig);
    if blocks.is_empty() {
        return LinkState::Error("the viewed text produced no blocks".to_string());
    }
    LinkState::Ready(ScratchDoc {
        title: if view.title.is_empty() { view.node_id.clone() } else { view.title.clone() },
        kind: view.kind.clone(),
        location: view.location(),
        repo: view.repo.clone(),
        from_source_pane: view.from_source_pane,
        node_id: view.node_id.clone(),
        line: view.line,
        blocks,
        warnings,
    })
}

/// A request the Scratch GUI sends back to the host.
///
/// The GUI never acts: it returns these and the host decides. `Reveal` is the
/// only one the read-only milestone emits; `OpenNode` exists for a port that
/// names a symbol, and is not emitted yet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    /// Show this file and line in the IDE's source pane.
    Reveal { file: String, line: i64 },
    /// Select/open a graph node.
    OpenNode(String),
}

/// The shape of a future edit — **data only, nothing applies it**.
///
/// Recording it now means the host and the GUI agree on "which block, what
/// text" before either side can write anything. `block_path` is the same child
/// path [`ScratchDoc::block_at`] resolves.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditIntent {
    pub block_path: Vec<usize>,
    pub replacement: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(src: &str) -> IdeView {
        IdeView {
            repo: "/repo".into(),
            node_id: "#m:Foo.Bar".into(),
            title: "Foo.Bar(int)".into(),
            kind: "method".into(),
            file: "Foo.cs".into(),
            line: 10,
            end_line: 24,
            source: src.into(),
            ret: Some("void".into()),
            from_source_pane: true,
        }
    }

    #[test]
    fn empty_source_is_the_empty_state_not_a_document() {
        assert_eq!(build(&view("   \n")), LinkState::Empty);
    }

    #[test]
    fn a_view_without_identity_is_an_error_not_a_blank_doc() {
        let v = IdeView { source: "void A() { }".into(), ..IdeView::default() };
        assert!(matches!(build(&v), LinkState::Error(_)));
    }

    #[test]
    fn a_real_view_becomes_a_document_with_ports() {
        let LinkState::Ready(doc) = build(&view("void Bar(int n) { Use(n); }")) else {
            panic!("expected a ready document");
        };
        assert_eq!(doc.title, "Foo.Bar(int)");
        assert_eq!(doc.location, "Foo.cs:10-24");
        assert_eq!(doc.kind, "method");
        assert!(doc.from_source_pane);
        let m = &doc.blocks[0];
        assert_eq!(m.head, "void Bar");
        assert_eq!(m.ports[0].label(), "n: int");
        assert_eq!(doc.count(), 2, "the method and its body statement");
    }

    #[test]
    fn the_declared_return_type_types_the_return_block() {
        let mut v = view("int F() { return 1; }");
        v.ret = Some("int".into());
        let LinkState::Ready(doc) = build(&v) else { panic!() };
        let ret = &doc.blocks[0].children[0];
        assert_eq!(ret.head, "return");
        assert_eq!(ret.ports[0].ty.as_deref(), Some("int"));
    }

    #[test]
    fn block_paths_resolve_and_report_lines() {
        let LinkState::Ready(doc) = build(&view("void A() {\n  if (x) {\n    Go();\n  }\n}")) else {
            panic!()
        };
        assert_eq!(doc.block_at(&[0]).unwrap().head, "void A");
        assert_eq!(doc.block_at(&[0, 0]).unwrap().head, "if");
        assert_eq!(doc.block_at(&[0, 0, 0]).unwrap().head, "Go()");
        assert_eq!(doc.block_at(&[9]), None);
        assert_eq!(doc.line_of(&[0, 0]), Some(2));
    }

    #[test]
    fn parse_defects_are_surfaced_not_hidden() {
        let LinkState::Ready(doc) = build(&view("void A() {")) else { panic!() };
        assert!(!doc.warnings.is_empty());
    }

    #[test]
    fn a_bodyless_declaration_is_still_a_document() {
        let LinkState::Ready(doc) = build(&view("public int Count => 3;")) else { panic!() };
        assert_eq!(doc.count(), 1);
        assert_eq!(doc.blocks[0].ports[0].name, "Count");
    }

    #[test]
    fn location_degrades_honestly() {
        let mut v = view("x");
        v.end_line = 0;
        assert_eq!(v.location(), "Foo.cs:10");
        v.line = 0;
        assert_eq!(v.location(), "Foo.cs");
        v.file.clear();
        assert_eq!(v.location(), "");
    }
}

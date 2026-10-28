//! `scratch_rs_ide_addon` — Scratch-style puzzle-piece blocks for a C# IDE.
//!
//! The crate is split by responsibility, and the dotted filenames are the real
//! filenames (`#[path]`), not aliases:
//!
//! | module | file | responsibility |
//! |---|---|---|
//! | [`scratch`] | `scratch.rs` | block model + C# statement splitter (pure) |
//! | `idelink_scratch` | `idelink.scratch.rs` | viewed code → block document; edit intents |
//! | `gui_scratch` | `gui.scratch.rs` | the puzzle-piece painter |
//! | `idelink_nzkide` | `idelink.nzkide.rs` | IDE context (repo/file/symbol) |
//! | `gui_nzkide` | `gui.nzkide.rs` | the IDE context strip |
//!
//! Nothing here depends on the host's graph engine: the host builds an
//! `idelink_scratch::IdeView` and the crate returns blocks and requests.

#[path = "scratch.rs"]
pub mod scratch;

#[path = "idelink.scratch.rs"]
pub mod idelink_scratch;

#[path = "gui.scratch.rs"]
pub mod gui_scratch;

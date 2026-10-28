# scratch_rs_ide_addon

Scratch-style **puzzle-piece blocks** for a C# IDE, in Rust, rendered with `egui`.
Built for **cs viewer v3** (`cs viewer builder/native-csviewer`), which takes this
crate as a path dependency and opens it with **Ctrl+Alt+S**.

Read-only for now: the window shows whatever C# the IDE is currently viewing as
nested blocks. Editing is designed for but not implemented — see `EditIntent`
in `idelink.scratch.rs`.

## Files

| File | Role |
|---|---|
| `src/scratch.rs` | The block model and the C# statement splitter (pure, no egui). |
| `src/idelink.scratch.rs` | Link *into* the language: viewed code → block document, plus the request/intent types that will carry edits back. |
| `src/gui.scratch.rs` | All Scratch GUI: puzzle-piece painting, ports, selection. |
| `src/idelink.nzkide.rs` | Link to the IDE window's context (repo, file, symbol, source size). |
| `src/gui.nzkide.rs` | The IDE window's context strip (breadcrumb + read-only badge). |

`src/lib.rs` declares them with `#[path]` so the dotted filenames are the real
names, not aliases.

## What a block is

A `Block` is one C# construct: a header (`if`, `class Foo`, `void Bar`) with a
typed `body` remainder, typed `ports`, and nested `children` for its braces.
Types are `Option<String>`: `None` means *unknown*, and is drawn `?` — never
guessed.

## Tests

```sh
cargo test
```

Everything except the GUI painter is display-free; `gui.scratch.rs` has one
headless `egui::Context` smoke test so a panic in the painter is a test failure,
not a runtime surprise.

## Milestones

Each commit on `main` is a milestone (model → link → GUI → IDE chrome).

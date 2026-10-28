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

## Wiring into cs viewer v3

The host (the `native-csviewer` crate) takes this crate by path and does three
things; nothing else in the host knows about Scratch:

1. `app/src/nzk_scratch.rs` fills an `IdeView` from the IDE's real state (source
   pane document first, graph selection as the fallback, both through the same
   engine `doc_source` the pane uses).
2. `app/src/app.rs` owns the window (`Role::Scratch`) and the chord
   (`Ctrl+Alt+S`), paints `gui_scratch::ui` / `empty` / `error` in the window,
   and maps the returned `Request::OpenNode` to a selection.
3. `app/Cargo.toml` declares the path dependency.

The block document is rebuilt from the current view each pass, so switching
declarations in the IDE switches the pieces; nothing is cached across views.

## Milestones

| commit | milestone |
|---|---|
| 1 | block model + C# statement splitter |
| 2 | `IdeView` → `ScratchDoc` link, requests, edit-intent data |
| 3 | puzzle-piece painter (notch/tab, ports, click) |
| 4 | IDE context link + context strip |
| 5 | one request per block click |
| 6 | host wiring documented |
| 7 | ports fit their piece; `[Attribute]` annotates instead of owning |
| 8 | text wraps instead of being cut off; ports pack into rows |
| 10 | kinds fixed against real files: empty strings, enums, accessors, directives, case labels |

Later milestones should start with write-back: `EditIntent` is the agreed shape,
and nothing yet applies it.

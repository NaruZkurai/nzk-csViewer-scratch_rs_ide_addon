//! Debug tool: parse a C# file and print the block tree the Scratch pane sees.
//!
//! ```sh
//! cargo run --example dump_kinds -- path/to/File.cs
//! ```

use scratch_rs_ide_addon::scratch::{parse, Block, Signature};

fn dump(block: &Block, depth: usize) {
    let ports: Vec<String> = block.ports.iter().map(|p| p.label()).collect();
    println!(
        "{}{:?}  head={:?} body={:?} attrs={:?} ports=[{}]{}",
        "  ".repeat(depth),
        block.kind,
        block.head,
        block.body,
        block.attrs,
        ports.join(", "),
        if block.closed { "" } else { "  (UNCLOSED)" }
    );
    for child in &block.children {
        dump(child, depth + 1);
    }
}

fn main() {
    let path = std::env::args().nth(1).expect("usage: dump_kinds <file.cs>");
    let src = std::fs::read_to_string(&path).expect("read");
    let parsed = parse(&src, &Signature::default());
    for block in &parsed.blocks {
        dump(block, 0);
    }
    for w in &parsed.warnings {
        println!("WARN {w}");
    }
    println!("-- {} blocks, {} warnings", parsed.count(), parsed.warnings.len());
}

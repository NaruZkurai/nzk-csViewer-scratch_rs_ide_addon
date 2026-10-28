//! `scratch.rs` — the Scratch model: a C# declaration as puzzle-piece blocks.
//!
//! # What this is
//!
//! The Scratch window shows **whatever C# the IDE is viewing** as nested
//! blocks. This module owns both halves of that: the [`Block`] tree (the data)
//! and [`parse`] (the statement splitter that builds it).
//!
//! It is deliberately **display-free** — no `egui`, no I/O, no csgraph — so the
//! whole model is unit-tested without a window. `gui.scratch.rs` paints it;
//! `idelink.scratch.rs` feeds it.
//!
//! # Honesty rules
//!
//! * A type this parser cannot determine is [`None`], drawn `?`. It is never
//!   guessed from a name.
//! * A brace that never closes is a [`Parsed::warnings`] entry and `closed:
//!   false`, not a silently repaired tree.
//! * Text is preserved verbatim except for comments, which are dropped: a block
//!   shows code, not the author's notes. Strings are opaque, so `"{"`, `"//"`
//!   and `";"` inside a literal never split a statement.
//!
//! # The shape of a block
//!
//! ```text
//! Block { kind: Method, head: "public void Bar", body: "(int n, string s)",
//!         ports: [n: int, s: string, -> void], children: [ ... ] }
//! ```
//!
//! `head` is the construct (`if`, `class Foo`, `void Bar`), `body` is the rest of
//! the line, `ports` are its typed inputs/outputs, and `children` are the blocks
//! inside its braces.

/// What a block *is*, which decides its colour and how its ports are read.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BlockKind {
    /// `namespace Foo`
    Namespace,
    /// `class` / `struct` / `interface` / `enum` / `record` / `delegate`
    Type,
    /// A signature with a parameter list: `void Bar(int n)`
    Method,
    /// `get` / `set` / `add` / `remove` / `init`
    Property,
    /// `if` / `for` / `while` / `try` / `using` / …
    Control,
    /// `return` / `throw` / `break` / `continue` / `yield`
    Keyword,
    /// Anything else that ends in `;` — a declaration, an assignment, a call.
    Simple,
}

impl BlockKind {
    /// The short label the port legend and the colour map use.
    pub fn tag(self) -> &'static str {
        match self {
            BlockKind::Namespace => "namespace",
            BlockKind::Type => "type",
            BlockKind::Method => "method",
            BlockKind::Property => "property",
            BlockKind::Control => "control",
            BlockKind::Keyword => "keyword",
            BlockKind::Simple => "simple",
        }
    }
}

/// Which way a port faces: an input the block consumes, or a value it produces.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PortDir {
    /// A value the block needs (`if`'s condition, a parameter, a local's type).
    In,
    /// A value the block produces (`return`'s type).
    Out,
}

/// One typed port on a block.
///
/// `ty` is `None` when the source does not determine the type (a `var`, a
/// `switch` scrutinee, an untyped `in` expression). Draw it `?`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Port {
    pub name: String,
    pub ty: Option<String>,
    pub dir: PortDir,
}

impl Port {
    /// The type as shown in the UI: the real type, or `?` for unknown.
    pub fn type_label(&self) -> &str {
        self.ty.as_deref().unwrap_or("?")
    }

    /// `n: int` / `-> void` — the one-line form the port pill paints.
    pub fn label(&self) -> String {
        match self.dir {
            PortDir::In => format!("{}: {}", self.name, self.type_label()),
            PortDir::Out => format!("-> {}", self.type_label()),
        }
    }
}

/// One puzzle piece.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Block {
    pub kind: BlockKind,
    /// The construct itself: `if`, `class Foo`, `public void Bar`.
    pub head: String,
    /// The remainder of the statement: `(x > 0)`, `= 5`, `(int n)`.
    pub body: String,
    pub ports: Vec<Port>,
    /// Blocks inside this block's braces, in source order.
    pub children: Vec<Block>,
    /// 1-based line within the parsed fragment where this block starts.
    pub line: usize,
    /// False when the block's `{` was never matched (an honest, reported defect).
    pub closed: bool,
}

impl Block {
    /// The full one-line text of this block, without its children.
    pub fn text(&self) -> String {
        if self.body.is_empty() {
            self.head.clone()
        } else {
            format!("{} {}", self.head, self.body)
        }
    }

    /// Total blocks in this subtree, including this one.
    pub fn count(&self) -> usize {
        1 + self.children.iter().map(Block::count).sum::<usize>()
    }
}

/// The enclosing declaration's own type information.
///
/// Only the return type is needed: it is the one fact `return` cannot know by
/// itself. Everything else is read off the source.
#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub struct Signature {
    /// The method's declared return type, when the fragment is a method.
    pub ret: Option<String>,
}

/// The result of splitting one fragment into blocks.
#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub struct Parsed {
    pub blocks: Vec<Block>,
    /// Real defects in the fragment (unbalanced braces, stray text at EOF).
    /// Empty is the normal case.
    pub warnings: Vec<String>,
}

impl Parsed {
    /// Blocks in the forest, all depths.
    pub fn count(&self) -> usize {
        self.blocks.iter().map(Block::count).sum()
    }
}

// ---------------------------------------------------------------------------
// the scanner
// ---------------------------------------------------------------------------

struct Scan {
    s: Vec<char>,
    i: usize,
    /// 1-based.
    line: usize,
}

impl Scan {
    fn new(source: &str) -> Scan {
        Scan { s: source.chars().collect(), i: 0, line: 1 }
    }

    fn peek(&self) -> Option<char> {
        self.s.get(self.i).copied()
    }

    fn at(&self, k: usize) -> Option<char> {
        self.s.get(self.i + k).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.i += 1;
        if c == '\n' {
            self.line += 1;
        }
        Some(c)
    }

    /// True when the cursor is at the start of any C# string literal form:
    /// `"…"`, `@"…"`, `$"…"`, `$@"…"`, `@$"…"`, or a raw `"""…"""`.
    fn starts_string(&self) -> bool {
        match self.peek() {
            Some('"') | Some('\'') => true,
            Some('@') => matches!(self.at(1), Some('"')) || (self.at(1) == Some('$') && self.at(2) == Some('"')),
            Some('$') => {
                matches!(self.at(1), Some('"'))
                    || (self.at(1) == Some('@') && self.at(2) == Some('"'))
                    || (self.at(1) == Some('$') && self.at(2) == Some('"'))
            }
            _ => false,
        }
    }

    /// Copy one literal into `out`, verbatim. Braces and semicolons inside it
    /// are text, never structure.
    fn scan_string(&mut self, out: &mut String) {
        // Prefixes: gather `$`/`@` runs before the opening quote.
        while matches!(self.peek(), Some('$') | Some('@')) {
            out.push(self.bump().unwrap());
        }
        let Some(open) = self.peek() else { return };
        if open == '\'' {
            out.push(self.bump().unwrap());
            while let Some(c) = self.bump() {
                out.push(c);
                if c == '\\' {
                    if let Some(n) = self.bump() {
                        out.push(n);
                    }
                } else if c == '\'' {
                    return;
                }
            }
            return;
        }
        // Count the opening quote run: 1 = normal, 3+ = raw string literal.
        let mut quotes = 0usize;
        while self.peek() == Some('"') {
            out.push(self.bump().unwrap());
            quotes += 1;
        }
        let close_run = quotes.max(1);
        let verbatim = out.trim_start_matches('$').starts_with('@');
        let mut run = 0usize;
        while let Some(c) = self.bump() {
            out.push(c);
            if c == '"' {
                run += 1;
                if run >= close_run {
                    // `""` inside a verbatim string is an escaped quote, not the end.
                    if verbatim && close_run == 1 && self.peek() == Some('"') {
                        out.push(self.bump().unwrap());
                        run = 0;
                        continue;
                    }
                    return;
                }
            } else {
                run = 0;
                if c == '\\' && !verbatim {
                    if let Some(n) = self.bump() {
                        out.push(n);
                    }
                }
            }
        }
    }

    /// Skip whitespace and comments, appending whitespace (never comments) to
    /// `out` so token boundaries survive.
    fn skip_trivia(&mut self, out: &mut String) {
        loop {
            match self.peek() {
                Some(c) if c.is_whitespace() => {
                    out.push(self.bump().unwrap());
                }
                Some('/') if self.at(1) == Some('/') => {
                    while let Some(c) = self.bump() {
                        if c == '\n' {
                            out.push('\n');
                            break;
                        }
                    }
                }
                Some('/') if self.at(1) == Some('*') => {
                    self.bump();
                    self.bump();
                    while let Some(c) = self.bump() {
                        if c == '*' && self.peek() == Some('/') {
                            self.bump();
                            break;
                        }
                        if c == '\n' {
                            out.push('\n');
                        }
                    }
                }
                _ => return,
            }
        }
    }

    fn skip_mid_statement_comment(&mut self, out: &mut String) {
        if self.at(1) == Some('/') {
            while let Some(c) = self.bump() {
                if c == '\n' {
                    out.push('\n');
                    return;
                }
            }
        } else {
            self.bump();
            self.bump();
            while let Some(c) = self.bump() {
                if c == '*' && self.peek() == Some('/') {
                    self.bump();
                    return;
                }
                if c == '\n' {
                    out.push('\n');
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// parsing
// ---------------------------------------------------------------------------

/// Split `source` into puzzle-piece blocks.
///
/// `sig` supplies the enclosing declaration's return type (used only to type a
/// `return` block's output port); pass [`Signature::default`] when unknown.
pub fn parse(source: &str, sig: &Signature) -> Parsed {
    let mut scan = Scan::new(source);
    let mut out = Parsed::default();
    if source.trim().is_empty() {
        return out;
    }
    let closed = scan.seq(&mut out.blocks, &mut out.warnings, sig, true);
    if !closed && !out.blocks.is_empty() {
        // Top level ending without a brace is normal; `seq` returns false at EOF.
    }
    out
}

impl Scan {
    /// Parse statements until EOF (`top`) or a closing `}`.
    ///
    /// Returns true when it consumed a matching `}`, false at EOF or a stray
    /// `}` at top level.
    fn seq(
        &mut self,
        out: &mut Vec<Block>,
        warnings: &mut Vec<String>,
        sig: &Signature,
        top: bool,
    ) -> bool {
        loop {
            let mut text = String::new();
            self.skip_trivia(&mut text);
            if self.peek().is_none() {
                self.push_leaf(out, sig, &text);
                if !text.trim().is_empty() && top {
                    warnings.push(format!(
                        "line {}: text after the last statement has no `;` or `{{`",
                        self.line
                    ));
                }
                return false;
            }
            if self.peek() == Some('}') {
                self.bump();
                if top {
                    warnings.push(format!("line {}: unmatched `}}`", self.line));
                    continue;
                }
                self.push_leaf(out, sig, &text);
                return true;
            }

            let start_line = self.line;
            let mut paren = 0i32;
            let mut brack = 0i32;
            loop {
                let Some(c) = self.peek() else {
                    self.push_leaf(out, sig, &text);
                    return false;
                };
                match c {
                    _ if self.starts_string() => self.scan_string(&mut text),
                    '(' => {
                        paren += 1;
                        text.push(self.bump().unwrap());
                    }
                    ')' => {
                        paren -= 1;
                        text.push(self.bump().unwrap());
                    }
                    '[' => {
                        brack += 1;
                        text.push(self.bump().unwrap());
                    }
                    ']' => {
                        brack -= 1;
                        text.push(self.bump().unwrap());
                    }
                    '{' if paren <= 0 && brack <= 0 => {
                        self.bump();
                        let header = text.trim().to_string();
                        let mut children = Vec::new();
                        let closed = self.seq(&mut children, warnings, sig, false);
                        if !closed {
                            warnings.push(format!(
                                "line {start_line}: `{{` opened here is never closed"
                            ));
                        }
                        out.push(make_block(&header, children, closed, start_line, sig));
                        break;
                    }
                    '}' if paren <= 0 && brack <= 0 => {
                        self.push_leaf(out, sig, &text);
                        self.bump();
                        if top {
                            warnings.push(format!("line {}: unmatched `}}`", self.line));
                            break;
                        }
                        return true;
                    }
                    ';' if paren <= 0 && brack <= 0 => {
                        self.bump();
                        self.push_leaf(out, sig, &text);
                        break;
                    }
                    '/' if matches!(self.at(1), Some('/') | Some('*')) => {
                        self.skip_mid_statement_comment(&mut text)
                    }
                    _ => text.push(self.bump().unwrap()),
                }
            }
        }
    }

    fn push_leaf(&self, out: &mut Vec<Block>, sig: &Signature, text: &str) {
        let t = text.trim();
        if t.is_empty() {
            return;
        }
        out.push(make_block(t, Vec::new(), true, self.line, sig));
    }
}

/// Build one block from its header text (and already-parsed children).
fn make_block(header: &str, children: Vec<Block>, closed: bool, line: usize, sig: &Signature) -> Block {
    let (kind, head, body) = classify(header, !children.is_empty());
    let ports = ports_for(kind, &head, &body, sig);
    Block { kind, head, body, ports, children, line, closed }
}

/// Split a header into its construct, the rest, and what kind of block it is.
fn classify(header: &str, has_children: bool) -> (BlockKind, String, String) {
    let t = header.trim();
    if t.is_empty() {
        return (BlockKind::Simple, "{}".to_string(), String::new());
    }
    let word = leading_word(t);
    let rest = t[word.len()..].trim().to_string();
    let kind = match word.as_str() {
        "namespace" => BlockKind::Namespace,
        "class" | "struct" | "interface" | "enum" | "record" | "delegate" => BlockKind::Type,
        "if" | "else" | "for" | "foreach" | "while" | "do" | "switch" | "try" | "catch"
        | "finally" | "using" | "lock" | "fixed" | "unsafe" | "checked" | "unchecked" => {
            BlockKind::Control
        }
        "get" | "set" | "add" | "remove" | "init" => BlockKind::Property,
        "return" | "throw" | "break" | "continue" | "yield" | "goto" => BlockKind::Keyword,
        _ => {
            if let Some(p) = first_top_paren(t) {
                // A parameter list only makes this a *method* when it is a
                // declaration: `void Bar(int n)` or a constructor with a body.
                // `Call()` and `var x = F()` are statements, and treating them
                // as signatures would invent a return type out of the callee.
                let assigned_before = split_top_assign(t)
                    .map(|(left, _)| left.len() < p)
                    .unwrap_or(false);
                let head = t[..p].trim().to_string();
                if !assigned_before && (has_children || looks_like_signature(&head)) {
                    let body = t[p..].trim().to_string();
                    return (BlockKind::Method, head, body);
                }
            }
            return (BlockKind::Simple, t.to_string(), String::new());
        }
    };
    (kind, word, rest)
}

fn leading_word(t: &str) -> String {
    t.chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect::<String>()
}

/// Index of the first `(` that is not inside a string or a nested group.
fn first_top_paren(t: &str) -> Option<usize> {
    let chars: Vec<char> = t.chars().collect();
    let mut i = 0usize;
    let mut in_str: Option<char> = None;
    while i < chars.len() {
        let c = chars[i];
        if let Some(q) = in_str {
            if c == '\\' && q != '\'' {
                i += 2;
                continue;
            }
            if c == q {
                in_str = None;
            }
            i += 1;
            continue;
        }
        match c {
            '"' | '\'' => in_str = Some(c),
            '(' => return Some(char_index_to_byte(t, i)),
            '<' => {}
            _ => {}
        }
        i += 1;
    }
    None
}

fn char_index_to_byte(s: &str, char_index: usize) -> usize {
    s.char_indices().nth(char_index).map(|(b, _)| b).unwrap_or(s.len())
}

// ---------------------------------------------------------------------------
// ports
// ---------------------------------------------------------------------------

fn ports_for(kind: BlockKind, head: &str, body: &str, sig: &Signature) -> Vec<Port> {
    match kind {
        BlockKind::Method => {
            let mut ports = params_in(body);
            if let Some(ret) = method_return(head) {
                ports.push(Port { name: "returns".into(), ty: Some(ret), dir: PortDir::Out });
            }
            ports
        }
        BlockKind::Control => control_ports(head, body),
        BlockKind::Keyword => {
            let keyword = leading_word(head);
            if keyword == "return" && !body.trim().is_empty() {
                vec![Port { name: "returns".into(), ty: sig.ret.clone(), dir: PortDir::Out }]
            } else {
                Vec::new()
            }
        }
        BlockKind::Simple | BlockKind::Property => simple_ports(&format!("{head} {body}")),
        BlockKind::Type | BlockKind::Namespace => Vec::new(),
    }
}

fn control_ports(head: &str, body: &str) -> Vec<Port> {
    let keyword = leading_word(head);
    let inner = inner_parens(body);
    match keyword.as_str() {
        "else" if body.trim_start().starts_with("if") => vec![Port {
            name: "condition".into(),
            ty: Some("bool".into()),
            dir: PortDir::In,
        }],
        "if" | "while" => vec![Port {
            name: "condition".into(),
            ty: Some("bool".into()),
            dir: PortDir::In,
        }],
        "switch" => vec![Port { name: "value".into(), ty: None, dir: PortDir::In }],
        "for" => {
            let Some(inner) = inner else { return Vec::new() };
            let parts: Vec<&str> = split_top(&inner, ';');
            let mut ports = Vec::new();
            if let Some(init) = parts.first().map(|s| s.trim()).filter(|s| !s.is_empty()) {
                ports.push(Port { name: "init".into(), ty: simple_ports(init).first().and_then(|p| p.ty.clone()), dir: PortDir::In });
            }
            if let Some(cond) = parts.get(1).map(|s| s.trim()).filter(|s| !s.is_empty()) {
                if cond != ";" {
                    ports.push(Port { name: "condition".into(), ty: Some("bool".into()), dir: PortDir::In });
                }
            }
            if let Some(iter) = parts.get(2).map(|s| s.trim()).filter(|s| !s.is_empty()) {
                ports.push(Port { name: "iterate".into(), ty: None, dir: PortDir::In });
            }
            ports
        }
        "foreach" => {
            let Some(inner) = inner else { return Vec::new() };
            let Some((left, right)) = split_top_word(&inner, " in ") else { return Vec::new() };
            let mut ports = Vec::new();
            if let Some(p) = param(left.trim()) {
                ports.push(p);
            }
            if !right.trim().is_empty() {
                ports.push(Port { name: "in".into(), ty: None, dir: PortDir::In });
            }
            ports
        }
        "catch" => inner.and_then(|i| param(i.trim())).into_iter().collect(),
        _ => Vec::new(),
    }
}

/// The left-hand declaration of a simple statement, as a typed port.
///
/// `int n = 0` → `n: int`; `var x = f()` → `x: ?`; `count = 1` → `count: ?`.
fn simple_ports(text: &str) -> Vec<Port> {
    let t = text.trim().trim_end_matches(';').trim();
    if t.is_empty() {
        return Vec::new();
    }
    let (left, _rhs) = split_top_arrow(t)
        .or_else(|| split_top_assign(t))
        .unwrap_or((t, ""));
    let left = left.trim();
    if left.is_empty() {
        return Vec::new();
    }
    // A bare call/expression has no declaration on the left.
    if left.contains('(') || left.contains(')') {
        return Vec::new();
    }
    let tokens = strip_modifiers(left);
    if tokens.is_empty() {
        return Vec::new();
    }
    if tokens.len() == 1 {
        // `count = 1` — the name is known, the type is not.
        let name = tokens[0].clone();
        if !is_identifier(&name) {
            return Vec::new();
        }
        return vec![Port { name, ty: None, dir: PortDir::In }];
    }
    let name = tokens.last().cloned().unwrap_or_default();
    let ty = tokens[..tokens.len() - 1].join(" ");
    if !is_identifier(&name) {
        return Vec::new();
    }
    let ty = if ty.trim() == "var" || ty.trim().is_empty() { None } else { Some(ty.trim().to_string()) };
    vec![Port { name, ty, dir: PortDir::In }]
}

/// True when `head` (the text before a parameter list) is a declaration such as
/// `void Bar` or `List<int> Build` rather than a callee such as `Call`.
fn looks_like_signature(head: &str) -> bool {
    let tokens = strip_modifiers(head);
    tokens.len() >= 2 && tokens.last().is_some_and(|t| is_identifier(t))
}

fn method_return(head: &str) -> Option<String> {
    let tokens = strip_modifiers(head);
    if tokens.len() < 2 {
        return None;
    }
    let ty = tokens[..tokens.len() - 1].join(" ");
    if ty.is_empty() { None } else { Some(ty) }
}

fn strip_modifiers(t: &str) -> Vec<String> {
    const MODS: [&str; 20] = [
        "public", "private", "protected", "internal", "static", "virtual", "override",
        "abstract", "sealed", "async", "extern", "unsafe", "new", "partial", "readonly",
        "const", "volatile", "required", "file", "ref",
    ];
    t.split_whitespace()
        .filter(|w| !MODS.contains(&w.trim()))
        .map(|w| w.trim().to_string())
        .filter(|w| !w.is_empty())
        .collect()
}

fn is_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_alphanumeric() || c == '_')
}

/// Parse `(int n, string s)` (the body of a method block) into ports.
fn params_in(body: &str) -> Vec<Port> {
    let Some(inner) = inner_parens(body) else { return Vec::new() };
    split_top(&inner, ',')
        .into_iter()
        .filter_map(|p| param(p.trim()))
        .collect()
}

/// One parameter: `ref readonly List<int> xs = null`.
fn param(text: &str) -> Option<Port> {
    let mut t = text.trim();
    if t.is_empty() {
        return None;
    }
    loop {
        let mut stripped = false;
        for m in ["this ", "ref ", "out ", "in ", "params ", "scoped ", "readonly "] {
            if let Some(rest) = t.strip_prefix(m) {
                t = rest.trim_start();
                stripped = true;
            }
        }
        if !stripped {
            break;
        }
    }
    if let Some((left, _)) = split_top_once(t, '=') {
        t = left.trim();
    }
    let name_start = t
        .char_indices()
        .rev()
        .find(|(_, c)| !(c.is_alphanumeric() || *c == '_'))
        .map(|(b, c)| b + c.len_utf8())
        .unwrap_or(0);
    let name = t[name_start..].trim();
    if !is_identifier(name) {
        return None;
    }
    let ty = t[..name_start].trim();
    let ty = if ty.is_empty() || ty == "var" { None } else { Some(ty.to_string()) };
    Some(Port { name: name.to_string(), ty, dir: PortDir::In })
}

/// The text inside the first balanced `( … )` in `s`.
fn inner_parens(s: &str) -> Option<String> {
    let chars: Vec<char> = s.chars().collect();
    let open = chars.iter().position(|c| *c == '(')?;
    let mut depth = 0i32;
    let mut in_str: Option<char> = None;
    let mut out = String::new();
    let mut i = open;
    while i < chars.len() {
        let c = chars[i];
        if let Some(q) = in_str {
            if c == '\\' && q != '\'' {
                i += 2;
                continue;
            }
            if c == q {
                in_str = None;
            }
            i += 1;
            continue;
        }
        match c {
            '"' | '\'' => {
                in_str = Some(c);
                if depth >= 1 {
                    out.push(c);
                }
            }
            '(' => {
                depth += 1;
                if depth > 1 {
                    out.push(c);
                }
            }
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(out);
                }
                out.push(c);
            }
            _ => {
                if depth >= 1 {
                    out.push(c);
                }
            }
        }
        i += 1;
    }
    None
}

/// Split on `sep` at bracket/angle/string depth zero.
pub fn split_top(s: &str, sep: char) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    let mut in_str: Option<char> = None;
    let mut prev = '\0';
    for (i, c) in s.char_indices() {
        if let Some(q) = in_str {
            if c == '\\' && q != '\'' {
                continue;
            }
            if c == q {
                in_str = None;
            }
            prev = c;
            continue;
        }
        match c {
            '"' | '\'' => in_str = Some(c),
            '(' | '[' | '{' | '<' => depth += 1,
            ')' | ']' | '}' | '>' => depth -= 1,
            c if c == sep && depth <= 0 => {
                out.push(&s[start..i]);
                start = i + c.len_utf8();
            }
            _ => {}
        }
        prev = c;
    }
    let _ = prev;
    out.push(&s[start..]);
    out
}

fn split_top_once(s: &str, sep: char) -> Option<(&str, &str)> {
    let parts = split_top(s, sep);
    if parts.len() < 2 {
        return None;
    }
    let first = parts[0];
    Some((first, &s[first.len() + sep.len_utf8()..]))
}

fn split_top_word<'a>(s: &'a str, word: &str) -> Option<(&'a str, &'a str)> {
    let mut depth = 0i32;
    let bytes = s.as_bytes();
    let mut i = 0usize;
    while i + word.len() <= s.len() {
        let c = bytes[i] as char;
        match c {
            '(' | '[' | '{' | '<' => depth += 1,
            ')' | ']' | '}' | '>' => depth -= 1,
            _ => {}
        }
        if depth == 0 && s[i..].starts_with(word) {
            return Some((&s[..i], &s[i + word.len()..]));
        }
        i += 1;
    }
    None
}

/// Split an expression-bodied member at its top-level `=>` (`int Count => count`).
fn split_top_arrow(s: &str) -> Option<(&str, &str)> {
    let bytes = s.as_bytes();
    let mut depth = 0i32;
    let mut i = 0usize;
    while i + 1 < s.len() {
        let c = bytes[i] as char;
        match c {
            '(' | '[' | '{' | '<' => depth += 1,
            ')' | ']' | '}' | '>' => depth -= 1,
            '=' if depth == 0 && bytes[i + 1] == b'>' => {
                return Some((&s[..i], &s[i + 2..]));
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Split a statement at its top-level assignment (`=`, not `==`, `=>`, `<=`, …).
fn split_top_assign(s: &str) -> Option<(&str, &str)> {
    let chars: Vec<char> = s.chars().collect();
    let mut depth = 0i32;
    let mut in_str: Option<char> = None;
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if let Some(q) = in_str {
            if c == '\\' && q != '\'' {
                i += 2;
                continue;
            }
            if c == q {
                in_str = None;
            }
            i += 1;
            continue;
        }
        match c {
            '"' | '\'' => in_str = Some(c),
            '(' | '[' | '{' | '<' => depth += 1,
            ')' | ']' | '}' | '>' => depth -= 1,
            '=' if depth == 0 => {
                let next = chars.get(i + 1).copied();
                let prev = if i == 0 { '\0' } else { chars[i - 1] };
                if next != Some('=') && next != Some('>') && !matches!(prev, '=' | '!' | '<' | '>') {
                    let b = char_index_to_byte(s, i);
                    return Some((&s[..b], &s[b + 1..]));
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sig(ret: &str) -> Signature {
        Signature { ret: Some(ret.to_string()) }
    }

    fn find<'a>(blocks: &'a [Block], head: &str) -> Option<&'a Block> {
        blocks.iter().find(|b| b.head == head)
    }

    #[test]
    fn empty_source_is_empty_not_a_fake_block() {
        let p = parse("   \n\t\n", &Signature::default());
        assert!(p.blocks.is_empty());
        assert!(p.warnings.is_empty());
        assert_eq!(p.count(), 0);
    }

    #[test]
    fn method_ports_are_typed() {
        let p = parse(
            "public void Bar(int n, string s, ref List<int> xs) { }",
            &Signature::default(),
        );
        let m = find(&p.blocks, "public void Bar").expect("method block");
        assert_eq!(m.kind, BlockKind::Method);
        let labels: Vec<String> = m.ports.iter().map(Port::label).collect();
        assert_eq!(
            labels,
            vec!["n: int", "s: string", "xs: List<int>", "-> void"],
            "every parameter is a typed input; the return type is an output"
        );
    }

    #[test]
    fn nested_control_blocks_keep_their_children() {
        let src = "\
void Tick(int n) {
    if (n > 0) {
        for (int i = 0; i < n; i++) {
            Work(i);
        }
    } else {
        Stop();
    }
}";
        let p = parse(src, &sig("void"));
        let m = find(&p.blocks, "void Tick").expect("method");
        assert_eq!(m.children.len(), 2, "the if and its else are siblings");
        let ifb = find(&m.children, "if").expect("the if block");
        assert_eq!(ifb.head, "if");
        assert_eq!(ifb.ports[0].name, "condition");
        assert_eq!(ifb.ports[0].ty.as_deref(), Some("bool"));
        assert_eq!(ifb.children.len(), 1, "the for lives inside the if");
        let forb = &ifb.children[0];
        assert_eq!(forb.head, "for");
        assert!(forb.children.iter().any(|b| b.head == "Work(i)"));
        let elseb = find(&m.children, "else").expect("else block");
        assert!(elseb.children.iter().any(|b| b.head == "Stop()"));
        assert!(p.count() >= 6, "method, if, for, call, else, stop");
    }

    #[test]
    fn braces_and_semicolons_in_strings_never_split() {
        let src = r#"string s = "a;b{c}"; // trailing
void Next() { }"#;
        let p = parse(src, &Signature::default());
        assert_eq!(p.blocks.len(), 2, "the string and the comment do not end a statement");
        assert_eq!(p.blocks[0].head, r#"string s = "a;b{c}""#);
        assert_eq!(p.blocks[1].head, "void Next");
    }

    #[test]
    fn comments_are_dropped_not_shown() {
        let src = "void A() { /* note */ Call(); // tail\n}";
        let p = parse(src, &Signature::default());
        let a = find(&p.blocks, "void A").expect("method");
        assert_eq!(a.children.len(), 1);
        assert_eq!(a.children[0].head, "Call()");
        assert!(!a.children[0].text().contains("note"));
    }

    #[test]
    fn unterminated_block_is_reported_not_repaired() {
        let p = parse("void A() {\n  if (x) {\n", &Signature::default());
        assert!(!p.warnings.is_empty(), "an unclosed brace must be reported");
        let a = find(&p.blocks, "void A").expect("method");
        assert!(!a.closed);
    }

    #[test]
    fn class_body_children_are_types_and_members() {
        let src = "\
namespace N {
class Foo {
    int count = 0;
    public int Count => count;
    void Add(int n) { count += n; }
}
}";
        let p = parse(src, &Signature::default());
        let ns = find(&p.blocks, "namespace").expect("namespace");
        assert_eq!(ns.body, "N");
        let cls = &ns.children[0];
        assert_eq!(cls.kind, BlockKind::Type);
        assert_eq!(cls.head, "class");
        assert_eq!(cls.body, "Foo");
        assert_eq!(cls.children.len(), 3);
        assert_eq!(cls.children[0].ports[0].name, "count");
        assert_eq!(cls.children[0].ports[0].ty.as_deref(), Some("int"));
        assert_eq!(cls.children[1].ports[0].name, "Count");
        assert_eq!(cls.children[2].kind, BlockKind::Method);
    }

    #[test]
    fn return_uses_the_enclosing_declaration_type() {
        let src = "\
int F(bool b) {
    if (b) { return 1; }
    return 0;
}";
        let p = parse(src, &sig("int"));
        let f = find(&p.blocks, "int F").expect("method");
        let ret = f
            .children
            .iter()
            .chain(f.children.iter().flat_map(|b| b.children.iter()))
            .find(|b| b.head == "return")
            .expect("a return block");
        assert_eq!(ret.kind, BlockKind::Keyword);
        assert_eq!(ret.ports[0].dir, PortDir::Out);
        assert_eq!(ret.ports[0].ty.as_deref(), Some("int"));
    }

    #[test]
    fn var_keeps_its_type_unknown_rather_than_guessing() {
        let p = parse("var x = Compute();", &Signature::default());
        let b = &p.blocks[0];
        assert_eq!(b.ports[0].name, "x");
        assert_eq!(b.ports[0].ty, None, "`var` is not a type");
        assert_eq!(b.ports[0].type_label(), "?");
    }

    #[test]
    fn else_if_is_a_control_block_with_a_condition() {
        let p = parse("if (a) { } else if (b) { }", &Signature::default());
        assert_eq!(p.blocks.len(), 2);
        assert_eq!(p.blocks[1].head, "else");
        assert_eq!(p.blocks[1].ports[0].name, "condition");
    }

    #[test]
    fn generics_do_not_split_parameters() {
        let p = parse("void M(Dictionary<string, int> map, int n) { }", &Signature::default());
        let m = find(&p.blocks, "void M").expect("method");
        assert_eq!(m.ports.len(), 3, "two parameters + the return port");
        assert_eq!(m.ports[0].ty.as_deref(), Some("Dictionary<string, int>"));
        assert_eq!(m.ports[1].ty.as_deref(), Some("int"));
    }

    #[test]
    fn foreach_types_the_loop_variable() {
        let p = parse("foreach (var item in rows) { Use(item); }", &Signature::default());
        let b = &p.blocks[0];
        assert_eq!(b.head, "foreach");
        assert_eq!(b.ports[0].name, "item");
        assert_eq!(b.ports[0].ty, None);
        assert_eq!(b.ports[1].name, "in");
    }

    #[test]
    fn split_top_is_depth_aware() {
        assert_eq!(split_top("a, B<c, d>, e", ',').len(), 3);
        assert_eq!(split_top("f(x, y), z", ',').len(), 2);
    }

    #[test]
    fn unbalanced_close_is_reported() {
        let p = parse("void A() { }\n}", &Signature::default());
        assert!(p.warnings.iter().any(|w| w.contains("unmatched")));
    }
}

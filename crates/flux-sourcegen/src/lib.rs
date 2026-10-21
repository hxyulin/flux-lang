//! Generates flux-syntax's typed AST (`src/ast/generated.rs`) from its
//! grammar, `flux.ungram`.
//!
//! A grammar rule `A | B | C` whose alternatives are all nodes becomes an
//! enum; any other rule becomes a node struct with an accessor per child:
//!
//! - an unlabeled node type gives one accessor named after the type, a list
//!   when it can occur more than once;
//! - `label:Type` gives an accessor named `label`;
//! - a token gives `<name>_token()`.
//!
//! Children whose kinds overlap (`lhs:Expr` and `rhs:Expr`) cannot be found
//! by kind, so their accessors are hand-written in `src/ast/ext.rs`; the
//! generated code checks at compile time that they exist.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use ungrammar::{Grammar, Rule};

/// The flux-syntax crate.
pub fn syntax_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../flux-syntax")
        .canonicalize()
        .unwrap()
}

pub fn generated_path() -> PathBuf {
    syntax_dir().join("src/ast/generated.rs")
}

/// The generated code, formatted.
pub fn generate_ast() -> String {
    rustfmt(&generate(&load()))
}

// Model.

pub struct Model {
    pub nodes: Vec<NodeDef>,
    enums: Vec<EnumDef>,
    /// The grammar text of each rule, for doc comments.
    rule_text: BTreeMap<String, String>,
}

pub struct NodeDef {
    pub name: String,
    fields: Vec<Field>,
    /// Labeled children whose accessors are hand-written.
    manual: Vec<String>,
    /// Kind names of every child node and token the rule mentions.
    pub child_nodes: BTreeSet<String>,
    pub child_tokens: BTreeSet<String>,
}

enum Field {
    Node {
        method: String,
        ty: String,
        many: bool,
    },
    Token {
        method: String,
        kind: String,
    },
}

struct EnumDef {
    name: String,
    variants: Vec<String>,
}

impl Model {
    fn enum_def(&self, name: &str) -> Option<&EnumDef> {
        self.enums.iter().find(|e| e.name == name)
    }

    /// The node kinds a node or enum type covers.
    fn kinds(&self, ty: &str) -> BTreeSet<String> {
        match self.enum_def(ty) {
            Some(e) => e.variants.iter().flat_map(|v| self.kinds(v)).collect(),
            None => BTreeSet::from([kind_name(ty)]),
        }
    }
}

pub fn load() -> Model {
    let text = fs::read_to_string(syntax_dir().join("flux.ungram")).unwrap();
    let grammar: Grammar = text.parse().unwrap();
    let mut model = Model {
        nodes: Vec::new(),
        enums: Vec::new(),
        rule_text: rule_texts(&text),
    };
    for node in grammar.iter() {
        let data = &grammar[node];
        if let Some(variants) = enum_variants(&grammar, &data.rule) {
            model.enums.push(EnumDef {
                name: data.name.clone(),
                variants,
            });
        }
    }
    for node in grammar.iter() {
        let data = &grammar[node];
        if model.enum_def(&data.name).is_none() {
            let def = lower_node(&model, &grammar, &data.name, &data.rule);
            model.nodes.push(def);
        }
    }
    model.nodes.sort_by(|a, b| a.name.cmp(&b.name));
    model.enums.sort_by(|a, b| a.name.cmp(&b.name));
    model
}

/// `A | B | C` where every alternative is a node is an enum.
fn enum_variants(grammar: &Grammar, rule: &Rule) -> Option<Vec<String>> {
    let Rule::Alt(alternatives) = rule else {
        return None;
    };
    alternatives
        .iter()
        .map(|alt| match alt {
            Rule::Node(node) => Some(grammar[*node].name.clone()),
            _ => None,
        })
        .collect()
}

/// How many times a child of type `ty` can occur: 0, 1, or 2 for many.
fn count(grammar: &Grammar, rule: &Rule, ty: &str) -> u8 {
    match rule {
        Rule::Node(node) => u8::from(grammar[*node].name == ty),
        Rule::Token(_) | Rule::Labeled { .. } => 0,
        Rule::Seq(rules) => rules
            .iter()
            .map(|r| count(grammar, r, ty))
            .sum::<u8>()
            .min(2),
        Rule::Alt(rules) => rules
            .iter()
            .map(|r| count(grammar, r, ty))
            .max()
            .unwrap_or(0),
        Rule::Opt(rule) => count(grammar, rule, ty),
        Rule::Rep(rule) => 2 * count(grammar, rule, ty).min(1),
    }
}

/// Node types and tokens anywhere in `rule`; with `labeled`, also inside
/// labels.
fn collect(
    grammar: &Grammar,
    rule: &Rule,
    labeled: bool,
    nodes: &mut BTreeSet<String>,
    tokens: &mut BTreeSet<String>,
) {
    match rule {
        Rule::Node(node) => {
            nodes.insert(grammar[*node].name.clone());
        }
        Rule::Token(token) => {
            tokens.insert(grammar[*token].name.clone());
        }
        Rule::Labeled { rule, .. } => {
            if labeled {
                collect(grammar, rule, labeled, nodes, tokens);
            }
        }
        Rule::Seq(rules) | Rule::Alt(rules) => {
            for rule in rules {
                collect(grammar, rule, labeled, nodes, tokens);
            }
        }
        Rule::Opt(rule) | Rule::Rep(rule) => collect(grammar, rule, labeled, nodes, tokens),
    }
}

/// The parts of a rule that give accessors, in order: unlabeled nodes and
/// tokens, and labeled subrules.
enum Part<'r> {
    Node(String),
    Token(String),
    Labeled(&'r str, &'r Rule),
}

fn parts<'r>(grammar: &Grammar, rule: &'r Rule, out: &mut Vec<Part<'r>>) {
    match rule {
        Rule::Node(node) => out.push(Part::Node(grammar[*node].name.clone())),
        Rule::Token(token) => out.push(Part::Token(grammar[*token].name.clone())),
        Rule::Labeled { label, rule } => out.push(Part::Labeled(label, rule)),
        Rule::Seq(rules) | Rule::Alt(rules) => rules.iter().for_each(|r| parts(grammar, r, out)),
        Rule::Opt(rule) | Rule::Rep(rule) => parts(grammar, rule, out),
    }
}

/// A child node accessor before overlap checking.
struct NodeField {
    method: String,
    ty: String,
    many: bool,
    labeled: bool,
}

fn lower_node(model: &Model, grammar: &Grammar, name: &str, rule: &Rule) -> NodeDef {
    // Accessors in grammar order; nodes refer into `node_fields`.
    enum Entry {
        Node(usize),
        Token(Field),
    }
    let mut entries = Vec::new();
    let mut node_fields: Vec<NodeField> = Vec::new();
    let mut manual = Vec::new();

    let mut rule_parts = Vec::new();
    parts(grammar, rule, &mut rule_parts);
    for part in rule_parts {
        match part {
            Part::Node(ty) => {
                if node_fields.iter().any(|f| !f.labeled && f.ty == ty) {
                    continue;
                }
                let many = count(grammar, rule, &ty) == 2;
                entries.push(Entry::Node(node_fields.len()));
                node_fields.push(NodeField {
                    method: method_name(&ty, many),
                    ty,
                    many,
                    labeled: false,
                });
            }
            Part::Token(token) => {
                let (method, kind) = token_info(&token);
                let method = format!("{method}_token");
                let seen = entries.iter().any(
                    |e| matches!(e, Entry::Token(Field::Token { method: m, .. }) if *m == method),
                );
                if !seen {
                    entries.push(Entry::Token(Field::Token { method, kind }));
                }
            }
            Part::Labeled(label, inner) => {
                let (mut nodes, mut tokens) = (BTreeSet::new(), BTreeSet::new());
                collect(grammar, inner, true, &mut nodes, &mut tokens);
                if nodes.len() == 1 {
                    let ty = nodes.pop_first().unwrap();
                    let many = count(grammar, inner, &ty) == 2;
                    entries.push(Entry::Node(node_fields.len()));
                    node_fields.push(NodeField {
                        method: label.to_owned(),
                        ty,
                        many,
                        labeled: true,
                    });
                } else if nodes.is_empty() && tokens.len() == 1 {
                    let (_, kind) = token_info(tokens.first().unwrap());
                    let method = label.to_owned();
                    entries.push(Entry::Token(Field::Token { method, kind }));
                } else {
                    manual.push(label.to_owned());
                }
            }
        }
    }

    // Children whose kinds overlap cannot be told apart by kind alone.
    let mut is_manual = vec![false; node_fields.len()];
    for i in 0..node_fields.len() {
        for j in i + 1..node_fields.len() {
            let (a, b) = (&node_fields[i], &node_fields[j]);
            if model.kinds(&a.ty).is_disjoint(&model.kinds(&b.ty)) {
                continue;
            }
            assert!(
                a.labeled && b.labeled,
                "{name}: `{}` and `{}` overlap; label both",
                a.method,
                b.method
            );
            is_manual[i] = true;
            is_manual[j] = true;
        }
    }
    let mut fields = Vec::new();
    for entry in entries {
        match entry {
            Entry::Node(i) if is_manual[i] => manual.push(node_fields[i].method.clone()),
            Entry::Node(i) => {
                let f = &node_fields[i];
                fields.push(Field::Node {
                    method: f.method.clone(),
                    ty: f.ty.clone(),
                    many: f.many,
                });
            }
            Entry::Token(field) => fields.push(field),
        }
    }

    let (mut all_nodes, mut all_tokens) = (BTreeSet::new(), BTreeSet::new());
    collect(grammar, rule, true, &mut all_nodes, &mut all_tokens);
    NodeDef {
        name: name.to_owned(),
        fields,
        manual,
        child_nodes: all_nodes.iter().flat_map(|ty| model.kinds(ty)).collect(),
        child_tokens: all_tokens.iter().map(|t| token_info(t).1).collect(),
    }
}

/// `BlockExpr` to `BLOCK_EXPR`.
pub fn kind_name(ty: &str) -> String {
    snake(ty).to_uppercase()
}

fn snake(ty: &str) -> String {
    let mut out = String::new();
    for (i, c) in ty.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            out.push('_');
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}

fn method_name(ty: &str, many: bool) -> String {
    match (snake(ty).as_str(), many) {
        ("type", false) => "ty".to_owned(),
        (name, false) => name.to_owned(),
        (name, true) => format!("{name}s"),
    }
}

/// The accessor name and `SyntaxKind` of a grammar token.
fn token_info(text: &str) -> (String, String) {
    const PUNCT: &[(&str, &str)] = &[
        (";", "SEMICOLON"),
        (",", "COMMA"),
        ("(", "L_PAREN"),
        (")", "R_PAREN"),
        ("{", "L_CURLY"),
        ("}", "R_CURLY"),
        ("[", "L_BRACK"),
        ("]", "R_BRACK"),
        ("<", "L_ANGLE"),
        (">", "R_ANGLE"),
        ("@", "AT"),
        ("#", "POUND"),
        ("?", "QUESTION"),
        ("&", "AMP"),
        ("|", "PIPE"),
        ("+", "PLUS"),
        ("*", "STAR"),
        ("/", "SLASH"),
        ("^", "CARET"),
        ("%", "PERCENT"),
        ("_", "UNDERSCORE"),
        (".", "DOT"),
        (":", "COLON"),
        ("=", "EQ"),
        ("!", "BANG"),
        ("-", "MINUS"),
        ("..", "DOT2"),
        ("...", "DOT3"),
        ("..=", "DOT2EQ"),
        ("::", "COLON2"),
        ("==", "EQ2"),
        ("=>", "FAT_ARROW"),
        ("!=", "NEQ"),
        ("->", "THIN_ARROW"),
        ("<=", "LTEQ"),
        (">=", "GTEQ"),
        ("&&", "AMP2"),
        ("||", "PIPE2"),
        ("<<", "SHL"),
        (">>", "SHR"),
        ("+=", "PLUSEQ"),
        ("-=", "MINUSEQ"),
        ("*=", "STAREQ"),
        ("/=", "SLASHEQ"),
        ("%=", "PERCENTEQ"),
        ("^=", "CARETEQ"),
        ("&=", "AMPEQ"),
        ("|=", "PIPEEQ"),
        ("<<=", "SHLEQ"),
        (">>=", "SHREQ"),
    ];
    const LITERALS: &[&str] = &[
        "ident",
        "lifetime_ident",
        "int_number",
        "float_number",
        "char",
        "byte",
        "string",
        "byte_string",
        "c_string",
    ];
    if let Some((_, kind)) = PUNCT.iter().find(|(t, _)| *t == text) {
        return (kind.to_lowercase(), (*kind).to_owned());
    }
    if LITERALS.contains(&text) {
        return (text.to_owned(), text.to_uppercase());
    }
    assert!(
        text.chars().all(|c| c.is_ascii_alphabetic()),
        "unknown token {text:?}"
    );
    if text == "Self" {
        return ("self_type".to_owned(), "SELF_TYPE_KW".to_owned());
    }
    (text.to_owned(), format!("{}_KW", text.to_uppercase()))
}

/// Each rule's text, from `Name =` to the next blank line.
fn rule_texts(text: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for block in text.split("\n\n") {
        let lines: Vec<&str> = block
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect();
        if let Some(name) = lines.first().and_then(|l| l.strip_suffix(" =")) {
            out.insert(name.to_owned(), lines.join("\n"));
        }
    }
    out
}

// Code generation.

fn generate(model: &Model) -> String {
    let mut out = String::new();
    let w = &mut out;
    writeln!(
        w,
        "//! Generated by flux-sourcegen from `flux.ungram`; do not edit.\n\
         //! Run `cargo run -p flux-sourcegen` to regenerate.\n"
    )
    .unwrap();
    writeln!(
        w,
        "use super::{{AstChildren, AstNode, support}};\n\
         use crate::SyntaxKind::{{self, *}};\n\
         use crate::{{SyntaxNode, SyntaxToken}};\n"
    )
    .unwrap();

    for node in &model.nodes {
        let name = &node.name;
        write_rule_doc(w, model, name);
        writeln!(
            w,
            "#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]\n\
             pub struct {name}<'t> {{ pub(crate) syntax: SyntaxNode<'t> }}\n"
        )
        .unwrap();
        writeln!(w, "impl<'t> {name}<'t> {{").unwrap();
        for field in &node.fields {
            match field {
                Field::Node {
                    method,
                    ty,
                    many: true,
                } => writeln!(
                    w,
                    "pub fn {method}(self) -> AstChildren<'t, {ty}<'t>> {{ support::children(self.syntax) }}"
                ),
                Field::Node { method, ty, .. } => writeln!(
                    w,
                    "pub fn {method}(self) -> Option<{ty}<'t>> {{ support::child(self.syntax) }}"
                ),
                Field::Token { method, kind } => writeln!(
                    w,
                    "pub fn {method}(self) -> Option<SyntaxToken<'t>> {{ support::token(self.syntax, {kind}) }}"
                ),
            }
            .unwrap();
        }
        writeln!(w, "}}\n").unwrap();
        writeln!(
            w,
            "impl<'t> AstNode<'t> for {name}<'t> {{\n\
             fn can_cast(kind: SyntaxKind) -> bool {{ kind == {kind} }}\n\
             fn cast(syntax: SyntaxNode<'t>) -> Option<Self> {{ Self::can_cast(syntax.kind()).then_some({name} {{ syntax }}) }}\n\
             fn syntax(self) -> SyntaxNode<'t> {{ self.syntax }}\n\
             }}\n",
            kind = kind_name(name)
        )
        .unwrap();
    }

    for e in &model.enums {
        let name = &e.name;
        write_rule_doc(w, model, name);
        writeln!(
            w,
            "#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]\npub enum {name}<'t> {{"
        )
        .unwrap();
        for v in &e.variants {
            writeln!(w, "{v}({v}<'t>),").unwrap();
        }
        writeln!(w, "}}\n").unwrap();

        let (nested, plain): (Vec<&String>, Vec<&String>) =
            e.variants.iter().partition(|v| model.enum_def(v).is_some());
        let mut can_cast = plain
            .iter()
            .map(|v| kind_name(v))
            .collect::<Vec<_>>()
            .join(" | ");
        can_cast = format!("matches!(kind, {can_cast})");
        for v in &nested {
            write!(can_cast, " || {v}::can_cast(kind)").unwrap();
        }
        writeln!(
            w,
            "impl<'t> AstNode<'t> for {name}<'t> {{\n\
             fn can_cast(kind: SyntaxKind) -> bool {{ {can_cast} }}\n\
             fn cast(syntax: SyntaxNode<'t>) -> Option<Self> {{\n\
             let node = match syntax.kind() {{"
        )
        .unwrap();
        for v in &plain {
            writeln!(w, "{} => {name}::{v}({v} {{ syntax }}),", kind_name(v)).unwrap();
        }
        for v in &nested {
            writeln!(
                w,
                "kind if {v}::can_cast(kind) => {name}::{v}({v}::cast(syntax)?),"
            )
            .unwrap();
        }
        writeln!(
            w,
            "_ => return None,\n}};\nSome(node)\n}}\n\
             fn syntax(self) -> SyntaxNode<'t> {{\nmatch self {{"
        )
        .unwrap();
        for v in &e.variants {
            writeln!(w, "{name}::{v}(it) => it.syntax(),").unwrap();
        }
        writeln!(w, "}}\n}}\n}}\n").unwrap();
        for v in &e.variants {
            writeln!(
                w,
                "impl<'t> From<{v}<'t>> for {name}<'t> {{ fn from(node: {v}<'t>) -> Self {{ {name}::{v}(node) }} }}\n"
            )
            .unwrap();
        }
    }

    // A compile-time check that every hand-written accessor exists.
    writeln!(
        w,
        "/// Fails to compile when an accessor the grammar leaves to `ext.rs` is\n\
         /// missing.\n\
         #[allow(dead_code)]\n\
         mod hand_written {{\n\
         use super::*;\n"
    )
    .unwrap();
    for node in model.nodes.iter().filter(|n| !n.manual.is_empty()) {
        writeln!(w, "fn {}_(it: {}<'_>) {{", snake(&node.name), node.name).unwrap();
        for method in &node.manual {
            writeln!(w, "let _ = it.{method}();").unwrap();
        }
        writeln!(w, "}}").unwrap();
    }
    writeln!(w, "}}").unwrap();
    out
}

fn write_rule_doc(w: &mut String, model: &Model, name: &str) {
    if let Some(text) = model.rule_text.get(name) {
        writeln!(w, "/// ```text").unwrap();
        for line in text.lines() {
            writeln!(w, "/// {line}").unwrap();
        }
        writeln!(w, "/// ```").unwrap();
    }
}

fn rustfmt(code: &str) -> String {
    let mut child = Command::new("rustfmt")
        .args(["--edition", "2024"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("rustfmt is needed to generate the AST");
    std::io::Write::write_all(&mut child.stdin.take().unwrap(), code.as_bytes()).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "rustfmt failed");
    String::from_utf8(output.stdout).unwrap()
}

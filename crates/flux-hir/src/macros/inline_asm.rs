//! Rust inline assembly syntax. Expressions keep their original token origins.
use flux_intern::Symbol;
use flux_syntax::{SyntaxKind, SyntaxNode, T, TextRange, parser};

use super::tt::{self, Tok};
use crate::{AsmOptions, AsmRegister, AsmTemplatePiece};

type Error = (TextRange, String);

pub(crate) struct ParsedAsm {
    pub template: Box<[AsmTemplatePiece]>,
    pub registers: Box<[AsmRegister]>,
    pub options: AsmOptions,
    pub expressions: Vec<Vec<Tok>>,
}

pub(crate) fn parse_call(call: SyntaxNode<'_>) -> Result<ParsedAsm, Error> {
    let mut tokens = Vec::new();
    let tree = call
        .children()
        .find(|n| n.kind() == SyntaxKind::TOKEN_TREE)
        .ok_or((call.text_range(), "expected assembly arguments".into()))?;
    tt::tokens_of(tree, &mut tokens);
    parse(super::strip_delimiters(&tokens), call.text_range())
}

pub(crate) fn parse(tokens: &[Tok], range: TextRange) -> Result<ParsedAsm, Error> {
    let mut input = parser::Input::default();
    for t in tokens {
        input.push(t.kind, t.is_union_keyword());
        if t.joint {
            input.set_last_joint();
        }
    }
    let mut at = 0;
    let mut text = String::new();
    let mut names = Vec::new();
    let mut expressions = Vec::new();
    let mut registers = Vec::new();
    let mut options = AsmOptions::default();
    let mut seen_options = Vec::new();
    let mut template_count = 0;
    while at < tokens.len() && tokens[at].kind == SyntaxKind::STRING {
        if template_count != 0 {
            text.push('\n');
        }
        template_count += 1;
        text.push_str(
            &crate::literal::unescape_str(tokens[at].text.as_str())
                .map_err(|e| (tokens[at].range(), e))?,
        );
        at += 1;
        if at < tokens.len() {
            if tokens[at].kind != T![,] {
                return Err((tokens[at].range(), "expected `,`".into()));
            }
            at += 1;
        }
    }
    if at == 0 {
        return Err((range, "expected an assembly template string".into()));
    }
    let mut options_seen = false;
    while at < tokens.len() {
        let mut name = None;
        if tokens.get(at + 1).is_some_and(|t| t.kind == T![=]) {
            if tokens[at].kind != SyntaxKind::IDENT && !tokens[at].kind.is_keyword() {
                return Err((
                    tokens[at].range(),
                    "expected an assembly operand name".into(),
                ));
            }
            let spelling = tokens[at].text.as_str();
            name = Some(Symbol::intern(
                spelling.strip_prefix("r#").unwrap_or(spelling),
            ));
            if names.contains(&name) {
                return Err((tokens[at].range(), "duplicate assembly operand name".into()));
            }
            at += 2;
        }
        let token = tokens
            .get(at)
            .ok_or((range, "expected an assembly operand".into()))?;
        let kind = token.text.as_str();
        if kind == "options" && name.is_none() {
            options_seen = true;
            at += 1;
            if tokens.get(at).is_none_or(|t| t.kind != T!['(']) {
                return Err((range, "expected `(`".into()));
            }
            at += 1;
            while tokens.get(at).is_some_and(|t| t.kind != T![')']) {
                let token = tokens[at];
                if seen_options.contains(&token.text) {
                    return Err((token.range(), "duplicate assembly option".into()));
                }
                seen_options.push(token.text);
                match token.text.as_str() {
                    "nomem" => options.nomem = true,
                    "readonly" => options.readonly = true,
                    "preserves_flags" => options.preserves_flags = true,
                    "nostack" => options.nostack = true,
                    "pure" => options.pure = true,
                    _ => {
                        return Err((
                            token.range(),
                            format!("assembly option `{}` is not supported yet", token.text),
                        ));
                    }
                }
                at += 1;
                if tokens.get(at).is_some_and(|t| t.kind == T![,]) {
                    at += 1;
                } else if tokens.get(at).is_none_or(|t| t.kind != T![')']) {
                    return Err((range, "expected `,`".into()));
                }
            }
            if tokens.get(at).is_none_or(|t| t.kind != T![')']) {
                return Err((range, "expected `)`".into()));
            }
            at += 1;
        } else {
            if options_seen {
                return Err((
                    tokens[at].range(),
                    "assembly operands cannot follow options".into(),
                ));
            }
            if !matches!(kind, "inout" | "inlateout" | "in" | "lateout" | "const") {
                return Err((
                    tokens[at].range(),
                    format!("assembly operand `{kind}` is not supported yet"),
                ));
            }
            at += 1;
            let register = if kind == "const" {
                AsmRegister::Const
            } else {
                if tokens.get(at).is_none_or(|t| t.kind != T!['(']) {
                    return Err((range, "expected `(`".into()));
                }
                at += 1;
                let register = match tokens.get(at).map(|t| t.text.as_str()) {
                    Some("reg") => AsmRegister::Reg,
                    Some("vreg") => AsmRegister::Vreg,
                    _ => {
                        return Err((range, "assembly register class is not supported yet".into()));
                    }
                };
                at += 1;
                if tokens.get(at).is_none_or(|t| t.kind != T![')']) {
                    return Err((range, "expected `)`".into()));
                }
                at += 1;
                match (kind, register) {
                    ("in", AsmRegister::Reg) => AsmRegister::InputReg,
                    ("in", AsmRegister::Vreg) => AsmRegister::InputVreg,
                    ("lateout", AsmRegister::Reg) => AsmRegister::OutputReg,
                    ("lateout", AsmRegister::Vreg) => AsmRegister::OutputVreg,
                    _ => register,
                }
            };
            let n = parser::fragment_len(&input, at, parser::Fragment::Expr)
                .filter(|&n| n > 0)
                .ok_or((range, "expected an assembly operand expression".into()))?;
            if kind == "const" && (n != 1 || tokens[at].kind != SyntaxKind::INT_NUMBER) {
                return Err((
                    range,
                    "only integer literals are supported as assembly constants".into(),
                ));
            }
            expressions.push(tokens[at..at + n].to_vec());
            registers.push(register);
            if name.is_none() && names.iter().any(Option::is_some) {
                return Err((
                    tokens[at].range(),
                    "positional operands cannot follow named operands".into(),
                ));
            }
            names.push(name);
            at += n;
            if tokens.get(at).is_some_and(|t| t.kind == T![=]) {
                return Err((
                    tokens[at].range(),
                    "separate assembly output places are not supported yet".into(),
                ));
            }
        }
        if at < tokens.len() {
            if tokens[at].kind != T![,] {
                return Err((tokens[at].range(), "expected `,`".into()));
            }
            at += 1;
        }
    }
    if options.nomem && options.readonly {
        return Err((
            range,
            "assembly options `nomem` and `readonly` are mutually exclusive".into(),
        ));
    }
    if options.pure && !options.nomem && !options.readonly {
        return Err((
            range,
            "`pure` assembly requires `nomem` or `readonly`".into(),
        ));
    }
    if options.pure && registers.iter().all(|register| register.input_only()) {
        return Err((range, "`pure` assembly requires an output".into()));
    }
    let template = template(&text, &names, range)?;
    Ok(ParsedAsm {
        template: template.into(),
        registers: registers.into(),
        options,
        expressions,
    })
}

fn template(
    text: &str,
    names: &[Option<Symbol>],
    range: TextRange,
) -> Result<Vec<AsmTemplatePiece>, Error> {
    let mut chars = text.chars().peekable();
    let mut literal = String::new();
    let mut pieces = Vec::new();
    let mut next = 0;
    let mut used = vec![false; names.len()];
    while let Some(c) = chars.next() {
        if c != '{' && c != '}' {
            literal.push(c);
            continue;
        }
        if chars.peek() == Some(&c) {
            chars.next();
            literal.push(c);
            continue;
        }
        if c == '}' {
            return Err((range, "unmatched `}` in assembly template".into()));
        }
        pieces.push(AsmTemplatePiece::Text(Symbol::intern(&literal)));
        literal.clear();
        let mut field = String::new();
        loop {
            match chars.next() {
                Some('}') => break,
                Some(c) => field.push(c),
                None => return Err((range, "unclosed `{` in assembly template".into())),
            }
        }
        let (key, modifier) = match field.split_once(':') {
            Some((key, modifier)) if modifier.chars().count() == 1 => {
                (key, modifier.chars().next())
            }
            Some(_) => return Err((range, "expected a single assembly template modifier".into())),
            None => (field.as_str(), None),
        };
        let index = if key.is_empty() {
            let i = next;
            next += 1;
            i
        } else if let Ok(index) = key.parse::<usize>() {
            index
        } else {
            names
                .iter()
                .position(|name| name.is_some_and(|n| n.as_str() == key))
                .ok_or((range, "unknown assembly operand name".into()))?
        };
        if index >= names.len()
            || (key.is_empty() || key.parse::<usize>().is_ok()) && names[index].is_some()
        {
            return Err((range, "invalid assembly operand reference".into()));
        }
        used[index] = true;
        pieces.push(AsmTemplatePiece::Operand {
            index: index as u32,
            modifier,
        });
    }
    pieces.push(AsmTemplatePiece::Text(Symbol::intern(&literal)));
    if used.contains(&false) {
        return Err((range, "unused assembly operand".into()));
    }
    Ok(pieces)
}

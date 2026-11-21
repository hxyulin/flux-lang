//! Rust inline assembly syntax. Expressions keep their original token origins.
use flux_intern::Symbol;
use flux_syntax::{SyntaxKind, T, TextRange, parser};

use super::builtin::BuiltinCx;
use super::tt::Tok;
use crate::{AsmOptions, AsmRegister, AsmTemplatePiece};

type Error = (TextRange, String);

#[derive(Debug)]
pub(crate) struct AsmLayout {
    pub template: Box<[AsmTemplatePiece]>,
    pub registers: Box<[AsmRegister]>,
    pub options: AsmOptions,
    pub outputs: Box<[u32]>,
}

pub(crate) fn expand(cx: &BuiltinCx<'_>) -> Result<(Vec<Tok>, AsmLayout), Error> {
    let tokens = cx.toks;
    let range = cx.call;
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
    let mut outputs = Vec::new();
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
            if !matches!(
                kind,
                "inout" | "inlateout" | "in" | "out" | "lateout" | "const"
            ) {
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
                let register = if tokens
                    .get(at)
                    .is_some_and(|token| token.kind == SyntaxKind::STRING)
                {
                    let name = crate::literal::unescape_str(tokens[at].text.as_str())
                        .map_err(|message| (range, message))?;
                    let name = fixed_register(&name).ok_or((
                        range,
                        "explicit assembly register is not supported on this target yet".into(),
                    ))?;
                    if registers
                        .iter()
                        .any(|register: &AsmRegister| register.fixed() == Some(name))
                    {
                        return Err((
                            range,
                            "overlapping explicit registers are not supported yet".into(),
                        ));
                    }
                    AsmRegister::Fixed(name)
                } else {
                    match tokens.get(at).map(|t| t.text.as_str()) {
                        Some("reg") => AsmRegister::Reg,
                        Some("vreg") => AsmRegister::Vreg,
                        Some("reg_byte") => AsmRegister::RegByte,
                        Some("xmm_reg") => AsmRegister::XmmReg,
                        _ => {
                            return Err((
                                range,
                                "assembly register class is not supported yet".into(),
                            ));
                        }
                    }
                };
                at += 1;
                if tokens.get(at).is_none_or(|t| t.kind != T![')']) {
                    return Err((range, "expected `)`".into()));
                }
                at += 1;
                match (kind, register) {
                    ("in", AsmRegister::Fixed(name)) => AsmRegister::InputFixed(name),
                    ("out" | "lateout", AsmRegister::Fixed(name)) => AsmRegister::OutputFixed(name),
                    ("in", AsmRegister::RegByte) => AsmRegister::InputRegByte,
                    ("out" | "lateout", AsmRegister::RegByte) => AsmRegister::OutputRegByte,
                    ("in", AsmRegister::XmmReg) => AsmRegister::InputXmmReg,
                    ("out" | "lateout", AsmRegister::XmmReg) => AsmRegister::OutputXmmReg,
                    ("in", AsmRegister::Reg) => AsmRegister::InputReg,
                    ("in", AsmRegister::Vreg) => AsmRegister::InputVreg,
                    ("out" | "lateout", AsmRegister::Reg) => AsmRegister::OutputReg,
                    ("out" | "lateout", AsmRegister::Vreg) => AsmRegister::OutputVreg,
                    _ => register,
                }
            };
            let n = parser::fragment_len(&input, at, parser::Fragment::Expr)
                .filter(|&n| n > 0)
                .ok_or((range, "expected an assembly operand expression".into()))?;
            expressions.push(tokens[at..at + n].to_vec());
            if register.fixed().is_some() && name.is_some() {
                return Err((range, "explicit register operands cannot have names".into()));
            }
            if register.fixed().is_none()
                && name.is_none()
                && registers.iter().any(|r| r.fixed().is_some())
            {
                return Err((
                    range,
                    "positional operands cannot follow explicit register operands".into(),
                ));
            }
            registers.push(register);
            if register.fixed().is_none() && name.is_none() && names.iter().any(Option::is_some) {
                return Err((
                    tokens[at].range(),
                    "positional operands cannot follow named operands".into(),
                ));
            }
            names.push(name);
            at += n;
            if tokens.get(at).is_some_and(|t| t.kind == T![=]) {
                if !matches!(kind, "inout" | "inlateout")
                    || tokens.get(at + 1).is_none_or(|t| t.kind != T![>])
                {
                    return Err((
                        tokens[at].range(),
                        "expected `=>` after an inout operand".into(),
                    ));
                }
                at += 2;
                let n = parser::fragment_len(&input, at, parser::Fragment::Expr)
                    .filter(|&n| n > 0)
                    .ok_or((range, "expected an assembly output place".into()))?;
                outputs.push(((registers.len() - 1) as u32, tokens[at..at + n].to_vec()));
                at += n;
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
    if options.pure
        && registers.iter().enumerate().all(|(index, register)| {
            let output = outputs
                .iter()
                .find(|(i, _)| *i as usize == index)
                .map_or(&expressions[index], |(_, output)| output);
            register.input_only() || matches!(&output[..], [token] if token.kind == T![_])
        })
    {
        return Err((range, "`pure` assembly requires an output".into()));
    }
    let template = template(&text, &names, &registers, range)?;
    let mut tuple = Vec::new();
    cx.quote("(", &mut tuple);
    let mut output_indices = Vec::with_capacity(outputs.len());
    for expr in expressions {
        tuple.extend(expr);
        cx.quote(",", &mut tuple);
    }
    for (index, expr) in outputs {
        output_indices.push(index);
        tuple.extend(expr);
        cx.quote(",", &mut tuple);
    }
    cx.quote(")", &mut tuple);
    Ok((
        tuple,
        AsmLayout {
            template: template.into(),
            registers: registers.into(),
            options,
            outputs: output_indices.into(),
        },
    ))
}

// Keep explicit operands within the pinned backend's allocatable pools.
// Canonical names also make alias overlap checks independent of operand width.
fn fixed_register(name: &str) -> Option<Symbol> {
    if cfg!(target_arch = "aarch64") {
        let (prefix, number) = name.split_at_checked(1)?;
        let number: u8 = number.parse().ok()?;
        if name[1..] != number.to_string() {
            return None;
        }
        let canonical = match prefix {
            "x" | "w" if number <= 8 || matches!(number, 16 | 17) => format!("x{number}"),
            "v" | "b" | "h" | "s" | "d" | "q" if number <= 7 || (16..=18).contains(&number) => {
                format!("v{number}")
            }
            _ => return None,
        };
        return Some(Symbol::intern(&canonical));
    }
    if cfg!(target_arch = "x86_64") {
        let canonical = match name {
            "rax" | "eax" | "ax" | "al" => "rax",
            "rcx" | "ecx" | "cx" | "cl" => "rcx",
            "rdx" | "edx" | "dx" | "dl" => "rdx",
            "rsi" | "esi" | "si" | "sil" => "rsi",
            "rdi" | "edi" | "di" | "dil" => "rdi",
            "r8" | "r8d" | "r8w" | "r8b" => "r8",
            "r9" | "r9d" | "r9w" | "r9b" => "r9",
            "r10" | "r10d" | "r10w" | "r10b" => "r10",
            "r11" | "r11d" | "r11w" | "r11b" => "r11",
            _ => {
                let number = name.strip_prefix("xmm")?.parse::<u8>().ok()?;
                if number >= 16 || name != format!("xmm{number}") {
                    return None;
                }
                return Some(Symbol::intern(name));
            }
        };
        return Some(Symbol::intern(canonical));
    }
    None
}

fn template(
    text: &str,
    names: &[Option<Symbol>],
    registers: &[AsmRegister],
    range: TextRange,
) -> Result<Vec<AsmTemplatePiece>, Error> {
    let mut chars = text.chars().peekable();
    let mut literal = String::new();
    let mut pieces = Vec::new();
    let mut next = 0;
    let mut used: Vec<_> = registers.iter().map(|r| r.fixed().is_some()).collect();
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
            || registers[index].fixed().is_some()
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

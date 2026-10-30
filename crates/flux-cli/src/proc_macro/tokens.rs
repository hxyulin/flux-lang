//! Checked flat-token encoding and physical-source callbacks for protocol 6.
use super::protocol::{FlatTree, SpanJoin as JoinedSpan, SubRequest, SubResponse};
use flux_hir::{ProcMacroInput, ProcMacroToken};
use flux_syntax::{LexedStr, SyntaxKind, TextRange, TokenOrigin};

pub(super) struct Codec<'a> {
    input: &'a ProcMacroInput,
    pub spans: Vec<u32>,
    origins: Vec<TokenOrigin>,
    span_ids: std::collections::BTreeMap<[u32; 5], u32>,
    file: u32,
}
impl<'a> Codec<'a> {
    pub fn new(input: &'a ProcMacroInput) -> Result<Self, String> {
        let edition = match input.source.edition {
            flux_syntax::Edition::E2015 => 0,
            flux_syntax::Edition::E2018 => 1,
            flux_syntax::Edition::E2021 => 2,
            flux_syntax::Edition::E2024 => 3,
        };
        let mut codec = Codec {
            input,
            spans: Vec::new(),
            origins: Vec::new(),
            span_ids: std::collections::BTreeMap::new(),
            file: edition << 23,
        };
        for (id, ctx) in [(0, 0), (1, 1), (2, 2)] {
            let start = input
                .call
                .start()
                .checked_sub(input.range_base)
                .ok_or("invalid proc-macro call source range")?;
            let end = input
                .call
                .end()
                .checked_sub(input.range_base)
                .ok_or("invalid proc-macro call source range")?;
            let row = [codec.file, 0, start, end, ctx];
            codec.span_ids.insert(row, id);
            codec.spans.extend(row);
        }
        codec.origins.push(TokenOrigin {
            range: input.call,
            group: TokenOrigin::SYNTHETIC,
            ctxt: 0,
        });
        Ok(codec)
    }
    fn span(&mut self, origin: TokenOrigin) -> Result<u32, String> {
        let anchor = if let Some(index) = self
            .origins
            .iter()
            .position(|s| s.group == origin.group && s.ctxt == origin.ctxt)
        {
            index
        } else {
            let index = self.origins.len();
            self.origins.push(origin);
            index
        };
        let start = origin
            .range
            .start()
            .checked_sub(self.input.range_base)
            .ok_or("invalid proc-macro token source range")?;
        let end = origin
            .range
            .end()
            .checked_sub(self.input.range_base)
            .ok_or("invalid proc-macro token source range")?;
        let row = [self.file, anchor as u32, start, end, 0];
        if let Some(&id) = self.span_ids.get(&row) {
            return Ok(id);
        }
        let id = u32::try_from(self.spans.len() / 5).map_err(|_| "too many proc-macro spans")?;
        self.span_ids.insert(row, id);
        self.spans.extend(row);
        Ok(id)
    }
    pub fn encode(&mut self, tokens: &[ProcMacroToken]) -> Result<FlatTree, String> {
        let mut tree = FlatTree::default();
        self.group(&mut tree, tokens, &mut 0, None, 0)?;
        Ok(tree)
    }
    fn group(
        &mut self,
        tree: &mut FlatTree,
        tokens: &[ProcMacroToken],
        pos: &mut usize,
        close: Option<&str>,
        depth: usize,
    ) -> Result<u32, String> {
        if depth > 128 {
            return Err("proc-macro token nesting limit exceeded".into());
        }
        let index = (tree.subtree.len() / 5) as u32;
        tree.subtree.extend([0, 0, 0, 0, 0]);
        let mut children = Vec::new();
        while let Some(t) = tokens.get(*pos) {
            if close == Some(t.text.as_str()) {
                break;
            }
            let span = self.span(t.origin)?;
            *pos += 1;
            if let Some((delimiter, close)) = match t.text.as_str() {
                "(" => Some((1, ")")),
                "{" => Some((2, "}")),
                "[" => Some((3, "]")),
                _ => None,
            } {
                let child = self.group(tree, tokens, pos, Some(close), depth + 1)?;
                let end = tokens.get(*pos).ok_or("unclosed proc-macro delimiter")?;
                let end = self.span(end.origin)?;
                tree.subtree[child as usize * 5..child as usize * 5 + 3]
                    .copy_from_slice(&[span, end, delimiter]);
                *pos += 1;
                children.push(child << 2);
            } else if matches!(t.text.as_str(), ")" | "}" | "]") {
                return Err("unmatched proc-macro delimiter".into());
            } else if t.kind.is_literal() {
                let combined;
                let mut literal_span = span;
                let text = if !matches!(t.kind, SyntaxKind::INT_NUMBER | SyntaxKind::FLOAT_NUMBER)
                    && t.joint
                    && tokens.get(*pos).is_some_and(|n| {
                        (n.kind == SyntaxKind::IDENT || n.kind.is_keyword())
                            && n.origin.group == t.origin.group
                            && n.origin.ctxt == t.origin.ctxt
                            && (t.origin.range.end() == n.origin.range.start()
                                || t.origin.range == n.origin.range)
                    }) {
                    let next = &tokens[*pos];
                    if next.text.starts_with("r#") {
                        return Err(
                            "raw-identifier literal suffix boundaries are not supported yet".into(),
                        );
                    }
                    combined = format!("{}{}", t.text, next.text);
                    literal_span = self.span(TokenOrigin {
                        range: TextRange::new(t.origin.range.start(), next.origin.range.end()),
                        ..t.origin
                    })?;
                    *pos += 1;
                    combined.as_str()
                } else {
                    t.text.as_str()
                };
                let (kind, symbol, suffix) = literal_parts(text, t.kind)?;
                let symbol = intern(tree, symbol);
                let suffix = if suffix.is_empty() {
                    u32::MAX
                } else {
                    intern(tree, suffix)
                };
                let index = (tree.literal.len() / 4) as u32;
                tree.literal.extend([literal_span, symbol, kind, suffix]);
                children.push(index << 2 | 1);
            } else if t.kind == SyntaxKind::LIFETIME_IDENT {
                let p = (tree.punct.len() / 3) as u32;
                tree.punct.extend([span, '\'' as u32, 1]);
                children.push(p << 2 | 2);
                let name = t.text.strip_prefix('\'').ok_or("invalid lifetime token")?;
                let raw = name.starts_with("r#");
                let name = intern(tree, name.trim_start_matches("r#"));
                let i = (tree.ident.len() / 3) as u32;
                tree.ident.extend([span, name, u32::from(raw)]);
                children.push(i << 2 | 3);
            } else if matches!(t.kind, SyntaxKind::IDENT | SyntaxKind::UNDERSCORE)
                || t.kind.is_keyword()
            {
                let raw = t.text.starts_with("r#");
                let name = intern(tree, t.text.trim_start_matches("r#"));
                let i = (tree.ident.len() / 3) as u32;
                tree.ident.extend([span, name, u32::from(raw)]);
                children.push(i << 2 | 3);
            } else {
                let mut chars = t.text.chars();
                let ch = chars.next().ok_or("empty proc-macro token")?;
                if chars.next().is_some() {
                    return Err("expected split proc-macro punctuation".into());
                }
                let i = (tree.punct.len() / 3) as u32;
                tree.punct.extend([span, ch as u32, u32::from(t.joint)]);
                children.push(i << 2 | 2);
            }
        }
        if close.is_some() && *pos == tokens.len() {
            return Err("unclosed proc-macro delimiter".into());
        }
        let lo = tree.token_tree.len() as u32;
        tree.token_tree.extend(children);
        let hi = tree.token_tree.len() as u32;
        tree.subtree[index as usize * 5 + 3..index as usize * 5 + 5].copy_from_slice(&[lo, hi]);
        Ok(index)
    }
    fn origin(&self, id: u32, spans: &[u32]) -> Result<TokenOrigin, String> {
        let row = spans
            .get(id as usize * 5..id as usize * 5 + 5)
            .ok_or("invalid host token span")?;
        if row[0] != self.file
            || row[2] > row[3]
            || (row[1] != 0 && row[3] as usize > self.input.source.text().len())
        {
            return Err("invalid host source span".into());
        }
        let base = self
            .origins
            .get(row[1] as usize)
            .ok_or("unknown host span anchor")?;
        let ctxt = match row[4] {
            0 => base.ctxt,
            1 => self.input.mixed_context,
            _ => return Err("proc-macro definition-site hygiene is not supported yet".into()),
        };
        // TokenStream::from_str uses synthetic offsets under the global anchor.
        // Those offsets describe generated text, not bytes in the invocation file.
        let range = if row[1] == 0 {
            self.input.call
        } else {
            TextRange::new(
                row[2] + self.input.range_base,
                row[3] + self.input.range_base,
            )
        };
        Ok(TokenOrigin {
            range,
            ctxt,
            group: base.group,
        })
    }
    pub fn decode(&self, tree: FlatTree, spans: &[u32]) -> Result<Vec<ProcMacroToken>, String> {
        if !tree.subtree.len().is_multiple_of(5)
            || !tree.literal.len().is_multiple_of(4)
            || !tree.punct.len().is_multiple_of(3)
            || !tree.ident.len().is_multiple_of(3)
            || !spans.len().is_multiple_of(5)
        {
            return Err("invalid host flat-token table".into());
        }
        let mut result = Vec::new();
        self.decode_group(&tree, spans, 0, 0, &mut result)?;
        let mut joined: Vec<ProcMacroToken> = Vec::new();
        for token in result {
            if let Some(previous) = joined.last_mut()
                && previous.text == "'"
                && previous.joint
                && (matches!(token.kind, SyntaxKind::IDENT | SyntaxKind::UNDERSCORE)
                    || token.kind.is_keyword())
            {
                if previous.origin.ctxt != token.origin.ctxt {
                    return Err("lifetimes with mixed token hygiene are not supported yet".into());
                }
                previous.text.push_str(&token.text);
                previous.kind = SyntaxKind::LIFETIME_IDENT;
                previous.joint = token.joint;
            } else {
                joined.push(token);
            }
        }
        if joined.iter().any(|t| t.kind == SyntaxKind::ERROR) {
            return Err("unpaired lifetime punctuation from host macro".into());
        }
        Ok(joined)
    }
    fn decode_group(
        &self,
        tree: &FlatTree,
        spans: &[u32],
        index: usize,
        depth: usize,
        out: &mut Vec<ProcMacroToken>,
    ) -> Result<(), String> {
        if depth > 128 || out.len() > 1_000_000 {
            return Err("host token output limit exceeded".into());
        }
        let row = tree
            .subtree
            .get(index * 5..index * 5 + 5)
            .ok_or("invalid host subtree")?;
        let pair = match row[2] {
            0 if depth != 0 => {
                return Err("invisible proc-macro delimiter groups are not supported yet".into());
            }
            0 => None,
            1 => Some(("(", ")")),
            2 => Some(("{", "}")),
            3 => Some(("[", "]")),
            _ => return Err("invalid host delimiter".into()),
        };
        if let Some((open, _)) = pair {
            self.atom(open, self.origin(row[0], spans)?, false, out)?;
        }
        let tokens = tree
            .token_tree
            .get(row[3] as usize..row[4] as usize)
            .ok_or("invalid host subtree interval")?;
        for &token in tokens {
            let i = (token >> 2) as usize;
            match token & 3 {
                0 => self.decode_group(tree, spans, i, depth + 1, out)?,
                1 => {
                    let row = tree
                        .literal
                        .get(i * 4..i * 4 + 4)
                        .ok_or("invalid host literal")?;
                    let text = tree
                        .text
                        .get(row[1] as usize)
                        .ok_or("invalid host literal text")?;
                    let mut text = literal_text(text, row[2])?;
                    if row[3] != u32::MAX {
                        text.push_str(
                            tree.text
                                .get(row[3] as usize)
                                .ok_or("invalid host literal suffix")?,
                        );
                    }
                    self.atom(&text, self.origin(row[0], spans)?, false, out)?;
                }
                2 => {
                    let row = tree
                        .punct
                        .get(i * 3..i * 3 + 3)
                        .ok_or("invalid host punctuation")?;
                    if row[2] > 1 {
                        return Err("invalid host punctuation spacing".into());
                    }
                    self.atom(
                        &char::from_u32(row[1])
                            .ok_or("invalid host punctuation character")?
                            .to_string(),
                        self.origin(row[0], spans)?,
                        row[2] == 1,
                        out,
                    )?;
                }
                3 => {
                    let row = tree
                        .ident
                        .get(i * 3..i * 3 + 3)
                        .ok_or("invalid host identifier")?;
                    let text = tree
                        .text
                        .get(row[1] as usize)
                        .ok_or("invalid host identifier text")?;
                    let text = if row[2] == 1 {
                        format!("r#{text}")
                    } else {
                        text.clone()
                    };
                    self.atom(&text, self.origin(row[0], spans)?, false, out)?;
                }
                _ => unreachable!(),
            }
        }
        if let Some((_, close)) = pair {
            self.atom(close, self.origin(row[1], spans)?, false, out)?;
        }
        Ok(())
    }
    fn atom(
        &self,
        text: &str,
        origin: TokenOrigin,
        joint: bool,
        out: &mut Vec<ProcMacroToken>,
    ) -> Result<(), String> {
        if out.len() >= 1_000_000 {
            return Err("host token output limit exceeded".into());
        }
        if text == "'" {
            if !joint {
                return Err("unpaired lifetime punctuation from host macro".into());
            }
            out.push(ProcMacroToken {
                edition: self.input.source.edition,
                kind: SyntaxKind::ERROR,
                text: text.into(),
                joint,
                origin,
            });
            return Ok(());
        }
        let lexed = LexedStr::with_edition(text, self.input.source.edition);
        if !lexed.errors().is_empty() {
            return Err(format!("invalid generated token `{text}`"));
        }
        for i in 0..lexed.len() {
            if lexed.kind(i).is_trivia() {
                return Err("unexpected trivia inside generated token".into());
            }
            out.push(ProcMacroToken {
                edition: self.input.source.edition,
                kind: lexed.kind(i),
                text: lexed.token_text(i).into(),
                joint: if i + 1 == lexed.len() {
                    joint
                } else {
                    lexed.kind(i) != SyntaxKind::MINUS
                },
                origin,
            });
        }
        Ok(())
    }
    fn source_range(&self, anchor: u32, start: u32, end: u32) -> Result<(u32, u32), String> {
        if anchor == 0 {
            return Ok((
                self.input.call.start() - self.input.range_base,
                self.input.call.end() - self.input.range_base,
            ));
        }
        if self.origins.get(anchor as usize).is_none()
            || start > end
            || end as usize > self.input.source.text().len()
        {
            return Err("invalid host source query".into());
        }
        Ok((start, end))
    }
    fn local_file(&self, id: u32) -> Result<(), String> {
        if id & 0x007f_ffff != 0 {
            return Err("unknown source file requested by host macro".into());
        }
        Ok(())
    }
    pub fn callback(&self, request: SubRequest) -> Result<SubResponse, String> {
        use SubRequest::*;
        use SubResponse::*;
        Ok(match request {
            FilePath { file_id } => {
                self.local_file(file_id)?;
                FilePathResult {
                    name: self.input.source.path.display().to_string(),
                }
            }
            LocalFilePath { file_id } => {
                self.local_file(file_id)?;
                LocalFilePathResult {
                    name: Some(self.input.source.path.display().to_string()),
                }
            }
            SourceText {
                file_id,
                ast_id,
                start,
                end,
            } => {
                self.local_file(file_id)?;
                let (start, end) = self.source_range(ast_id, start, end)?;
                SourceTextResult {
                    text: self
                        .input
                        .source
                        .text()
                        .get(start as usize..end as usize)
                        .map(str::to_owned),
                }
            }
            LineColumn {
                file_id,
                ast_id,
                offset,
            } => {
                self.local_file(file_id)?;
                let (offset, _) = self.source_range(ast_id, offset, offset)?;
                let before = self
                    .input
                    .source
                    .text()
                    .get(..offset as usize)
                    .ok_or("invalid host line/column query")?;
                LineColumnResult {
                    line: before.bytes().filter(|&b| b == b'\n').count() as u32 + 1,
                    column: before.rsplit('\n').next().unwrap_or("").chars().count() as u32 + 1,
                }
            }
            ByteRange {
                file_id,
                ast_id,
                start,
                end,
            } => {
                self.local_file(file_id)?;
                let (start, end) = self.source_range(ast_id, start, end)?;
                ByteRangeResult {
                    range: start as usize..end as usize,
                }
            }
            SpanSource {
                file_id,
                ast_id,
                start,
                end,
                ctx,
            } => {
                self.local_file(file_id)?;
                SpanSourceResult {
                    file_id,
                    ast_id,
                    start,
                    end,
                    ctx,
                }
            }
            SpanParent { .. } => SpanParentResult { parent_span: None },
            SpanJoin {
                file_id,
                ast_id_first,
                start_first,
                end_first,
                ctx_first,
                ast_id_second,
                start_second,
                end_second,
                ctx_second,
            } => {
                self.local_file(file_id)?;
                SpanJoinResult {
                    span: (ctx_first == ctx_second && ast_id_first == ast_id_second).then_some(
                        JoinedSpan {
                            ast_id: ast_id_first,
                            start: start_first.min(start_second),
                            end: end_first.max(end_second),
                            ctx: ctx_first,
                        },
                    ),
                }
            }
        })
    }
}
fn intern(tree: &mut FlatTree, text: &str) -> u32 {
    let id = tree.text.len() as u32;
    tree.text.push(text.into());
    id
}
fn literal_parts(text: &str, kind: SyntaxKind) -> Result<(u32, &str, &str), String> {
    if matches!(kind, SyntaxKind::INT_NUMBER | SyntaxKind::FLOAT_NUMBER) {
        let bytes = text.as_bytes();
        let mut end = 0;
        if text.starts_with("0x") || text.starts_with("0o") || text.starts_with("0b") {
            let radix = match bytes[1] {
                b'x' => 16,
                b'o' => 8,
                _ => 2,
            };
            end = 2;
            while end < bytes.len() && (bytes[end] == b'_' || (bytes[end] as char).is_digit(radix))
            {
                end += 1;
            }
        } else {
            while end < bytes.len() && (bytes[end].is_ascii_digit() || bytes[end] == b'_') {
                end += 1;
            }
            if bytes.get(end) == Some(&b'.') {
                end += 1;
                while end < bytes.len() && (bytes[end].is_ascii_digit() || bytes[end] == b'_') {
                    end += 1;
                }
            }
            if matches!(bytes.get(end), Some(b'e' | b'E')) {
                end += 1;
                if matches!(bytes.get(end), Some(b'+' | b'-')) {
                    end += 1;
                }
                while end < bytes.len() && (bytes[end].is_ascii_digit() || bytes[end] == b'_') {
                    end += 1;
                }
            }
        }
        return Ok((
            if kind == SyntaxKind::FLOAT_NUMBER {
                4
            } else {
                3
            },
            &text[..end],
            &text[end..],
        ));
    }
    let first = text.find(['\'', '"']).ok_or("invalid literal delimiter")?;
    let delimiter = text.as_bytes()[first] as char;
    let last = text.rfind(delimiter).ok_or("invalid literal terminator")?;
    let prefix = &text[..first];
    let hashes = prefix.bytes().filter(|&c| c == b'#').count();
    let raw = prefix.contains('r');
    let byte = prefix.starts_with('b');
    let cstr = prefix.starts_with('c');
    let tag = if delimiter == '\'' {
        if byte { 1 } else { 2 }
    } else {
        match (raw, byte, cstr) {
            (false, false, false) => 5,
            (true, false, false) => 6,
            (false, true, _) => 7,
            (true, true, _) => 8,
            (false, _, true) => 9,
            (true, _, true) => 10,
        }
    };
    let suffix = last + 1 + if raw { hashes } else { 0 };
    Ok((
        tag | ((hashes as u32) << 8),
        &text[first + 1..last],
        text.get(suffix..).ok_or("invalid raw literal suffix")?,
    ))
}
fn literal_text(text: &str, kind: u32) -> Result<String, String> {
    let hashes = "#".repeat((kind >> 8) as usize);
    Ok(match kind & 255 {
        1 => format!("b'{text}'"),
        2 => format!("'{text}'"),
        3 | 4 => text.into(),
        5 => format!("\"{text}\""),
        6 => format!("r{hashes}\"{text}\"{hashes}"),
        7 => format!("b\"{text}\""),
        8 => format!("br{hashes}\"{text}\"{hashes}"),
        9 => format!("c\"{text}\""),
        10 => format!("cr{hashes}\"{text}\"{hashes}"),
        _ => return Err("invalid host literal kind".into()),
    })
}

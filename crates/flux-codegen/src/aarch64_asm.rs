//! Compatibility with the pinned Darwin assembler.
//! Rewrite only complete instructions after register allocation, preserving
//! every register operand and the exact architectural encoding.
use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::Write;

/// Resolves GNU numeric labels in emission order, including forward references
/// across source fragments. Mach-O assembly still checks branch distances and
/// rejects references with no definition.
#[derive(Default)]
pub(crate) struct NumericLabels {
    counts: HashMap<Box<str>, usize>,
    preprocessed: bool,
    rewritten: bool,
}

impl NumericLabels {
    pub(crate) fn has_preprocessed_numeric_labels(&self) -> bool {
        // The pinned assembler expands these directives after this pass.
        // Expansion can change which definition a reference should see.
        self.preprocessed && self.rewritten
    }

    pub(crate) fn rewrite<'a>(&mut self, text: &'a str) -> Cow<'a, str> {
        fn name_byte(byte: u8) -> bool {
            byte.is_ascii_alphanumeric() || b"_.$".contains(&byte)
        }

        let bytes = text.as_bytes();
        let mut output = None;
        let mut written = 0;
        let mut at = 0;
        let mut statement_start = true;
        while at < bytes.len() {
            let start = at;
            at += 1;
            match bytes[start] {
                b'\n' | b'\r' => statement_start = true,
                b if b.is_ascii_whitespace() => {}
                b if b == b';' || (b == b'/' && bytes.get(at) == Some(&b'/')) => {
                    at = text[at..]
                        .find(['\n', '\r'])
                        .map_or(bytes.len(), |end| at + end);
                }
                b'/' if bytes.get(at) == Some(&b'*') => {
                    at += 1;
                    at = text[at..]
                        .find("*/")
                        .map_or(bytes.len(), |end| at + end + 2);
                }
                quote @ (b'"' | b'\'') => {
                    while let Some(&byte) = bytes.get(at) {
                        at += 1;
                        if byte == quote {
                            break;
                        }
                        if byte == b'\\' && at < bytes.len() {
                            at += 1;
                        }
                    }
                    statement_start = false;
                }
                b if name_byte(b) => {
                    while bytes.get(at).is_some_and(|&b| name_byte(b)) {
                        at += 1;
                    }
                    let end = at;
                    let token = &text[start..end];
                    if statement_start && token.starts_with('.') {
                        self.preprocessed |= matches!(
                            token.to_ascii_lowercase().as_str(),
                            ".macro"
                                | ".rept"
                                | ".if"
                                | ".ifeq"
                                | ".ifne"
                                | ".ifgt"
                                | ".ifge"
                                | ".iflt"
                                | ".ifle"
                        );
                    }
                    let colon = if statement_start {
                        label_colon(&text[end..])
                    } else {
                        None
                    };
                    statement_start = colon.is_some();
                    if let Some(colon) = colon {
                        at = end + colon + 1;
                    }
                    let (digits, direction) = if colon.is_some() {
                        (token, None)
                    } else if let Some(digits) = token.strip_suffix('f') {
                        (digits, Some(true))
                    } else if let Some(digits) = token.strip_suffix('b') {
                        (digits, Some(false))
                    } else {
                        continue;
                    };
                    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
                        continue;
                    }
                    let number = digits.trim_start_matches('0');
                    let occurrence = match direction {
                        None => {
                            let count = self.counts.entry(number.into()).or_default();
                            *count += 1;
                            *count
                        }
                        Some(forward) => {
                            self.counts.get(number).copied().unwrap_or(0) + usize::from(forward)
                        }
                    };
                    if occurrence == 0 {
                        // A backward reference with no definition is invalid as written.
                        continue;
                    }
                    let output = output.get_or_insert_with(|| String::with_capacity(text.len()));
                    self.rewritten = true;
                    output.push_str(&text[written..start]);
                    write!(output, "L__flux_asm_numeric_{number}_{occurrence}").unwrap();
                    written = end;
                }
                _ => statement_start = false,
            }
        }
        match output {
            Some(mut output) => {
                output.push_str(&text[written..]);
                Cow::Owned(output)
            }
            None => Cow::Borrowed(text),
        }
    }
}

/// The assembler treats block comments as whitespace within a statement.
fn label_colon(mut tail: &str) -> Option<usize> {
    let len = tail.len();
    loop {
        tail = tail.trim_start_matches([' ', '\t']);
        if tail.starts_with(':') {
            return Some(len - tail.len());
        }
        let comment = tail.strip_prefix("/*")?;
        tail = comment.split_once("*/")?.1;
    }
}

pub(crate) fn compatible_source(text: &str) -> Cow<'_, str> {
    fn code(line: &str) -> &str {
        line.split_once("//").map_or(line, |(code, _)| code).trim()
    }

    // The pinned assembler already enables FEAT_LSE in its baseline.
    // Other extensions and explicit disables still reach the assembler.
    fn redundant_lse(line: &str) -> bool {
        let mut words = code(line).split_ascii_whitespace();
        words.next() == Some(".arch_extension")
            && words.next() == Some("lse")
            && words.next().is_none()
    }

    fn register(text: &str) -> Option<u32> {
        if text == "xzr" {
            return Some(31);
        }
        text.strip_prefix('x')?
            .parse::<u32>()
            .ok()
            .filter(|&n| n < 31)
    }
    fn encoding(line: &str) -> Option<u32> {
        let line = code(line).to_ascii_lowercase();
        if line == "sb" {
            return Some(0xd503_30ff);
        }
        let (op, args) = line.split_once(char::is_whitespace)?;
        let (left, right) = args.trim().split_once(',')?;
        let (left, right) = (left.trim(), right.trim());
        match (op, left, right) {
            ("mrs", reg, "dit") => Some(0xd53b_42a0 | register(reg)?),
            ("msr", "dit", "#0") => Some(0xd503_405f),
            ("msr", "dit", "#1") => Some(0xd503_415f),
            ("msr", "dit", reg) => Some(0xd51b_42a0 | register(reg)?),
            _ => None,
        }
    }
    if !text
        .lines()
        .any(|line| redundant_lse(line) || encoding(line).is_some())
    {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        if redundant_lse(line) {
            if line.ends_with('\n') {
                out.push('\n');
            }
        } else if let Some(word) = encoding(line) {
            write!(out, ".long 0x{word:08x}").unwrap();
            if line.ends_with('\n') {
                out.push('\n');
            }
        } else {
            out.push_str(line);
        }
    }
    Cow::Owned(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn system_encodings_match_clang() {
        // clang -target arm64-apple-macos13 -march=armv8.5-a, checked
        // against the __text bytes for these instructions.
        let source =
            "mrs x0, DIT\nmrs x7, DIT\nmsr DIT, x0\nmsr DIT, x7\nmsr DIT, #0\nmsr DIT, #1\nsb\n";
        assert_eq!(
            compatible_source(source),
            ".long 0xd53b42a0\n.long 0xd53b42a7\n.long 0xd51b42a0\n.long 0xd51b42a7\n.long 0xd503405f\n.long 0xd503415f\n.long 0xd50330ff\n"
        );
        assert!(matches!(
            compatible_source("sb:\nmsr dit, #2\nmrs w0, dit\n"),
            Cow::Borrowed(_)
        ));
    }

    #[test]
    fn only_the_baseline_lse_enable_is_redundant() {
        let source = ".arch_extension lse // already enabled\nnop\n.arch_extension nolse\n.arch_extension lse128\n";
        assert_eq!(
            compatible_source(source),
            "\nnop\n.arch_extension nolse\n.arch_extension lse128\n"
        );
    }

    #[test]
    fn numeric_labels_use_the_nearest_definition_across_fragments() {
        let mut labels = NumericLabels::default();
        assert_eq!(labels.rewrite("b 2f\n"), "b L__flux_asm_numeric_2_1\n");
        let source =
            "02 /* label */ : nop ; 2: ignored\ncbnz x5, 2b\n2: b 02b // 2f\n.ascii \"2b\"\nb 7b\n";
        assert_eq!(
            labels.rewrite(source),
            "L__flux_asm_numeric_2_1 /* label */ : nop ; 2: ignored\ncbnz x5, L__flux_asm_numeric_2_1\nL__flux_asm_numeric_2_2: b L__flux_asm_numeric_2_2 // 2f\n.ascii \"2b\"\nb 7b\n"
        );
    }

    #[test]
    fn numeric_labels_require_definitions_after_preprocessing() {
        let mut labels = NumericLabels::default();
        labels.rewrite("2: nop\n");
        assert!(!labels.has_preprocessed_numeric_labels());
        labels.rewrite(".rept 2\nnop\n.endr\n");
        assert!(labels.has_preprocessed_numeric_labels());
    }
}

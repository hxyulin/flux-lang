//! System instruction spellings missing from the pinned assembler.
//! Rewrite only complete instructions after register allocation, preserving
//! every register operand and the exact architectural encoding.
use std::borrow::Cow;

pub(crate) fn system_aliases(text: &str) -> Cow<'_, str> {
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
        let line = line
            .split_once("//")
            .map_or(line, |(code, _)| code)
            .trim()
            .to_ascii_lowercase();
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
    if !text.lines().any(|line| encoding(line).is_some()) {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        if let Some(word) = encoding(line) {
            use std::fmt::Write;
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
            system_aliases(source),
            ".long 0xd53b42a0\n.long 0xd53b42a7\n.long 0xd51b42a0\n.long 0xd51b42a7\n.long 0xd503405f\n.long 0xd503415f\n.long 0xd50330ff\n"
        );
        assert!(matches!(
            system_aliases("sb:\nmsr dit, #2\nmrs w0, dit\n"),
            Cow::Borrowed(_)
        ));
    }
}

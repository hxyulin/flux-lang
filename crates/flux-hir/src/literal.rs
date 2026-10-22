//! Values of literal tokens: integers, characters, bytes and strings.
//!
//! Escapes are decoded as rustc decodes them, with its messages for the
//! malformed ones. The lexer has already checked that each literal is
//! terminated.

use flux_intern::Symbol;

use crate::Literal;

/// Parses an integer literal: `1_000`, `0xff_u8`, `0b1010`.
pub(crate) fn parse_int(text: &str) -> Result<Literal, String> {
    let (radix, rest) = match text.get(..2) {
        Some("0x") => (16, &text[2..]),
        Some("0o") => (8, &text[2..]),
        Some("0b") => (2, &text[2..]),
        _ => (10, text),
    };
    let digits_end = rest
        .find(|c: char| !(c.is_digit(radix) || c == '_'))
        .unwrap_or(rest.len());
    let (digits, suffix) = rest.split_at(digits_end);
    let suffix = match suffix {
        "" => None,
        "f32" | "f64" if radix == 10 => {
            return Err("floating-point numbers are not supported yet".to_owned());
        }
        "u8" | "u16" | "u32" | "u64" | "u128" | "usize" | "i8" | "i16" | "i32" | "i64" | "i128"
        | "isize" => Some(Symbol::intern(suffix)),
        _ => return Err(format!("invalid suffix `{suffix}` for number literal")),
    };
    let mut value: u128 = 0;
    let mut any_digit = false;
    for c in digits.chars().filter(|&c| c != '_') {
        let digit = c.to_digit(radix).expect("digits were checked") as u128;
        value = value
            .checked_mul(radix as u128)
            .and_then(|v| v.checked_add(digit))
            .ok_or("integer literal is too large")?;
        any_digit = true;
    }
    if !any_digit {
        return Err("no valid digits found for number".to_owned());
    }
    Ok(Literal::Int { value, suffix })
}

/// What a literal's escapes may produce, as rustc's `Mode`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Char,
    Byte,
    Str,
    ByteStr,
    CStr,
}

impl Mode {
    /// Whether the literal's units are bytes, which must be ASCII
    /// unless escaped and may not be `\u` escapes.
    fn is_byte(self) -> bool {
        matches!(self, Mode::Byte | Mode::ByteStr)
    }

    /// Whether `\x80` to `\xff` are allowed: as bytes.
    fn allows_high_bytes(self) -> bool {
        matches!(self, Mode::Byte | Mode::ByteStr | Mode::CStr)
    }
}

/// One decoded unit of a literal: a character, or a byte from a `\x`
/// escape of `\x80` or above in a byte or C string.
enum Unit {
    Char(char),
    Byte(u8),
}

/// The value of a character literal, `'a'` or `'\n'`.
pub(crate) fn unescape_char(text: &str) -> Result<char, String> {
    let body = text
        .strip_prefix('\'')
        .and_then(|t| t.strip_suffix('\''))
        .ok_or("unterminated character literal")?;
    match single(body, Mode::Char)? {
        Unit::Char(c) => Ok(c),
        Unit::Byte(b) => Ok(char::from(b)),
    }
}

/// The value of a byte literal, `b'a'` or `b'\xff'`.
pub(crate) fn unescape_byte(text: &str) -> Result<u8, String> {
    let body = text
        .strip_prefix("b'")
        .and_then(|t| t.strip_suffix('\''))
        .ok_or("unterminated byte constant")?;
    match single(body, Mode::Byte)? {
        Unit::Byte(b) => Ok(b),
        // Byte literals' characters were checked to be ASCII.
        Unit::Char(c) => Ok(c as u8),
    }
}

/// The text of a string literal, `"..."` or `r#"..."#`.
pub(crate) fn unescape_str(text: &str) -> Result<String, String> {
    let mut out = String::new();
    string(text, Mode::Str, |unit| match unit {
        Unit::Char(c) => out.push(c),
        Unit::Byte(b) => out.push(char::from(b)),
    })?;
    Ok(out)
}

/// The bytes of a byte string literal, `b"..."` or `br#"..."#`.
pub(crate) fn unescape_byte_str(text: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    string(text, Mode::ByteStr, |unit| match unit {
        Unit::Byte(b) => out.push(b),
        Unit::Char(c) => out.push(c as u8),
    })?;
    Ok(out)
}

/// The bytes of a C string literal, `c"..."` or `cr#"..."#`, without the
/// terminating NUL: escapes decoded, characters as UTF-8.
pub(crate) fn unescape_c_str(text: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut nul = false;
    string(text, Mode::CStr, |unit| match unit {
        Unit::Byte(b) => out.push(b),
        Unit::Char(c) => {
            nul |= c == '\0';
            let mut buffer = [0; 4];
            out.extend_from_slice(c.encode_utf8(&mut buffer).as_bytes());
        }
    })?;
    if nul {
        return Err("null characters in C string literals are not supported".to_owned());
    }
    Ok(out)
}

/// The one unit of a character or byte literal's `body`.
fn single(body: &str, mode: Mode) -> Result<Unit, String> {
    let mut chars = body.chars();
    let unit = match chars.next() {
        None => return Err("empty character literal".to_owned()),
        Some('\\') => escape(&mut chars, mode)?,
        Some(c @ ('\'' | '\n' | '\t' | '\r')) => {
            let what = if mode.is_byte() { "byte" } else { "character" };
            return Err(format!(
                "{what} constant must be escaped: `{}`",
                c.escape_default()
            ));
        }
        Some(c) if mode.is_byte() && !c.is_ascii() => {
            return Err("non-ASCII character in byte literal".to_owned());
        }
        Some(c) => Unit::Char(c),
    };
    if chars.next().is_some() {
        return Err("character literal may only contain one codepoint".to_owned());
    }
    Ok(unit)
}

/// Decodes a string literal of `mode`, raw or not, passing each unit to
/// `out`.
fn string(text: &str, mode: Mode, mut out: impl FnMut(Unit)) -> Result<(), String> {
    let body = text.trim_start_matches(['b', 'c']);
    let raw = body.starts_with('r');
    let body = body.strip_prefix('r').unwrap_or(body);
    let hashes = body.len() - body.trim_start_matches('#').len();
    let body = body[hashes..]
        .strip_prefix('"')
        .and_then(|b| b.strip_suffix(&"#".repeat(hashes)))
        .and_then(|b| b.strip_suffix('"'))
        .ok_or("unterminated double quote string")?;
    let mut chars = body.chars();
    while let Some(c) = chars.next() {
        let unit = match c {
            // rustc reads `\r\n` in source files as `\n`.
            '\r' if chars.clone().next() == Some('\n') => continue,
            '\r' if raw => return Err("bare CR not allowed in raw string".to_owned()),
            '\r' => return Err("bare CR not allowed in string, use `\\r` instead".to_owned()),
            '\\' if !raw => {
                let rest = chars.as_str();
                if let Some(after) = rest.strip_prefix('\n') {
                    // A line continuation: skips the newline and the
                    // whitespace after it.
                    chars = after.trim_start_matches([' ', '\t', '\n', '\r']).chars();
                    continue;
                }
                escape(&mut chars, mode)?
            }
            c if mode.is_byte() && !c.is_ascii() => {
                let what = if raw {
                    "raw byte string"
                } else {
                    "byte string"
                };
                return Err(format!("non-ASCII character in {what} literal"));
            }
            c => Unit::Char(c),
        };
        out(unit);
    }
    Ok(())
}

/// The unit an escape after its `\` stands for.
fn escape(chars: &mut std::str::Chars<'_>, mode: Mode) -> Result<Unit, String> {
    let c = match chars.next() {
        None => return Err("invalid trailing slash in literal".to_owned()),
        Some('n') => '\n',
        Some('r') => '\r',
        Some('t') => '\t',
        Some('\\') => '\\',
        Some('0') => '\0',
        Some('\'') => '\'',
        Some('"') => '"',
        Some('x') => {
            let mut digit = || match chars.next() {
                None => Err("numeric character escape is too short".to_owned()),
                Some(c) => c
                    .to_digit(16)
                    .ok_or_else(|| format!("invalid character in numeric character escape: `{c}`")),
            };
            let value = (digit()? * 16 + digit()?) as u8;
            if value > 0x7f {
                if !mode.allows_high_bytes() {
                    return Err("out of range hex escape".to_owned());
                }
                // `\x80` and above are single bytes, not characters.
                return Ok(Unit::Byte(value));
            }
            char::from(value)
        }
        Some('u') => {
            let c = unicode_escape(chars)?;
            if mode.is_byte() {
                return Err("unicode escape in byte string".to_owned());
            }
            c
        }
        Some(other) => {
            let what = if mode.is_byte() { "byte" } else { "character" };
            return Err(format!(
                "unknown {what} escape: `{}`",
                other.escape_default()
            ));
        }
    };
    Ok(Unit::Char(c))
}

/// `{1F600}` after `\u`.
fn unicode_escape(chars: &mut std::str::Chars<'_>) -> Result<char, String> {
    if chars.clone().next() != Some('{') {
        return Err("incorrect unicode escape sequence".to_owned());
    }
    chars.next();
    match chars.clone().next() {
        Some('_') => return Err("invalid start of unicode escape: `_`".to_owned()),
        Some('}') => return Err("empty unicode escape".to_owned()),
        _ => {}
    }
    let mut value: u32 = 0;
    let mut digits = 0;
    loop {
        match chars.next() {
            None => return Err("unterminated unicode escape".to_owned()),
            Some('}') => break,
            Some('_') => {}
            Some(c) => {
                let digit = c
                    .to_digit(16)
                    .ok_or_else(|| format!("invalid character in unicode escape: `{c}`"))?;
                digits += 1;
                if digits <= 6 {
                    value = value * 16 + digit;
                }
            }
        }
    }
    if digits > 6 {
        return Err("overlong unicode escape".to_owned());
    }
    char::from_u32(value).ok_or_else(|| "invalid unicode character escape".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_literals() {
        let int = |text| match parse_int(text) {
            Ok(Literal::Int { value, suffix }) => Ok((value, suffix.map(Symbol::as_str))),
            Ok(other) => panic!("{other:?}"),
            Err(message) => Err(message),
        };
        assert_eq!(int("0"), Ok((0, None)));
        assert_eq!(int("1_000"), Ok((1000, None)));
        assert_eq!(int("0xff_u8"), Ok((255, Some("u8"))));
        assert_eq!(int("0b1010i64"), Ok((10, Some("i64"))));
        assert_eq!(int("0o17"), Ok((15, None)));
        assert_eq!(
            int("340282366920938463463374607431768211455"),
            Ok((u128::MAX, None))
        );
        assert!(int("340282366920938463463374607431768211456").is_err());
        assert!(int("1u7").is_err());
        assert!(int("0x").is_err());
        assert!(int("1f32").is_err());
    }

    #[test]
    fn c_strings() {
        let c = |text: &str| unescape_c_str(text);
        assert_eq!(c(r#"c"hi""#).unwrap(), b"hi");
        assert_eq!(c(r#"c"a\nb\t\\\"\'""#).unwrap(), b"a\nb\t\\\"'");
        assert_eq!(c(r#"c"\x41\xff""#).unwrap(), b"A\xff");
        assert_eq!(c(r#"c"\u{e9}\u{1F_600}""#).unwrap(), "é😀".as_bytes());
        assert_eq!(c("c\"a\\\n    b\"").unwrap(), b"ab");
        assert_eq!(c("c\"a\r\nb\"").unwrap(), b"a\nb");
        assert_eq!(
            c(r##"cr#"no \n "escapes""#"##).unwrap(),
            br#"no \n "escapes""#
        );
        assert_eq!(c(r#"c"é""#).unwrap(), "é".as_bytes());
        assert!(c(r#"c"\0""#).is_err());
        assert!(c(r#"c"\x00""#).is_err());
        assert!(c(r#"c"\u{0}""#).is_err());
        assert!(c(r#"c"\q""#).is_err());
        assert!(c(r#"c"\x4""#).is_err());
        assert!(c(r#"c"\u{110000}""#).is_err());
        assert!(c("c\"a\rb\"").is_err());
    }

    #[test]
    fn chars_and_bytes() {
        assert_eq!(unescape_char("'a'"), Ok('a'));
        assert_eq!(unescape_char("'é'"), Ok('é'));
        assert_eq!(unescape_char(r"'\n'"), Ok('\n'));
        assert_eq!(unescape_char(r"'\''"), Ok('\''));
        assert_eq!(unescape_char(r"'\x7f'"), Ok('\x7f'));
        assert_eq!(unescape_char(r"'\u{10FFFF}'"), Ok('\u{10ffff}'));
        let err = |r: Result<char, String>| r.unwrap_err();
        assert_eq!(err(unescape_char("''")), "empty character literal");
        assert_eq!(
            err(unescape_char("'ab'")),
            "character literal may only contain one codepoint"
        );
        assert_eq!(err(unescape_char(r"'\x80'")), "out of range hex escape");
        assert_eq!(err(unescape_char(r"'\q'")), "unknown character escape: `q`");
        assert_eq!(
            err(unescape_char(r"'\u{d800}'")),
            "invalid unicode character escape"
        );
        assert_eq!(err(unescape_char(r"'\u{}'")), "empty unicode escape");
        assert_eq!(
            err(unescape_char(r"'\u41'")),
            "incorrect unicode escape sequence"
        );
        assert_eq!(
            err(unescape_char(r"'\u{1234567}'")),
            "overlong unicode escape"
        );
        assert_eq!(
            err(unescape_char("'\t'")),
            "character constant must be escaped: `\\t`"
        );
        assert_eq!(unescape_byte("b'a'"), Ok(b'a'));
        assert_eq!(unescape_byte(r"b'\xff'"), Ok(0xff));
        assert_eq!(unescape_byte(r"b'\0'"), Ok(0));
        assert_eq!(
            unescape_byte("b'é'").unwrap_err(),
            "non-ASCII character in byte literal"
        );
        assert_eq!(
            unescape_byte(r"b'\u{41}'").unwrap_err(),
            "unicode escape in byte string"
        );
        assert_eq!(
            unescape_byte(r"b'\q'").unwrap_err(),
            "unknown byte escape: `q`"
        );
        assert_eq!(
            unescape_byte(r"b'\x8'").unwrap_err(),
            "numeric character escape is too short"
        );
        assert_eq!(
            unescape_byte(r"b'\xg0'").unwrap_err(),
            "invalid character in numeric character escape: `g`"
        );
    }

    #[test]
    fn strings() {
        assert_eq!(unescape_str(r#""a\tb\u{e9}""#).unwrap(), "a\tb\u{e9}");
        assert_eq!(unescape_str("\"a\\\n   \t b\"").unwrap(), "ab");
        assert_eq!(unescape_str(r###"r##"a"#b"##"###).unwrap(), "a\"#b");
        assert_eq!(unescape_str("\"é\"").unwrap(), "é");
        assert_eq!(
            unescape_str(r#""\x80""#).unwrap_err(),
            "out of range hex escape"
        );
        assert_eq!(unescape_byte_str(r#"b"a\xff\n""#).unwrap(), b"a\xff\n");
        assert_eq!(unescape_byte_str(r#"br"a\n""#).unwrap(), b"a\\n");
        assert_eq!(
            unescape_byte_str("b\"é\"").unwrap_err(),
            "non-ASCII character in byte string literal"
        );
        assert_eq!(
            unescape_byte_str("br\"é\"").unwrap_err(),
            "non-ASCII character in raw byte string literal"
        );
        assert_eq!(
            unescape_byte_str(r#"b"\u{41}""#).unwrap_err(),
            "unicode escape in byte string"
        );
    }
}

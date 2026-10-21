//! Values of literal tokens: integers and C strings.

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

/// The bytes of a C string literal, `c"..."` or `cr#"..."#`, without the
/// terminating NUL: escapes decoded, characters as UTF-8. The lexer has
/// already checked that the literal is terminated.
pub(crate) fn unescape_c_str(text: &str) -> Result<Vec<u8>, String> {
    let raw = text.starts_with("cr");
    let body = text.trim_start_matches(['c', 'r']);
    let hashes = body.len() - body.trim_start_matches('#').len();
    let body = &body[hashes..];
    let body = body
        .strip_prefix('"')
        .and_then(|b| b.strip_suffix(&"#".repeat(hashes)))
        .and_then(|b| b.strip_suffix('"'))
        .ok_or("unterminated C string")?;
    let mut bytes = Vec::with_capacity(body.len());
    let mut chars = body.chars().peekable();
    while let Some(c) = chars.next() {
        let c = match c {
            '\r' if chars.peek() == Some(&'\n') => continue,
            '\r' => return Err("bare CR not allowed in string, use `\\r` instead".to_owned()),
            '\\' if !raw => match chars.next() {
                Some('n') => '\n',
                Some('r') => '\r',
                Some('t') => '\t',
                Some('\\') => '\\',
                Some('0') => '\0',
                Some('\'') => '\'',
                Some('"') => '"',
                Some('x') => {
                    let hex: String = chars.by_ref().take(2).collect();
                    let byte = (hex.len() == 2)
                        .then(|| u8::from_str_radix(&hex, 16).ok())
                        .flatten()
                        .ok_or("invalid `\\x` escape: expected two hexadecimal digits")?;
                    if byte == 0 {
                        return Err(nul());
                    }
                    // `\x80` and above are single bytes, not characters.
                    bytes.push(byte);
                    continue;
                }
                Some('u') => unicode_escape(&mut chars)?,
                Some('\n') => {
                    // A line continuation: skip the newline and indentation.
                    while chars.peek().is_some_and(|c| c.is_ascii_whitespace()) {
                        chars.next();
                    }
                    continue;
                }
                Some(other) => return Err(format!("unknown character escape: `{other}`")),
                None => return Err("unterminated escape".to_owned()),
            },
            c => c,
        };
        if c == '\0' {
            return Err(nul());
        }
        let mut buffer = [0; 4];
        bytes.extend_from_slice(c.encode_utf8(&mut buffer).as_bytes());
    }
    Ok(bytes)
}

fn nul() -> String {
    "null characters in C string literals are not supported".to_owned()
}

/// `{1F600}` after `\u`.
fn unicode_escape(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Result<char, String> {
    let invalid =
        || "invalid unicode escape: expected `\\u{...}` with 1 to 6 hex digits".to_owned();
    if chars.next() != Some('{') {
        return Err(invalid());
    }
    let mut digits = String::new();
    loop {
        match chars.next() {
            Some('}') => break,
            Some('_') => {}
            Some(c) if c.is_ascii_hexdigit() && digits.len() < 6 => digits.push(c),
            _ => return Err(invalid()),
        }
    }
    let value = u32::from_str_radix(&digits, 16).map_err(|_| invalid())?;
    char::from_u32(value).ok_or_else(|| format!("invalid unicode character escape `{value:x}`"))
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
}

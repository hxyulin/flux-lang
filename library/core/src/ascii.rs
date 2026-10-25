//! ASCII methods of `u8` and `[u8]`, as Rust's `core` has them.

#[derive(Clone)]
pub struct EscapeDefault { bytes: [u8; 4], front: usize, back: usize }

pub fn escape_default(byte: u8) -> EscapeDefault {
    let escaped = match byte {
        b'\t' => b't', b'\r' => b'r', b'\n' => b'n',
        b'\\' => b'\\', b'\'' => b'\'', b'"' => b'"', _ => 0,
    };
    let (bytes, back) = if escaped != 0 { ([b'\\', escaped, 0, 0], 2) }
        else if byte >= 0x20 && byte <= 0x7e { ([byte, 0, 0, 0], 1) }
        else { ([b'\\', b'x', b"0123456789abcdef"[(byte >> 4) as usize], b"0123456789abcdef"[(byte & 15) as usize]], 4) };
    EscapeDefault { bytes, front: 0, back }
}
impl Iterator for EscapeDefault {
    type Item = u8;
    fn next(&mut self) -> Option<u8> {
        if self.front == self.back { None } else { let byte = self.bytes[self.front]; self.front += 1; Some(byte) }
    }
    fn size_hint(&self) -> (usize, Option<usize>) { let len = self.back - self.front; (len, Some(len)) }
}
impl DoubleEndedIterator for EscapeDefault {
    fn next_back(&mut self) -> Option<u8> {
        if self.front == self.back { None } else { self.back -= 1; Some(self.bytes[self.back]) }
    }
}
impl ExactSizeIterator for EscapeDefault {}
impl crate::iter::FusedIterator for EscapeDefault {}
impl crate::fmt::Display for EscapeDefault {
    fn fmt(&self, f: &mut crate::fmt::Formatter<'_>) -> crate::fmt::Result {
        for byte in self.clone() { crate::write!(f, "{}", byte as char)?; } Ok(())
    }
}

impl u8 {
    /// Whether the byte is ASCII: below 128.
    pub const fn is_ascii(&self) -> bool {
        *self < 128
    }

    /// `A` to `Z` and `a` to `z`.
    pub const fn is_ascii_alphabetic(&self) -> bool {
        match *self {
            b'A'..=b'Z' | b'a'..=b'z' => true,
            _ => false,
        }
    }

    /// `A` to `Z`.
    pub const fn is_ascii_uppercase(&self) -> bool {
        match *self {
            b'A'..=b'Z' => true,
            _ => false,
        }
    }

    /// `a` to `z`.
    pub const fn is_ascii_lowercase(&self) -> bool {
        match *self {
            b'a'..=b'z' => true,
            _ => false,
        }
    }

    /// `0` to `9`, `A` to `Z` and `a` to `z`.
    pub const fn is_ascii_alphanumeric(&self) -> bool {
        match *self {
            b'0'..=b'9' | b'A'..=b'Z' | b'a'..=b'z' => true,
            _ => false,
        }
    }

    /// `0` to `9`.
    pub const fn is_ascii_digit(&self) -> bool {
        match *self {
            b'0'..=b'9' => true,
            _ => false,
        }
    }

    /// `0` to `9`, `A` to `F` and `a` to `f`.
    pub const fn is_ascii_hexdigit(&self) -> bool {
        match *self {
            b'0'..=b'9' | b'A'..=b'F' | b'a'..=b'f' => true,
            _ => false,
        }
    }

    /// The printable characters other than letters, digits and space.
    pub const fn is_ascii_punctuation(&self) -> bool {
        match *self {
            b'!'..=b'/' | b':'..=b'@' | b'['..=b'`' | b'{'..=b'~' => true,
            _ => false,
        }
    }

    /// The printable characters other than space: `!` to `~`.
    pub const fn is_ascii_graphic(&self) -> bool {
        match *self {
            b'!'..=b'~' => true,
            _ => false,
        }
    }

    /// Space, tab, line feed, form feed and carriage return, as the
    /// WhatWG's Infra Standard says (not vertical tab).
    pub const fn is_ascii_whitespace(&self) -> bool {
        match *self {
            b'\t' | b'\n' | b'\x0C' | b'\r' | b' ' => true,
            _ => false,
        }
    }

    /// The control characters: below space, and delete.
    pub const fn is_ascii_control(&self) -> bool {
        match *self {
            b'\0'..=b'\x1F' | b'\x7F' => true,
            _ => false,
        }
    }

    /// The byte, with `a` to `z` made `A` to `Z`.
    pub const fn to_ascii_uppercase(&self) -> u8 {
        if self.is_ascii_lowercase() { *self - 32 } else { *self }
    }

    /// The byte, with `A` to `Z` made `a` to `z`.
    pub const fn to_ascii_lowercase(&self) -> u8 {
        if self.is_ascii_uppercase() { *self + 32 } else { *self }
    }

    /// Whether the bytes are equal ignoring ASCII case.
    pub const fn eq_ignore_ascii_case(&self, other: &u8) -> bool {
        self.to_ascii_lowercase() == other.to_ascii_lowercase()
    }

    /// Makes `a` to `z` into `A` to `Z`, in place.
    pub const fn make_ascii_uppercase(&mut self) {
        *self = self.to_ascii_uppercase();
    }

    /// Makes `A` to `Z` into `a` to `z`, in place.
    pub const fn make_ascii_lowercase(&mut self) {
        *self = self.to_ascii_lowercase();
    }
}

impl [u8] {
    pub fn escape_ascii(&self) -> crate::slice::EscapeAscii<'_> {
        crate::slice::EscapeAscii::new(self)
    }
    /// Whether every byte is ASCII.
    pub const fn is_ascii(&self) -> bool {
        let mut i = 0;
        while i < self.len() {
            if !self[i].is_ascii() {
                return false;
            }
            i += 1;
        }
        true
    }

    /// Whether the slices have the same length and bytes equal ignoring
    /// ASCII case.
    pub const fn eq_ignore_ascii_case(&self, other: &[u8]) -> bool {
        if self.len() != other.len() {
            return false;
        }
        let mut i = 0;
        while i < self.len() {
            if !self[i].eq_ignore_ascii_case(&other[i]) {
                return false;
            }
            i += 1;
        }
        true
    }

    /// Makes `a` to `z` into `A` to `Z`, in place.
    pub const fn make_ascii_uppercase(&mut self) {
        let mut i = 0;
        while i < self.len() {
            self[i].make_ascii_uppercase();
            i += 1;
        }
    }

    /// Makes `A` to `Z` into `a` to `z`, in place.
    pub const fn make_ascii_lowercase(&mut self) {
        let mut i = 0;
        while i < self.len() {
            self[i].make_ascii_lowercase();
            i += 1;
        }
    }
}

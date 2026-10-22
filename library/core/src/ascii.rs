//! ASCII methods of `u8` and `[u8]`, as Rust's `core` has them.

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

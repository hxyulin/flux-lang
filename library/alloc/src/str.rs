//! `str`'s methods that allocate.

pub use core::str::*;

use crate::string::String;

mod lowercase;

impl str {
    /// Returns Unicode default lowercase, including contextual final sigma.
    pub fn to_lowercase(&self) -> String { lowercase::convert(self) }

    /// Returns Unicode default uppercase, mapping each character on its own.
    pub fn to_uppercase(&self) -> String {
        let mut result = String::with_capacity(self.len());
        for c in self.chars() { result.extend(c.to_uppercase()); }
        result
    }

    /// Copies the string, converting ASCII uppercase bytes to lowercase.
    pub fn to_ascii_lowercase(&self) -> String {
        let mut bytes = self.as_bytes().to_vec();
        bytes.make_ascii_lowercase();
        // SAFETY: ASCII case conversion preserves every non-ASCII byte and
        // replaces ASCII bytes only with other single-byte UTF-8 characters.
        unsafe { String::from_utf8_unchecked(bytes) }
    }

    /// Copies the string, converting ASCII lowercase bytes to uppercase.
    pub fn to_ascii_uppercase(&self) -> String {
        let mut bytes = self.as_bytes().to_vec();
        bytes.make_ascii_uppercase();
        // SAFETY: ASCII case conversion preserves every non-ASCII byte and
        // replaces ASCII bytes only with other single-byte UTF-8 characters.
        unsafe { String::from_utf8_unchecked(bytes) }
    }

    /// The string, `n` times over.
    pub fn repeat(&self, n: usize) -> String {
        let mut s = String::with_capacity(self.len() * n);
        for _ in 0..n {
            s.push_str(self);
        }
        s
    }
}

impl str {
    pub fn replace<P: core::str::Pattern>(&self, from: P, to: &str) -> String {
        let mut output = String::new();
        let mut previous = 0;
        for (start, matched) in self.match_indices(from) {
            output.push_str(&self[previous..start]);
            output.push_str(to);
            previous = start + matched.len();
        }
        output.push_str(&self[previous..]);
        output
    }
}

impl str {
    pub fn replacen<P: core::str::Pattern>(&self, from: P, to: &str, count: usize) -> String {
        let mut output = String::new();
        let mut previous = 0;
        for (start, matched) in self.match_indices(from).take(count) {
            output.push_str(&self[previous..start]);
            output.push_str(to);
            previous = start + matched.len();
        }
        output.push_str(&self[previous..]);
        output
    }
}

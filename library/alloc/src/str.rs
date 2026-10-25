//! `str`'s methods that allocate.

use crate::string::String;

impl str {
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

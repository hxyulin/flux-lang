//! `str`'s methods that allocate.

use crate::string::String;

impl str {
    /// The string, `n` times over.
    pub fn repeat(&self, n: usize) -> String {
        let mut s = String::with_capacity(self.len() * n);
        for _ in 0..n {
            s.push_str(self);
        }
        s
    }
}

//! `String`: UTF-8 text on the heap, a `Vec<u8>` that holds only valid
//! UTF-8. It dereferences to `str`, so every `str` method works on it.

use core::cmp::Ordering;
use core::fmt;
use core::ops::{Add, AddAssign, Deref, DerefMut};

use crate::vec::Vec;

pub struct String {
    vec: Vec<u8>,
}

/// Bytes that are not UTF-8, given back.
pub struct FromUtf8Error {
    bytes: Vec<u8>,
    error: core::str::Utf8Error,
}

impl FromUtf8Error {
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn utf8_error(&self) -> core::str::Utf8Error {
        self.error
    }
}

impl fmt::Debug for FromUtf8Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("FromUtf8Error")
    }
}

impl String {
    /// An empty string, which allocates nothing yet.
    pub fn new() -> String {
        String { vec: Vec::new() }
    }

    /// An empty string with room for exactly `capacity` bytes.
    pub fn with_capacity(capacity: usize) -> String {
        String {
            vec: Vec::with_capacity(capacity),
        }
    }

    /// `vec` as a string, if it is UTF-8.
    pub fn from_utf8(vec: Vec<u8>) -> Result<String, FromUtf8Error> {
        match core::str::from_utf8(&vec) {
            Ok(_) => Ok(String { vec }),
            Err(error) => Err(FromUtf8Error { bytes: vec, error }),
        }
    }

    /// `bytes` as a string, which must be UTF-8.
    pub unsafe fn from_utf8_unchecked(bytes: Vec<u8>) -> String {
        String { vec: bytes }
    }

    pub fn as_str(&self) -> &str {
        self
    }

    pub fn as_mut_str(&mut self) -> &mut str {
        self
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.vec
    }

    /// Its bytes, as a vector.
    pub fn into_bytes(self) -> Vec<u8> {
        self.vec
    }

    /// Its bytes, which must stay UTF-8.
    pub unsafe fn as_mut_vec(&mut self) -> &mut Vec<u8> {
        &mut self.vec
    }

    /// Adds `string` at the end.
    pub fn push_str(&mut self, string: &str) {
        self.vec.extend_from_slice(string.as_bytes());
    }

    /// Adds `ch` at the end, as its UTF-8 bytes.
    pub fn push(&mut self, ch: char) {
        if ch.len_utf8() == 1 {
            self.vec.push(ch as u8);
        } else {
            let mut buf = [0u8; 4];
            self.vec.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
        }
    }

    /// Removes the last character.
    pub fn pop(&mut self) -> Option<char> {
        let ch = self.chars().next_back()?;
        let new_len = self.len() - ch.len_utf8();
        unsafe { self.vec.set_len(new_len) };
        Some(ch)
    }

    /// Removes the character at byte `idx`, which must start one.
    #[track_caller]
    pub fn remove(&mut self, idx: usize) -> char {
        let ch = match self[idx..].chars().next() {
            Some(ch) => ch,
            None => panic!("cannot remove a char from the end of a string"),
        };
        let next = idx + ch.len_utf8();
        let len = self.len();
        unsafe {
            let base = self.vec.as_mut_ptr();
            core::ptr::copy(base.add(next), base.add(idx), len - next);
            self.vec.set_len(len - (next - idx));
        }
        ch
    }

    /// Puts `ch` at byte `idx`, which must be a character boundary.
    #[track_caller]
    pub fn insert(&mut self, idx: usize, ch: char) {
        let mut buf = [0u8; 4];
        let bytes = ch.encode_utf8(&mut buf).as_bytes();
        self.insert_bytes(idx, bytes);
    }

    /// Puts `string` at byte `idx`, which must be a character boundary.
    #[track_caller]
    pub fn insert_str(&mut self, idx: usize, string: &str) {
        self.insert_bytes(idx, string.as_bytes());
    }

    #[track_caller]
    fn insert_bytes(&mut self, idx: usize, bytes: &[u8]) {
        if !self.is_char_boundary(idx) {
            panic!("assertion failed: self.is_char_boundary(idx)");
        }
        let len = self.len();
        let amt = bytes.len();
        self.vec.reserve(amt);
        unsafe {
            let base = self.vec.as_mut_ptr();
            core::ptr::copy(base.add(idx), base.add(idx + amt), len - idx);
            core::ptr::copy_nonoverlapping(bytes.as_ptr(), base.add(idx), amt);
            self.vec.set_len(len + amt);
        }
    }

    /// Shortens it to `new_len` bytes, a character boundary.
    #[track_caller]
    pub fn truncate(&mut self, new_len: usize) {
        if new_len <= self.len() {
            if !self.is_char_boundary(new_len) {
                panic!("assertion failed: self.is_char_boundary(new_len)");
            }
            self.vec.truncate(new_len);
        }
    }

    pub fn clear(&mut self) {
        self.vec.clear();
    }

    pub fn len(&self) -> usize {
        self.vec.len()
    }

    pub fn is_empty(&self) -> bool {
        self.vec.is_empty()
    }

    pub fn capacity(&self) -> usize {
        self.vec.capacity()
    }

    pub fn reserve(&mut self, additional: usize) {
        self.vec.reserve(additional);
    }

    pub fn reserve_exact(&mut self, additional: usize) {
        self.vec.reserve_exact(additional);
    }

    pub fn shrink_to_fit(&mut self) {
        self.vec.shrink_to_fit();
    }

    /// Splits off the bytes from `at` on into a new string.
    #[track_caller]
    pub fn split_off(&mut self, at: usize) -> String {
        if !self.is_char_boundary(at) {
            panic!("assertion failed: self.is_char_boundary(at)");
        }
        String {
            vec: self.vec.split_off(at),
        }
    }
}

impl Deref for String {
    type Target = str;

    fn deref(&self) -> &str {
        unsafe { core::str::from_utf8_unchecked(&self.vec) }
    }
}

impl DerefMut for String {
    fn deref_mut(&mut self) -> &mut str {
        unsafe { core::str::from_utf8_unchecked_mut(&mut self.vec) }
    }
}

impl Clone for String {
    fn clone(&self) -> String {
        String {
            vec: self.vec.clone(),
        }
    }
}

impl Default for String {
    fn default() -> String {
        String::new()
    }
}

impl fmt::Debug for String {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_str(), f)
    }
}

impl fmt::Display for String {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self.as_str(), f)
    }

    #[doc(hidden)]
    fn __to_string_hint(&self) -> fmt::ToStringHint {
        fmt::ToStringHint::Text(self.len())
    }
}

impl PartialEq for String {
    fn eq(&self, other: &String) -> bool {
        self.as_str() == other.as_str()
    }
}

impl Eq for String {}

impl PartialEq<str> for String {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl<'a> PartialEq<&'a str> for String {
    fn eq(&self, other: &&'a str) -> bool {
        self.as_str() == *other
    }
}

impl PartialEq<String> for str {
    fn eq(&self, other: &String) -> bool {
        self == other.as_str()
    }
}

impl<'a> PartialEq<String> for &'a str {
    fn eq(&self, other: &String) -> bool {
        *self == other.as_str()
    }
}

impl PartialOrd for String {
    fn partial_cmp(&self, other: &String) -> Option<Ordering> {
        Some(self.as_str().cmp(other.as_str()))
    }
}

impl Ord for String {
    fn cmp(&self, other: &String) -> Ordering {
        self.as_str().cmp(other.as_str())
    }
}

impl From<&str> for String {
    fn from(s: &str) -> String {
        let mut vec = Vec::with_capacity(s.len());
        vec.extend_from_slice(s.as_bytes());
        String { vec }
    }
}

impl From<&String> for String {
    fn from(s: &String) -> String {
        s.clone()
    }
}

impl From<char> for String {
    fn from(c: char) -> String {
        let mut s = String::new();
        s.push(c);
        s
    }
}

impl From<String> for Vec<u8> {
    fn from(s: String) -> Vec<u8> {
        s.vec
    }
}

impl<'a> Add<&'a str> for String {
    type Output = String;

    fn add(mut self, other: &'a str) -> String {
        self.push_str(other);
        self
    }
}

impl<'a> AddAssign<&'a str> for String {
    fn add_assign(&mut self, other: &'a str) {
        self.push_str(other);
    }
}

/// A value's text as a `String`, as `{}` writes it.
pub trait ToString {
    fn to_string(&self) -> String;
}

impl<T: fmt::Display + ?Sized> ToString for T {
    /// With the room rustc's specializations make first: exactly the
    /// text of a `str` or `String` (also behind references), a `char` or
    /// `bool`, the most digits of an integer.
    fn to_string(&self) -> String {
        let mut buf = match fmt::Display::__to_string_hint(self) {
            fmt::ToStringHint::Text(n) | fmt::ToStringHint::Capacity(n) => String::with_capacity(n),
            fmt::ToStringHint::None => String::new(),
        };
        let _ = fmt::Write::write_fmt(&mut buf, format_args!("{}", self));
        buf
    }
}

impl fmt::Write for String {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.push_str(s);
        Ok(())
    }

    fn write_char(&mut self, c: char) -> fmt::Result {
        self.push(c);
        Ok(())
    }
}

impl FromIterator<char> for String {
    fn from_iter<I: IntoIterator<Item = char>>(iter: I) -> String {
        let mut buf = String::new();
        buf.extend(iter);
        buf
    }
}

impl<'a> FromIterator<&'a char> for String {
    fn from_iter<I: IntoIterator<Item = &'a char>>(iter: I) -> String {
        let mut buf = String::new();
        buf.extend(iter);
        buf
    }
}

impl<'a> FromIterator<&'a str> for String {
    fn from_iter<I: IntoIterator<Item = &'a str>>(iter: I) -> String {
        let mut buf = String::new();
        buf.extend(iter);
        buf
    }
}

impl FromIterator<String> for String {
    fn from_iter<I: IntoIterator<Item = String>>(iter: I) -> String {
        let mut iterator = iter.into_iter();
        match iterator.next() {
            None => String::new(),
            Some(mut buf) => {
                buf.extend(iterator);
                buf
            }
        }
    }
}

impl Extend<char> for String {
    fn extend<I: IntoIterator<Item = char>>(&mut self, iter: I) {
        let iterator = iter.into_iter();
        let (lower, _) = iterator.size_hint();
        self.reserve(lower);
        for c in iterator {
            self.push(c);
        }
    }
}

impl<'a> Extend<&'a char> for String {
    fn extend<I: IntoIterator<Item = &'a char>>(&mut self, iter: I) {
        self.extend(iter.into_iter().copied());
    }
}

impl<'a> Extend<&'a str> for String {
    fn extend<I: IntoIterator<Item = &'a str>>(&mut self, iter: I) {
        for s in iter {
            self.push_str(s);
        }
    }
}

impl Extend<String> for String {
    fn extend<I: IntoIterator<Item = String>>(&mut self, iter: I) {
        for s in iter {
            self.push_str(&s);
        }
    }
}

impl core::borrow::Borrow<str> for String {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl core::borrow::BorrowMut<str> for String {
    fn borrow_mut(&mut self) -> &mut str {
        self.as_mut_str()
    }
}

impl core::hash::Hash for String {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        core::hash::Hash::hash(self.as_str(), state);
    }
}

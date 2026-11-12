//! `String`: UTF-8 text on the heap, a `Vec<u8>` that holds only valid
//! UTF-8. It dereferences to `str`, so every `str` method works on it.

use core::cmp::Ordering;
use core::fmt;
use core::ops::{Add, AddAssign, Deref, DerefMut};

use crate::vec::Vec;

pub struct String {
    vec: Vec<u8>,
}

impl AsRef<[u8]> for String {
    fn as_ref(&self) -> &[u8] {
        self.vec.as_slice()
    }
}

impl From<crate::boxed::Box<str>> for String {
    fn from(value: crate::boxed::Box<str>) -> Self {
        let bytes = crate::boxed::Box::into_raw(value) as *mut [u8];
        // SAFETY: str and [u8] share allocation layout and length metadata.
        // Ownership transfers once, and the bytes remain valid UTF-8.
        unsafe { Self::from_utf8_unchecked(crate::boxed::Box::from_raw(bytes).into_vec()) }
    }
}

impl core::convert::AsRef<str> for String {
    fn as_ref(&self) -> &str { self }
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

impl fmt::Display for FromUtf8Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.error, f)
    }
}
impl core::error::Error for FromUtf8Error {}

impl String {
    /// Replaces a byte range whose endpoints are UTF-8 character boundaries.
    #[track_caller]
    pub fn replace_range<R:core::ops::RangeBounds<usize>>(&mut self,range:R,replacement:&str) {
        use core::ops::Bound::{Included,Excluded,Unbounded};
        let start=match range.start_bound() {Included(&n)=>n,Excluded(&n)=>n.checked_add(1).expect("range start overflow"),Unbounded=>0};
        let end=match range.end_bound() {Included(&n)=>n.checked_add(1).expect("range end overflow"),Excluded(&n)=>n,Unbounded=>self.len()};
        assert!(start<=end && end<=self.len(),"string range out of bounds");
        assert!(self.is_char_boundary(start) && self.is_char_boundary(end),"string range is not on a character boundary");
        let capacity=(self.len()-(end-start)).checked_add(replacement.len()).expect("capacity overflow");
        let mut result=Self::with_capacity(capacity);
        result.push_str(&self[..start]);
        result.push_str(replacement);
        result.push_str(&self[end..]);
        *self=result;
    }

    pub fn into_boxed_str(self) -> crate::boxed::Box<str> {
        let bytes = self.into_bytes().into_boxed_slice();
        // SAFETY: String owns valid UTF-8, and str and [u8] have identical
        // allocation layout and length metadata. Ownership transfers once.
        unsafe { crate::boxed::Box::from_raw(crate::boxed::Box::into_raw(bytes) as *mut str) }
    }
    /// An empty string, which allocates nothing yet.
    pub const fn new() -> String {
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

    /// Borrows valid UTF-8 and replaces each invalid sequence with U+FFFD.
    pub fn from_utf8_lossy(v: &[u8]) -> crate::borrow::Cow<'_, str> {
        let first = match core::str::from_utf8(v) {
            Ok(text) => return crate::borrow::Cow::Borrowed(text),
            Err(error) => error,
        };
        let mut result = String::with_capacity(v.len());
        let mut rest = v;
        let mut error = first;
        loop {
            // Validate the prefix independently so no unchecked reference is
            // needed, even when the invalid bytes end in a partial sequence.
            let valid = error.valid_up_to();
            result.push_str(core::str::from_utf8(&rest[..valid]).unwrap());
            result.push('\u{fffd}');
            let Some(invalid) = error.error_len() else {
                break;
            };
            rest = &rest[valid + invalid..];
            match core::str::from_utf8(rest) {
                Ok(text) => {
                    result.push_str(text);
                    break;
                }
                Err(next) => error = next,
            }
        }
        crate::borrow::Cow::Owned(result)
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
    /// Keeps only the characters `f` accepts, in order.
    pub fn retain<F: FnMut(char) -> bool>(&mut self, mut f: F) {
        let mut kept = String::with_capacity(self.len());
        for c in self.chars() { if f(c) { kept.push(c); } }
        *self = kept;
    }

    /// Removes the range and returns its characters.
    ///
    /// # Panics
    /// Panics if the range is out of bounds or not on character boundaries.
    pub fn drain<R: core::ops::RangeBounds<usize>>(&mut self, range: R) -> Drain<'_> {
        let core::ops::Range { start, end } = core::slice::range(range, ..self.len());
        assert!(self.is_char_boundary(start));
        assert!(self.is_char_boundary(end));
        let removed = String::from(&self[start..end]);
        self.replace_range(start..end, "");
        Drain { chars: removed, front: 0, marker: core::marker::PhantomData }
    }

    /// Appends a copy of the given range of the string.
    pub fn extend_from_within<R: core::ops::RangeBounds<usize>>(&mut self, src: R) {
        let core::ops::Range { start, end } = core::slice::range(src, ..self.len());
        assert!(self.is_char_boundary(start));
        assert!(self.is_char_boundary(end));
        self.vec.extend_from_within(start..end);
    }

    /// Gives up the string's allocation, returning its text for 'a.
    pub fn leak<'a>(self) -> &'a mut str {
        let bytes = self.vec.leak();
        // SAFETY: the bytes were valid UTF-8 in the string.
        unsafe { core::str::from_utf8_unchecked_mut(bytes) }
    }

    /// Decodes UTF-16, failing on an unpaired surrogate.
    pub fn from_utf16(v: &[u16]) -> Result<String, FromUtf16Error> {
        let mut result = String::with_capacity(v.len());
        for c in core::char::decode_utf16(v.iter().copied()) {
            match c {
                Ok(c) => result.push(c),
                Err(_) => return Err(FromUtf16Error(())),
            }
        }
        Ok(result)
    }

    /// Decodes UTF-16, replacing each unpaired surrogate with U+FFFD.
    pub fn from_utf16_lossy(v: &[u16]) -> String {
        core::char::decode_utf16(v.iter().copied()).map(|r| r.unwrap_or(core::char::REPLACEMENT_CHARACTER)).collect()
    }

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

    /// Reserves space for `additional` bytes without aborting on allocation failure.
    pub fn try_reserve(&mut self, additional: usize) -> Result<(), crate::collections::TryReserveError> {
        self.vec.try_reserve(additional)
    }
    /// Reserves the minimum space needed for `additional` bytes.
    pub fn try_reserve_exact(&mut self, additional: usize) -> Result<(), crate::collections::TryReserveError> {
        self.vec.try_reserve_exact(additional)
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

impl core::str::Pattern for &String {
    fn next_match_in(&mut self, haystack: &str, lo: usize, hi: usize) -> Option<(usize, usize)> {
        core::str::Pattern::next_match_in(&mut self.as_str(), haystack, lo, hi)
    }
    fn next_match_back_in(&mut self, haystack: &str, lo: usize, hi: usize) -> Option<(usize, usize)> {
        core::str::Pattern::next_match_back_in(&mut self.as_str(), haystack, lo, hi)
    }
    fn prefix_len(&mut self, haystack: &str) -> Option<usize> {
        core::str::Pattern::prefix_len(&mut self.as_str(), haystack)
    }
    fn suffix_len(&mut self, haystack: &str) -> Option<usize> {
        core::str::Pattern::suffix_len(&mut self.as_str(), haystack)
    }
}

impl core::str::FromStr for String {
    type Err = core::convert::Infallible;
    fn from_str(s: &str) -> Result<String, core::convert::Infallible> { Ok(String::from(s)) }
}

/// The error from String::from_utf16: an unpaired surrogate.
#[derive(Debug)]
pub struct FromUtf16Error(());
impl fmt::Display for FromUtf16Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt("invalid utf-16: lone surrogate found", f)
    }
}
impl core::error::Error for FromUtf16Error {}

/// The characters removed by String::drain.
pub struct Drain<'a> { chars: String, front: usize, marker: core::marker::PhantomData<&'a mut String> }
impl Drain<'_> {
    pub fn as_str(&self) -> &str { &self.chars[self.front..] }
}
impl Iterator for Drain<'_> {
    type Item = char;
    fn next(&mut self) -> Option<char> {
        let c = self.chars[self.front..].chars().next()?;
        self.front += c.len_utf8();
        Some(c)
    }
    fn size_hint(&self) -> (usize, Option<usize>) { self.chars[self.front..].chars().size_hint() }
}
impl DoubleEndedIterator for Drain<'_> {
    fn next_back(&mut self) -> Option<char> {
        if self.front == self.chars.len() { return None; }
        let c = self.chars[self.front..].chars().next_back()?;
        let len = self.chars.len() - c.len_utf8();
        self.chars.truncate(len);
        Some(c)
    }
}
impl core::iter::FusedIterator for Drain<'_> {}
impl fmt::Debug for Drain<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.debug_tuple("Drain").field(&self.as_str()).finish() }
}

impl From<&mut str> for String {
    fn from(value: &mut str) -> String { String::from(&*value) }
}
impl<'a> Extend<crate::borrow::Cow<'a, str>> for String {
    fn extend<I: IntoIterator<Item = crate::borrow::Cow<'a, str>>>(&mut self, iter: I) {
        for s in iter { self.push_str(&s); }
    }
}
impl Extend<crate::boxed::Box<str>> for String {
    fn extend<I: IntoIterator<Item = crate::boxed::Box<str>>>(&mut self, iter: I) {
        for s in iter { self.push_str(&s); }
    }
}
impl<'a> FromIterator<crate::borrow::Cow<'a, str>> for String {
    fn from_iter<I: IntoIterator<Item = crate::borrow::Cow<'a, str>>>(iter: I) -> String {
        let mut s = String::new();
        s.extend(iter);
        s
    }
}
impl FromIterator<crate::boxed::Box<str>> for String {
    fn from_iter<I: IntoIterator<Item = crate::boxed::Box<str>>>(iter: I) -> String {
        let mut s = String::new();
        s.extend(iter);
        s
    }
}

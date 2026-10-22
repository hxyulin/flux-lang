//! `str`'s methods, its iterators and UTF-8 validation.
//!
//! A `str` is UTF-8 bytes behind a pointer with a length, as a `[u8]`
//! is, so a `&str` and a `&[u8]` convert by casting the pointer, as
//! Rust's `core` does; a part of a string (`&s[1..3]`, `split_at`,
//! `trim`) is a part of its bytes, checked to start and end at
//! characters.

use crate::clone::Clone;
use crate::cmp::Ordering::{self, Equal, Greater, Less};
use crate::cmp::{Eq, Ord, PartialEq, PartialOrd};
use crate::iter::{DoubleEndedIterator, ExactSizeIterator, Iterator};
use crate::marker::Copy;
use crate::ops::{FnMut, Index, Range, RangeFrom, RangeFull, RangeInclusive, RangeTo, RangeToInclusive};
use crate::option::Option::{self, None, Some};
use crate::panicking::panic;
use crate::result::Result::{self, Err, Ok};

/// The `str` the bytes are, if they are UTF-8.
pub const fn from_utf8(v: &[u8]) -> Result<&str, Utf8Error> {
    match validate_utf8(v) {
        Ok(()) => Ok(unsafe { from_utf8_unchecked(v) }),
        Err(e) => Err(e),
    }
}

/// The `str` the bytes are, which must be UTF-8.
pub const unsafe fn from_utf8_unchecked(v: &[u8]) -> &str {
    unsafe { &*(v as *const [u8] as *const str) }
}

/// The mutable `str` the bytes are, which must be UTF-8.
pub const unsafe fn from_utf8_unchecked_mut(v: &mut [u8]) -> &mut str {
    unsafe { &mut *(v as *mut [u8] as *mut str) }
}

/// Why bytes are not UTF-8: where the valid prefix ends, and how long
/// the invalid sequence after it is (`None` if the input ended inside
/// a character).
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Utf8Error {
    valid_up_to: usize,
    /// 0 for `None`.
    error_len: u8,
}

/// As rustc's derived impl writes it, with `error_len` an `Option`.
impl crate::fmt::Debug for Utf8Error {
    fn fmt(&self, f: &mut crate::fmt::Formatter<'_>) -> crate::fmt::Result {
        f.debug_struct("Utf8Error")
            .field("valid_up_to", &self.valid_up_to)
            .field("error_len", &self.error_len())
            .finish()
    }
}

impl crate::fmt::Display for Utf8Error {
    fn fmt(&self, f: &mut crate::fmt::Formatter<'_>) -> crate::fmt::Result {
        match self.error_len() {
            Some(len) => write!(
                f,
                "invalid utf-8 sequence of {} bytes from index {}",
                len, self.valid_up_to
            ),
            None => write!(f, "incomplete utf-8 byte sequence from index {}", self.valid_up_to),
        }
    }
}

impl Utf8Error {
    /// How many bytes from the start are valid UTF-8.
    pub const fn valid_up_to(&self) -> usize {
        self.valid_up_to
    }

    /// How many bytes after the valid ones are invalid, or `None` if the
    /// input ended in the middle of a character.
    pub const fn error_len(&self) -> Option<usize> {
        if self.error_len == 0 { None } else { Some(self.error_len as usize) }
    }
}

/// How many bytes a character starting with `first` takes: 0 if no
/// character starts with it.
const fn utf8_char_width(first: u8) -> usize {
    match first {
        0x00..=0x7F => 1,
        0xC2..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF4 => 4,
        _ => 0,
    }
}

/// Whether `byte` continues a character: `0b10xx_xxxx`.
const fn is_continuation(byte: u8) -> bool {
    (byte as i8) < -64
}

/// Checks that `v` is UTF-8, as Rust's `run_utf8_validation` does.
const fn validate_utf8(v: &[u8]) -> Result<(), Utf8Error> {
    let len = v.len();
    let mut index = 0;
    while index < len {
        let start = index;
        let first = v[index];
        if first >= 128 {
            let width = utf8_char_width(first);
            if width == 0 {
                return Err(Utf8Error { valid_up_to: start, error_len: 1 });
            }
            if index + 1 >= len {
                return Err(Utf8Error { valid_up_to: start, error_len: 0 });
            }
            let second = v[index + 1];
            let valid = match width {
                2 => is_continuation(second),
                3 => match (first, second) {
                    (0xE0, 0xA0..=0xBF)
                    | (0xE1..=0xEC, 0x80..=0xBF)
                    | (0xED, 0x80..=0x9F)
                    | (0xEE..=0xEF, 0x80..=0xBF) => true,
                    _ => false,
                },
                _ => match (first, second) {
                    (0xF0, 0x90..=0xBF) | (0xF1..=0xF3, 0x80..=0xBF) | (0xF4, 0x80..=0x8F) => true,
                    _ => false,
                },
            };
            if !valid {
                return Err(Utf8Error { valid_up_to: start, error_len: 1 });
            }
            index += 1;
            let mut seen = 2;
            while seen < width {
                if index + 1 >= len {
                    return Err(Utf8Error { valid_up_to: start, error_len: 0 });
                }
                if !is_continuation(v[index + 1]) {
                    return Err(Utf8Error { valid_up_to: start, error_len: seen as u8 });
                }
                index += 1;
                seen += 1;
            }
        }
        index += 1;
    }
    Ok(())
}

/// The character that starts at `v[at]`, valid UTF-8, and its width.
const fn decode_at(v: &[u8], at: usize) -> (char, usize) {
    let first = v[at] as u32;
    let (code, width) = if first < 0x80 {
        (first, 1)
    } else if first < 0xE0 {
        (((first & 0x1F) << 6) | (v[at + 1] as u32 & 0x3F), 2)
    } else if first < 0xF0 {
        let code = ((first & 0x0F) << 12)
            | ((v[at + 1] as u32 & 0x3F) << 6)
            | (v[at + 2] as u32 & 0x3F);
        (code, 3)
    } else {
        let code = ((first & 0x07) << 18)
            | ((v[at + 1] as u32 & 0x3F) << 12)
            | ((v[at + 2] as u32 & 0x3F) << 6)
            | (v[at + 3] as u32 & 0x3F);
        (code, 4)
    };
    (unsafe { char::from_u32_unchecked(code) }, width)
}

/// Whether `haystack` has `needle` at `at`.
const fn bytes_at(haystack: &[u8], at: usize, needle: &[u8]) -> bool {
    if at > haystack.len() || haystack.len() - at < needle.len() {
        return false;
    }
    let mut i = 0;
    while i < needle.len() {
        if haystack[at + i] != needle[i] {
            return false;
        }
        i += 1;
    }
    true
}

/// Where `needle` first occurs in `haystack`.
const fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    let mut at = 0;
    while at + needle.len() <= haystack.len() {
        if bytes_at(haystack, at, needle) {
            return Some(at);
        }
        at += 1;
    }
    None
}

/// `c`'s UTF-8 bytes, in the first `c.len_utf8()` of the array.
pub(crate) const fn encode_utf8(c: char) -> [u8; 4] {
    let code = c as u32;
    if code < 0x80 {
        [code as u8, 0, 0, 0]
    } else if code < 0x800 {
        [0xC0 | (code >> 6) as u8, 0x80 | (code & 0x3F) as u8, 0, 0]
    } else if code < 0x10000 {
        [
            0xE0 | (code >> 12) as u8,
            0x80 | ((code >> 6) & 0x3F) as u8,
            0x80 | (code & 0x3F) as u8,
            0,
        ]
    } else {
        [
            0xF0 | (code >> 18) as u8,
            0x80 | ((code >> 12) & 0x3F) as u8,
            0x80 | ((code >> 6) & 0x3F) as u8,
            0x80 | (code & 0x3F) as u8,
        ]
    }
}

/// What `find`, `split`, `trim_matches` and the other searching
/// methods look for: a `&str`, a `char`, an array or slice of `char`s
/// (any of them), or a closure `FnMut(char) -> bool` (the characters it
/// is `true` for). Rust's `Pattern` is unstable; programs only use its
/// implementations. A pattern is searched for between two byte offsets
/// at character boundaries, from either end.
pub trait Pattern: Sized {
    /// The first match within `haystack[lo..hi]`: its start and end.
    fn next_match_in(&mut self, haystack: &str, lo: usize, hi: usize) -> Option<(usize, usize)>;
    /// The last match within `haystack[lo..hi]`.
    fn next_match_back_in(&mut self, haystack: &str, lo: usize, hi: usize)
    -> Option<(usize, usize)>;
    /// How long the match `haystack` starts with is, if it starts with
    /// one.
    fn prefix_len(&mut self, haystack: &str) -> Option<usize>;
    /// How long the match `haystack` ends with is, if it ends with one.
    fn suffix_len(&mut self, haystack: &str) -> Option<usize>;
}

/// A pattern of single characters, whose matches are the same searched
/// for from either end: rustc's `DoubleEndedSearcher`, which
/// `trim_matches` and the back ends of `split` and `matches` need.
pub trait CharPattern: Pattern {}

/// The first match of `needle` within `haystack[lo..hi]`.
fn str_next_match(haystack: &str, needle: &str, lo: usize, hi: usize) -> Option<(usize, usize)> {
    let n = needle.len();
    if lo > hi || hi - lo < n {
        return None;
    }
    let (h, m) = (haystack.as_bytes(), needle.as_bytes());
    let mut at = lo;
    while at + n <= hi {
        if bytes_at(h, at, m) {
            return Some((at, at + n));
        }
        at += 1;
    }
    None
}

/// The last match of `needle` within `haystack[lo..hi]`.
fn str_next_match_back(
    haystack: &str,
    needle: &str,
    lo: usize,
    hi: usize,
) -> Option<(usize, usize)> {
    let n = needle.len();
    if lo > hi || hi - lo < n {
        return None;
    }
    let (h, m) = (haystack.as_bytes(), needle.as_bytes());
    let mut at = hi - n;
    loop {
        if bytes_at(h, at, m) {
            return Some((at, at + n));
        }
        if at == lo {
            return None;
        }
        at -= 1;
    }
}

/// The character that ends at `v[end]` (exclusive), valid UTF-8, and
/// where it starts.
fn decode_before(v: &[u8], end: usize) -> (char, usize) {
    let mut start = end - 1;
    while is_continuation(v[start]) {
        start -= 1;
    }
    (decode_at(v, start).0, start)
}

impl Pattern for &str {
    fn next_match_in(&mut self, haystack: &str, lo: usize, hi: usize) -> Option<(usize, usize)> {
        str_next_match(haystack, self, lo, hi)
    }

    fn next_match_back_in(
        &mut self,
        haystack: &str,
        lo: usize,
        hi: usize,
    ) -> Option<(usize, usize)> {
        str_next_match_back(haystack, self, lo, hi)
    }

    fn prefix_len(&mut self, haystack: &str) -> Option<usize> {
        if bytes_at(haystack.as_bytes(), 0, self.as_bytes()) { Some(self.len()) } else { None }
    }

    fn suffix_len(&mut self, haystack: &str) -> Option<usize> {
        let (h, n) = (haystack.as_bytes(), self.as_bytes());
        if n.len() <= h.len() && bytes_at(h, h.len() - n.len(), n) {
            Some(n.len())
        } else {
            None
        }
    }
}

impl Pattern for &&str {
    fn next_match_in(&mut self, haystack: &str, lo: usize, hi: usize) -> Option<(usize, usize)> {
        str_next_match(haystack, *self, lo, hi)
    }

    fn next_match_back_in(
        &mut self,
        haystack: &str,
        lo: usize,
        hi: usize,
    ) -> Option<(usize, usize)> {
        str_next_match_back(haystack, *self, lo, hi)
    }

    fn prefix_len(&mut self, haystack: &str) -> Option<usize> {
        let mut s: &str = *self;
        s.prefix_len(haystack)
    }

    fn suffix_len(&mut self, haystack: &str) -> Option<usize> {
        let mut s: &str = *self;
        s.suffix_len(haystack)
    }
}

impl Pattern for char {
    fn next_match_in(&mut self, haystack: &str, lo: usize, hi: usize) -> Option<(usize, usize)> {
        let bytes = encode_utf8(*self);
        let needle = unsafe { from_utf8_unchecked(&bytes[..self.len_utf8()]) };
        str_next_match(haystack, needle, lo, hi)
    }

    fn next_match_back_in(
        &mut self,
        haystack: &str,
        lo: usize,
        hi: usize,
    ) -> Option<(usize, usize)> {
        let bytes = encode_utf8(*self);
        let needle = unsafe { from_utf8_unchecked(&bytes[..self.len_utf8()]) };
        str_next_match_back(haystack, needle, lo, hi)
    }

    fn prefix_len(&mut self, haystack: &str) -> Option<usize> {
        match haystack.chars().next() {
            Some(c) if c == *self => Some(c.len_utf8()),
            _ => None,
        }
    }

    fn suffix_len(&mut self, haystack: &str) -> Option<usize> {
        match haystack.chars().next_back() {
            Some(c) if c == *self => Some(c.len_utf8()),
            _ => None,
        }
    }
}

impl CharPattern for char {}

/// Whether `c` is one of `set`.
fn char_in(set: &[char], c: char) -> bool {
    let mut i = 0;
    while i < set.len() {
        if set[i] == c {
            return true;
        }
        i += 1;
    }
    false
}

/// What a pattern of characters (a closure, or a set of characters)
/// says of one character.
trait CharEq {
    fn matches(&mut self, c: char) -> bool;
}

impl<F: FnMut(char) -> bool> CharEq for F {
    fn matches(&mut self, c: char) -> bool {
        self(c)
    }
}

/// `[char; N]` and the others, as a set.
struct CharSet<'a> {
    set: &'a [char],
}

impl CharEq for CharSet<'_> {
    fn matches(&mut self, c: char) -> bool {
        char_in(self.set, c)
    }
}

/// The first character within `haystack[lo..hi]` that `eq` matches.
fn char_eq_next<E: CharEq>(eq: &mut E, haystack: &str, lo: usize, hi: usize) -> Option<(usize, usize)> {
    let v = haystack.as_bytes();
    let mut at = lo;
    while at < hi {
        let (c, width) = decode_at(v, at);
        if eq.matches(c) {
            return Some((at, at + width));
        }
        at += width;
    }
    None
}

/// The last character within `haystack[lo..hi]` that `eq` matches.
fn char_eq_next_back<E: CharEq>(
    eq: &mut E,
    haystack: &str,
    lo: usize,
    hi: usize,
) -> Option<(usize, usize)> {
    let v = haystack.as_bytes();
    let mut end = hi;
    while end > lo {
        let (c, start) = decode_before(v, end);
        if eq.matches(c) {
            return Some((start, end));
        }
        end = start;
    }
    None
}

/// The length of the character `haystack` starts with, if `eq` matches
/// it.
fn char_eq_prefix<E: CharEq>(eq: &mut E, haystack: &str) -> Option<usize> {
    match haystack.chars().next() {
        Some(c) if eq.matches(c) => Some(c.len_utf8()),
        _ => None,
    }
}

/// The length of the character `haystack` ends with, if `eq` matches it.
fn char_eq_suffix<E: CharEq>(eq: &mut E, haystack: &str) -> Option<usize> {
    match haystack.chars().next_back() {
        Some(c) if eq.matches(c) => Some(c.len_utf8()),
        _ => None,
    }
}

impl<F: FnMut(char) -> bool> Pattern for F {
    fn next_match_in(&mut self, haystack: &str, lo: usize, hi: usize) -> Option<(usize, usize)> {
        char_eq_next(self, haystack, lo, hi)
    }

    fn next_match_back_in(
        &mut self,
        haystack: &str,
        lo: usize,
        hi: usize,
    ) -> Option<(usize, usize)> {
        char_eq_next_back(self, haystack, lo, hi)
    }

    fn prefix_len(&mut self, haystack: &str) -> Option<usize> {
        char_eq_prefix(self, haystack)
    }

    fn suffix_len(&mut self, haystack: &str) -> Option<usize> {
        char_eq_suffix(self, haystack)
    }
}

impl<F: FnMut(char) -> bool> CharPattern for F {}

impl<const N: usize> Pattern for [char; N] {
    fn next_match_in(&mut self, haystack: &str, lo: usize, hi: usize) -> Option<(usize, usize)> {
        char_eq_next(&mut CharSet { set: self }, haystack, lo, hi)
    }

    fn next_match_back_in(
        &mut self,
        haystack: &str,
        lo: usize,
        hi: usize,
    ) -> Option<(usize, usize)> {
        char_eq_next_back(&mut CharSet { set: self }, haystack, lo, hi)
    }

    fn prefix_len(&mut self, haystack: &str) -> Option<usize> {
        char_eq_prefix(&mut CharSet { set: self }, haystack)
    }

    fn suffix_len(&mut self, haystack: &str) -> Option<usize> {
        char_eq_suffix(&mut CharSet { set: self }, haystack)
    }
}

impl<const N: usize> CharPattern for [char; N] {}

impl<const N: usize> Pattern for &[char; N] {
    fn next_match_in(&mut self, haystack: &str, lo: usize, hi: usize) -> Option<(usize, usize)> {
        char_eq_next(&mut CharSet { set: *self }, haystack, lo, hi)
    }

    fn next_match_back_in(
        &mut self,
        haystack: &str,
        lo: usize,
        hi: usize,
    ) -> Option<(usize, usize)> {
        char_eq_next_back(&mut CharSet { set: *self }, haystack, lo, hi)
    }

    fn prefix_len(&mut self, haystack: &str) -> Option<usize> {
        char_eq_prefix(&mut CharSet { set: *self }, haystack)
    }

    fn suffix_len(&mut self, haystack: &str) -> Option<usize> {
        char_eq_suffix(&mut CharSet { set: *self }, haystack)
    }
}

impl<const N: usize> CharPattern for &[char; N] {}

impl Pattern for &[char] {
    fn next_match_in(&mut self, haystack: &str, lo: usize, hi: usize) -> Option<(usize, usize)> {
        char_eq_next(&mut CharSet { set: *self }, haystack, lo, hi)
    }

    fn next_match_back_in(
        &mut self,
        haystack: &str,
        lo: usize,
        hi: usize,
    ) -> Option<(usize, usize)> {
        char_eq_next_back(&mut CharSet { set: *self }, haystack, lo, hi)
    }

    fn prefix_len(&mut self, haystack: &str) -> Option<usize> {
        char_eq_prefix(&mut CharSet { set: *self }, haystack)
    }

    fn suffix_len(&mut self, haystack: &str) -> Option<usize> {
        char_eq_suffix(&mut CharSet { set: *self }, haystack)
    }
}

impl CharPattern for &[char] {}

impl str {
    /// The `str` the bytes are, if they are UTF-8.
    pub const fn from_utf8(v: &[u8]) -> Result<&str, Utf8Error> {
        from_utf8(v)
    }

    /// The `str` the bytes are, which must be UTF-8.
    pub const unsafe fn from_utf8_unchecked(v: &[u8]) -> &str {
        unsafe { from_utf8_unchecked(v) }
    }

    /// The length in bytes.
    pub const fn len(&self) -> usize {
        self.as_bytes().len()
    }

    /// Whether the string has no bytes.
    pub const fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The string's UTF-8 bytes.
    pub const fn as_bytes(&self) -> &[u8] {
        unsafe { &*(self as *const str as *const [u8]) }
    }

    /// A pointer to the first byte, for C: valid while the string is.
    pub const fn as_ptr(&self) -> *const u8 {
        self as *const str as *const u8
    }

    /// Whether `index` is the start or end of a character: 0, the
    /// length, or a byte that is not a continuation.
    pub const fn is_char_boundary(&self, index: usize) -> bool {
        if index == 0 {
            return true;
        }
        let bytes = self.as_bytes();
        if index >= bytes.len() {
            return index == bytes.len();
        }
        !is_continuation(bytes[index])
    }

    /// Whether every byte is ASCII.
    pub const fn is_ascii(&self) -> bool {
        self.as_bytes().is_ascii()
    }

    /// Whether the strings are equal ignoring ASCII case.
    pub const fn eq_ignore_ascii_case(&self, other: &str) -> bool {
        self.as_bytes().eq_ignore_ascii_case(other.as_bytes())
    }

    /// An iterator over the bytes.
    pub fn bytes(&self) -> Bytes<'_> {
        Bytes {
            bytes: self.as_bytes(),
            front: 0,
            back: self.len(),
        }
    }

    /// An iterator over the characters, decoded from UTF-8.
    pub fn chars(&self) -> Chars<'_> {
        Chars {
            bytes: self.as_bytes(),
            front: 0,
            back: self.len(),
        }
    }

    /// An iterator over the characters and the byte offsets they start
    /// at.
    pub fn char_indices(&self) -> CharIndices<'_> {
        CharIndices { chars: self.chars() }
    }

    /// Whether the string starts with a match of `pat`.
    pub fn starts_with<P: Pattern>(&self, mut pat: P) -> bool {
        pat.prefix_len(self).is_some()
    }

    /// Whether the string ends with a match of `pat`.
    pub fn ends_with<P: Pattern>(&self, mut pat: P) -> bool {
        pat.suffix_len(self).is_some()
    }

    /// Whether `pat` occurs in the string.
    pub fn contains<P: Pattern>(&self, mut pat: P) -> bool {
        pat.next_match_in(self, 0, self.len()).is_some()
    }

    /// The byte offset where `pat` first occurs.
    pub fn find<P: Pattern>(&self, mut pat: P) -> Option<usize> {
        match pat.next_match_in(self, 0, self.len()) {
            Some((start, _)) => Some(start),
            None => None,
        }
    }

    /// The byte offset where `pat` last occurs.
    pub fn rfind<P: Pattern>(&self, mut pat: P) -> Option<usize> {
        match pat.next_match_back_in(self, 0, self.len()) {
            Some((start, _)) => Some(start),
            None => None,
        }
    }

    /// The string without a match of `prefix` at its start, if it starts
    /// with one.
    pub fn strip_prefix<P: Pattern>(&self, mut prefix: P) -> Option<&str> {
        match prefix.prefix_len(self) {
            Some(n) => Some(substr(self, n, self.len())),
            None => None,
        }
    }

    /// The string without a match of `suffix` at its end, if it ends
    /// with one.
    pub fn strip_suffix<P: Pattern>(&self, mut suffix: P) -> Option<&str> {
        match suffix.suffix_len(self) {
            Some(n) => Some(substr(self, 0, self.len() - n)),
            None => None,
        }
    }

    /// The parts before and after the first match of `delimiter`.
    pub fn split_once<P: Pattern>(&self, mut delimiter: P) -> Option<(&str, &str)> {
        let (start, end) = delimiter.next_match_in(self, 0, self.len())?;
        Some((substr(self, 0, start), substr(self, end, self.len())))
    }

    /// The parts before and after the last match of `delimiter`.
    pub fn rsplit_once<P: Pattern>(&self, mut delimiter: P) -> Option<(&str, &str)> {
        let (start, end) = delimiter.next_match_back_in(self, 0, self.len())?;
        Some((substr(self, 0, start), substr(self, end, self.len())))
    }

    /// The string without the characters `pat` matches at either end.
    pub fn trim_matches<P: CharPattern>(&self, mut pat: P) -> &str {
        let mut start = 0;
        while let Some(n) = pat.prefix_len(substr(self, start, self.len())) {
            start += n;
        }
        let mut end = self.len();
        while end > start {
            match pat.suffix_len(substr(self, start, end)) {
                Some(n) => end -= n,
                None => break,
            }
        }
        substr(self, start, end)
    }

    /// The string without the matches of `pat` it starts with, one after
    /// another.
    pub fn trim_start_matches<P: Pattern>(&self, mut pat: P) -> &str {
        let mut start = 0;
        while let Some(n) = pat.prefix_len(substr(self, start, self.len())) {
            if n == 0 {
                break;
            }
            start += n;
        }
        substr(self, start, self.len())
    }

    /// The string without the matches of `pat` it ends with, one after
    /// another.
    pub fn trim_end_matches<P: Pattern>(&self, mut pat: P) -> &str {
        let mut end = self.len();
        while let Some(n) = pat.suffix_len(substr(self, 0, end)) {
            if n == 0 {
                break;
            }
            end -= n;
        }
        substr(self, 0, end)
    }

    /// The part of the string a range of byte offsets gives, if it is
    /// within it and at character boundaries.
    pub fn get<I: StrIndex>(&self, index: I) -> Option<&str> {
        index.get(self)
    }

    /// The string split at byte `mid`, which must be a character
    /// boundary (or this panics).
    #[track_caller]
    pub fn split_at(&self, mid: usize) -> (&str, &str) {
        if !self.is_char_boundary(mid) {
            panic("failed to slice string")
        }
        (substr(self, 0, mid), substr(self, mid, self.len()))
    }

    /// The string without leading whitespace.
    pub fn trim_start(&self) -> &str {
        let mut iter = self.char_indices();
        while let Some((at, c)) = iter.next() {
            if !c.is_whitespace() {
                return substr(self, at, self.len());
            }
        }
        substr(self, self.len(), self.len())
    }

    /// The string without trailing whitespace.
    pub fn trim_end(&self) -> &str {
        let mut iter = self.char_indices();
        while let Some((at, c)) = iter.next_back() {
            if !c.is_whitespace() {
                return substr(self, 0, at + c.len_utf8());
            }
        }
        substr(self, 0, 0)
    }

    /// The string without leading and trailing whitespace.
    pub fn trim(&self) -> &str {
        self.trim_start().trim_end()
    }

    /// The parts of the string between the matches of `pat`.
    pub fn split<P: Pattern>(&self, pat: P) -> Split<'_, P> {
        Split { inner: SplitInternal::new(self, pat, true) }
    }

    /// The parts of the string between the matches of `pat`, from the
    /// end.
    pub fn rsplit<P: Pattern>(&self, pat: P) -> RSplit<'_, P> {
        RSplit { inner: SplitInternal::new(self, pat, true) }
    }

    /// The parts of the string between the matches of `pat`, each ending
    /// with its match, as `split` gives them but without an empty last
    /// one.
    pub fn split_terminator<P: Pattern>(&self, pat: P) -> SplitTerminator<'_, P> {
        SplitTerminator { inner: SplitInternal::new(self, pat, false) }
    }

    /// The parts of the string between the matches of `pat`, each with
    /// the match that ends it.
    pub fn split_inclusive<P: Pattern>(&self, pat: P) -> SplitInclusive<'_, P> {
        SplitInclusive { inner: SplitInternal::new(self, pat, false) }
    }

    /// At most `n` parts of the string between the matches of `pat`, the
    /// last one the rest of it.
    pub fn splitn<P: Pattern>(&self, n: usize, pat: P) -> SplitN<'_, P> {
        SplitN { inner: SplitInternal::new(self, pat, true), count: n }
    }

    /// At most `n` parts of the string between the matches of `pat`,
    /// from the end, the last one the rest of it.
    pub fn rsplitn<P: Pattern>(&self, n: usize, pat: P) -> RSplitN<'_, P> {
        RSplitN { inner: SplitInternal::new(self, pat, true), count: n }
    }

    /// The matches of `pat`.
    pub fn matches<P: Pattern>(&self, pat: P) -> Matches<'_, P> {
        Matches { inner: MatchesInternal::new(self, pat) }
    }

    /// The matches of `pat` from the end.
    pub fn rmatches<P: Pattern>(&self, pat: P) -> RMatches<'_, P> {
        RMatches { inner: MatchesInternal::new(self, pat) }
    }

    /// The matches of `pat` and the byte offsets they start at.
    pub fn match_indices<P: Pattern>(&self, pat: P) -> MatchIndices<'_, P> {
        MatchIndices { inner: MatchesInternal::new(self, pat) }
    }

    /// The matches of `pat` and the byte offsets they start at, from the
    /// end.
    pub fn rmatch_indices<P: Pattern>(&self, pat: P) -> RMatchIndices<'_, P> {
        RMatchIndices { inner: MatchesInternal::new(self, pat) }
    }

    /// The lines of the string, without their `\n` or `\r\n`; a final
    /// line ending ends the last line rather than starting an empty one.
    pub fn lines(&self) -> Lines<'_> {
        Lines { rest: self }
    }

    /// The parts of the string between runs of whitespace.
    pub fn split_whitespace(&self) -> SplitWhitespace<'_> {
        SplitWhitespace { rest: self }
    }
}

/// What `str::get` takes: a range of byte offsets. rustc's
/// `SliceIndex<str>`.
#[diagnostic::on_unimplemented(message = "the type `str` cannot be indexed by `{Self}`")]
pub trait StrIndex {
    /// The part of `s` it gives, if it is within it and at character
    /// boundaries.
    fn get(self, s: &str) -> Option<&str>;
}

/// `&s[start..end]`, if both are within it and at character boundaries.
fn get_substr(s: &str, start: usize, end: usize) -> Option<&str> {
    if start <= end && s.is_char_boundary(start) && s.is_char_boundary(end) {
        Some(unsafe { from_utf8_unchecked(&s.as_bytes()[start..end]) })
    } else {
        None
    }
}

impl StrIndex for Range<usize> {
    fn get(self, s: &str) -> Option<&str> {
        get_substr(s, self.start, self.end)
    }
}

impl StrIndex for RangeTo<usize> {
    fn get(self, s: &str) -> Option<&str> {
        get_substr(s, 0, self.end)
    }
}

impl StrIndex for RangeFrom<usize> {
    fn get(self, s: &str) -> Option<&str> {
        get_substr(s, self.start, s.len())
    }
}

impl StrIndex for RangeFull {
    fn get(self, s: &str) -> Option<&str> {
        Some(s)
    }
}

impl StrIndex for RangeInclusive<usize> {
    fn get(self, s: &str) -> Option<&str> {
        if self.end == usize::MAX {
            return None;
        }
        let end = self.end + 1;
        let start = if self.exhausted { end } else { self.start };
        get_substr(s, start, end)
    }
}

impl StrIndex for RangeToInclusive<usize> {
    fn get(self, s: &str) -> Option<&str> {
        if self.end == usize::MAX { None } else { get_substr(s, 0, self.end + 1) }
    }
}

/// `&s[start..end]`, which panics, as rustc's does, unless both are
/// within the string and at character boundaries.
#[track_caller]
fn substr(s: &str, start: usize, end: usize) -> &str {
    if start > end || !s.is_char_boundary(start) || !s.is_char_boundary(end) {
        slice_error_fail(s, start, end)
    }
    unsafe { from_utf8_unchecked(crate::intrinsics::subslice(s.as_bytes(), start, end)) }
}

/// rustc's messages for a range not within a string, or not at its
/// characters' boundaries.
#[track_caller]
fn slice_error_fail(s: &str, begin: usize, end: usize) -> ! {
    let len = s.len();
    if begin > len {
        crate::panicking::panic_fmt(format_args!(
            "start byte index {begin} is out of bounds for string of length {len}"
        ))
    }
    if end > len {
        crate::panicking::panic_fmt(format_args!(
            "end byte index {end} is out of bounds for string of length {len}"
        ))
    }
    if begin > end {
        crate::panicking::panic_fmt(format_args!("byte range starts at {begin} but ends at {end}"))
    }
    let (which, index) = if !s.is_char_boundary(begin) { ("start", begin) } else { ("end", end) };
    let mut floor = index;
    while !s.is_char_boundary(floor) {
        floor -= 1;
    }
    let mut ceil = index;
    while !s.is_char_boundary(ceil) {
        ceil += 1;
    }
    let ch = crate::fmt::decode_char(s.as_bytes(), floor);
    let range = floor..ceil;
    crate::panicking::panic_fmt(format_args!(
        "{which} byte index {index} is not a char boundary; it is inside {ch:?} (bytes {range:?} of string)"
    ))
}

impl Index<Range<usize>> for str {
    type Output = str;

    #[track_caller]
    fn index(&self, index: Range<usize>) -> &str {
        substr(self, index.start, index.end)
    }
}

impl Index<RangeTo<usize>> for str {
    type Output = str;

    #[track_caller]
    fn index(&self, index: RangeTo<usize>) -> &str {
        substr(self, 0, index.end)
    }
}

impl Index<RangeFrom<usize>> for str {
    type Output = str;

    #[track_caller]
    fn index(&self, index: RangeFrom<usize>) -> &str {
        substr(self, index.start, self.len())
    }
}

impl Index<RangeFull> for str {
    type Output = str;

    #[track_caller]
    fn index(&self, _index: RangeFull) -> &str {
        self
    }
}

impl Index<RangeInclusive<usize>> for str {
    type Output = str;

    #[track_caller]
    fn index(&self, index: RangeInclusive<usize>) -> &str {
        if index.end == usize::MAX {
            panic("attempted to index str up to maximum usize")
        }
        let start = if index.exhausted { index.end + 1 } else { index.start };
        substr(self, start, index.end + 1)
    }
}

impl Index<RangeToInclusive<usize>> for str {
    type Output = str;

    #[track_caller]
    fn index(&self, index: RangeToInclusive<usize>) -> &str {
        if index.end == usize::MAX {
            panic("attempted to index str up to maximum usize")
        }
        substr(self, 0, index.end + 1)
    }
}

/// A pattern searched for in a string from both ends, as rustc's
/// `Searcher`s do: what is left to search is `haystack[front..back]`.
struct Searcher<'a, P> {
    haystack: &'a str,
    pat: P,
    front: usize,
    back: usize,
    /// Set once an empty match at an end leaves nothing to search.
    done: bool,
}

impl<'a, P: Pattern> Searcher<'a, P> {
    fn new(haystack: &'a str, pat: P) -> Searcher<'a, P> {
        Searcher { haystack, pat, front: 0, back: haystack.len(), done: false }
    }

    /// The next match from the front. After an empty one, the search
    /// goes on past the character after it, as rustc's does.
    fn next_match(&mut self) -> Option<(usize, usize)> {
        if self.done {
            return None;
        }
        let (a, b) = self.pat.next_match_in(self.haystack, self.front, self.back)?;
        if a != b {
            self.front = b;
        } else if b < self.back {
            self.front = b + decode_at(self.haystack.as_bytes(), b).1;
        } else {
            self.done = true;
        }
        Some((a, b))
    }

    /// The next match from the back.
    fn next_match_back(&mut self) -> Option<(usize, usize)> {
        if self.done {
            return None;
        }
        let (a, b) = self.pat.next_match_back_in(self.haystack, self.front, self.back)?;
        if a != b {
            self.back = a;
        } else if a > self.front {
            self.back = decode_before(self.haystack.as_bytes(), a).1;
        } else {
            self.done = true;
        }
        Some((a, b))
    }
}

impl<'a, P: Clone> Clone for Searcher<'a, P> {
    fn clone(&self) -> Searcher<'a, P> {
        Searcher {
            haystack: self.haystack,
            pat: self.pat.clone(),
            front: self.front,
            back: self.back,
            done: self.done,
        }
    }
}

/// The parts of a string between a pattern's matches, from either end:
/// rustc's `SplitInternal`, which `split` and its relatives are made of.
struct SplitInternal<'a, P> {
    searcher: Searcher<'a, P>,
    /// Where the part the front gives next starts.
    start: usize,
    /// Where the part the back gives next ends.
    end: usize,
    /// Whether an empty last part is given.
    allow_trailing_empty: bool,
    finished: bool,
}

impl<'a, P: Pattern> SplitInternal<'a, P> {
    fn new(haystack: &'a str, pat: P, allow_trailing_empty: bool) -> SplitInternal<'a, P> {
        SplitInternal {
            searcher: Searcher::new(haystack, pat),
            start: 0,
            end: haystack.len(),
            allow_trailing_empty,
            finished: false,
        }
    }

    /// What is left between the parts given, once.
    fn get_end(&mut self) -> Option<&'a str> {
        if !self.finished {
            self.finished = true;
            if self.allow_trailing_empty || self.end > self.start {
                return Some(substr(self.searcher.haystack, self.start, self.end));
            }
        }
        None
    }

    fn next(&mut self) -> Option<&'a str> {
        if self.finished {
            return None;
        }
        match self.searcher.next_match() {
            Some((a, b)) => {
                let part = substr(self.searcher.haystack, self.start, a);
                self.start = b;
                Some(part)
            }
            None => self.get_end(),
        }
    }

    fn next_inclusive(&mut self) -> Option<&'a str> {
        if self.finished {
            return None;
        }
        match self.searcher.next_match() {
            Some((_, b)) => {
                let part = substr(self.searcher.haystack, self.start, b);
                self.start = b;
                Some(part)
            }
            None => self.get_end(),
        }
    }

    fn next_back(&mut self) -> Option<&'a str> {
        if self.finished {
            return None;
        }
        if !self.allow_trailing_empty {
            self.allow_trailing_empty = true;
            match self.next_back() {
                Some(part) if !part.is_empty() => return Some(part),
                _ => {
                    if self.finished {
                        return None;
                    }
                }
            }
        }
        match self.searcher.next_match_back() {
            Some((a, b)) => {
                let part = substr(self.searcher.haystack, b, self.end);
                self.end = a;
                Some(part)
            }
            None => {
                self.finished = true;
                Some(substr(self.searcher.haystack, self.start, self.end))
            }
        }
    }

    fn next_back_inclusive(&mut self) -> Option<&'a str> {
        if self.finished {
            return None;
        }
        if !self.allow_trailing_empty {
            self.allow_trailing_empty = true;
            match self.next_back_inclusive() {
                Some(part) if !part.is_empty() => return Some(part),
                _ => {
                    if self.finished {
                        return None;
                    }
                }
            }
        }
        match self.searcher.next_match_back() {
            Some((_, b)) => {
                let part = substr(self.searcher.haystack, b, self.end);
                self.end = b;
                Some(part)
            }
            None => {
                self.finished = true;
                Some(substr(self.searcher.haystack, self.start, self.end))
            }
        }
    }
}

impl<'a, P: Clone> Clone for SplitInternal<'a, P> {
    fn clone(&self) -> SplitInternal<'a, P> {
        SplitInternal {
            searcher: self.searcher.clone(),
            start: self.start,
            end: self.end,
            allow_trailing_empty: self.allow_trailing_empty,
            finished: self.finished,
        }
    }
}

/// An iterator over the parts of a string between the matches of a
/// pattern: what [`str::split`] gives.
pub struct Split<'a, P> {
    inner: SplitInternal<'a, P>,
}

impl<'a, P: Pattern> Iterator for Split<'a, P> {
    type Item = &'a str;

    fn next(&mut self) -> Option<&'a str> {
        self.inner.next()
    }
}

impl<'a, P: CharPattern> DoubleEndedIterator for Split<'a, P> {
    fn next_back(&mut self) -> Option<&'a str> {
        self.inner.next_back()
    }
}

impl<'a, P: Clone> Clone for Split<'a, P> {
    fn clone(&self) -> Split<'a, P> {
        Split { inner: self.inner.clone() }
    }
}

/// [`str::rsplit`]'s iterator: [`Split`]'s parts from the end.
pub struct RSplit<'a, P> {
    inner: SplitInternal<'a, P>,
}

impl<'a, P: Pattern> Iterator for RSplit<'a, P> {
    type Item = &'a str;

    fn next(&mut self) -> Option<&'a str> {
        self.inner.next_back()
    }
}

impl<'a, P: CharPattern> DoubleEndedIterator for RSplit<'a, P> {
    fn next_back(&mut self) -> Option<&'a str> {
        self.inner.next()
    }
}

impl<'a, P: Clone> Clone for RSplit<'a, P> {
    fn clone(&self) -> RSplit<'a, P> {
        RSplit { inner: self.inner.clone() }
    }
}

/// [`str::split_terminator`]'s iterator: [`Split`]'s parts without an
/// empty last one.
pub struct SplitTerminator<'a, P> {
    inner: SplitInternal<'a, P>,
}

impl<'a, P: Pattern> Iterator for SplitTerminator<'a, P> {
    type Item = &'a str;

    fn next(&mut self) -> Option<&'a str> {
        self.inner.next()
    }
}

impl<'a, P: CharPattern> DoubleEndedIterator for SplitTerminator<'a, P> {
    fn next_back(&mut self) -> Option<&'a str> {
        self.inner.next_back()
    }
}

impl<'a, P: Clone> Clone for SplitTerminator<'a, P> {
    fn clone(&self) -> SplitTerminator<'a, P> {
        SplitTerminator { inner: self.inner.clone() }
    }
}

/// [`str::split_inclusive`]'s iterator: parts that end with the match
/// that ends them.
pub struct SplitInclusive<'a, P> {
    inner: SplitInternal<'a, P>,
}

impl<'a, P: Pattern> Iterator for SplitInclusive<'a, P> {
    type Item = &'a str;

    fn next(&mut self) -> Option<&'a str> {
        self.inner.next_inclusive()
    }
}

impl<'a, P: CharPattern> DoubleEndedIterator for SplitInclusive<'a, P> {
    fn next_back(&mut self) -> Option<&'a str> {
        self.inner.next_back_inclusive()
    }
}

impl<'a, P: Clone> Clone for SplitInclusive<'a, P> {
    fn clone(&self) -> SplitInclusive<'a, P> {
        SplitInclusive { inner: self.inner.clone() }
    }
}

/// [`str::splitn`]'s iterator.
pub struct SplitN<'a, P> {
    inner: SplitInternal<'a, P>,
    count: usize,
}

impl<'a, P: Pattern> Iterator for SplitN<'a, P> {
    type Item = &'a str;

    fn next(&mut self) -> Option<&'a str> {
        match self.count {
            0 => None,
            1 => {
                self.count = 0;
                self.inner.get_end()
            }
            _ => {
                self.count -= 1;
                self.inner.next()
            }
        }
    }
}

impl<'a, P: Clone> Clone for SplitN<'a, P> {
    fn clone(&self) -> SplitN<'a, P> {
        SplitN { inner: self.inner.clone(), count: self.count }
    }
}

/// [`str::rsplitn`]'s iterator.
pub struct RSplitN<'a, P> {
    inner: SplitInternal<'a, P>,
    count: usize,
}

impl<'a, P: Pattern> Iterator for RSplitN<'a, P> {
    type Item = &'a str;

    fn next(&mut self) -> Option<&'a str> {
        match self.count {
            0 => None,
            1 => {
                self.count = 0;
                self.inner.get_end()
            }
            _ => {
                self.count -= 1;
                self.inner.next_back()
            }
        }
    }
}

impl<'a, P: Clone> Clone for RSplitN<'a, P> {
    fn clone(&self) -> RSplitN<'a, P> {
        RSplitN { inner: self.inner.clone(), count: self.count }
    }
}

/// A pattern's matches in a string, from either end.
struct MatchesInternal<'a, P> {
    searcher: Searcher<'a, P>,
}

impl<'a, P: Pattern> MatchesInternal<'a, P> {
    fn new(haystack: &'a str, pat: P) -> MatchesInternal<'a, P> {
        MatchesInternal { searcher: Searcher::new(haystack, pat) }
    }

    fn next(&mut self) -> Option<(usize, &'a str)> {
        let (a, b) = self.searcher.next_match()?;
        Some((a, substr(self.searcher.haystack, a, b)))
    }

    fn next_back(&mut self) -> Option<(usize, &'a str)> {
        let (a, b) = self.searcher.next_match_back()?;
        Some((a, substr(self.searcher.haystack, a, b)))
    }
}

impl<'a, P: Clone> Clone for MatchesInternal<'a, P> {
    fn clone(&self) -> MatchesInternal<'a, P> {
        MatchesInternal { searcher: self.searcher.clone() }
    }
}

/// [`str::matches`]'s iterator.
pub struct Matches<'a, P> {
    inner: MatchesInternal<'a, P>,
}

impl<'a, P: Pattern> Iterator for Matches<'a, P> {
    type Item = &'a str;

    fn next(&mut self) -> Option<&'a str> {
        match self.inner.next() {
            Some((_, m)) => Some(m),
            None => None,
        }
    }
}

impl<'a, P: CharPattern> DoubleEndedIterator for Matches<'a, P> {
    fn next_back(&mut self) -> Option<&'a str> {
        match self.inner.next_back() {
            Some((_, m)) => Some(m),
            None => None,
        }
    }
}

impl<'a, P: Clone> Clone for Matches<'a, P> {
    fn clone(&self) -> Matches<'a, P> {
        Matches { inner: self.inner.clone() }
    }
}

/// [`str::rmatches`]'s iterator.
pub struct RMatches<'a, P> {
    inner: MatchesInternal<'a, P>,
}

impl<'a, P: Pattern> Iterator for RMatches<'a, P> {
    type Item = &'a str;

    fn next(&mut self) -> Option<&'a str> {
        match self.inner.next_back() {
            Some((_, m)) => Some(m),
            None => None,
        }
    }
}

impl<'a, P: CharPattern> DoubleEndedIterator for RMatches<'a, P> {
    fn next_back(&mut self) -> Option<&'a str> {
        match self.inner.next() {
            Some((_, m)) => Some(m),
            None => None,
        }
    }
}

impl<'a, P: Clone> Clone for RMatches<'a, P> {
    fn clone(&self) -> RMatches<'a, P> {
        RMatches { inner: self.inner.clone() }
    }
}

/// [`str::match_indices`]'s iterator.
pub struct MatchIndices<'a, P> {
    inner: MatchesInternal<'a, P>,
}

impl<'a, P: Pattern> Iterator for MatchIndices<'a, P> {
    type Item = (usize, &'a str);

    fn next(&mut self) -> Option<(usize, &'a str)> {
        self.inner.next()
    }
}

impl<'a, P: CharPattern> DoubleEndedIterator for MatchIndices<'a, P> {
    fn next_back(&mut self) -> Option<(usize, &'a str)> {
        self.inner.next_back()
    }
}

impl<'a, P: Clone> Clone for MatchIndices<'a, P> {
    fn clone(&self) -> MatchIndices<'a, P> {
        MatchIndices { inner: self.inner.clone() }
    }
}

/// [`str::rmatch_indices`]'s iterator.
pub struct RMatchIndices<'a, P> {
    inner: MatchesInternal<'a, P>,
}

impl<'a, P: Pattern> Iterator for RMatchIndices<'a, P> {
    type Item = (usize, &'a str);

    fn next(&mut self) -> Option<(usize, &'a str)> {
        self.inner.next_back()
    }
}

impl<'a, P: CharPattern> DoubleEndedIterator for RMatchIndices<'a, P> {
    fn next_back(&mut self) -> Option<(usize, &'a str)> {
        self.inner.next()
    }
}

impl<'a, P: Clone> Clone for RMatchIndices<'a, P> {
    fn clone(&self) -> RMatchIndices<'a, P> {
        RMatchIndices { inner: self.inner.clone() }
    }
}

/// An iterator over a string's lines: what [`str::lines`] gives.
pub struct Lines<'a> {
    rest: &'a str,
}

impl<'a> Iterator for Lines<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<&'a str> {
        let len = self.rest.len();
        if len == 0 {
            return None;
        }
        let (line, rest) = match find_bytes(self.rest.as_bytes(), b"\n") {
            Some(i) => {
                let end = if i > 0 && self.rest.as_bytes()[i - 1] == b'\r' { i - 1 } else { i };
                (substr(self.rest, 0, end), substr(self.rest, i + 1, len))
            }
            None => (self.rest, substr(self.rest, len, len)),
        };
        self.rest = rest;
        Some(line)
    }
}

/// An iterator over the parts of a string between runs of whitespace:
/// what [`str::split_whitespace`] gives.
pub struct SplitWhitespace<'a> {
    rest: &'a str,
}

impl<'a> Iterator for SplitWhitespace<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<&'a str> {
        let all = self.rest;
        let rest = all.trim_start();
        if rest.is_empty() {
            self.rest = rest;
            return None;
        }
        let mut iter = rest.char_indices();
        while let Some((at, c)) = iter.next() {
            if c.is_whitespace() {
                self.rest = substr(rest, at, rest.len());
                return Some(substr(rest, 0, at));
            }
        }
        self.rest = substr(rest, rest.len(), rest.len());
        Some(rest)
    }
}

/// An iterator over a string's bytes: what [`str::bytes`] gives.
pub struct Bytes<'a> {
    bytes: &'a [u8],
    front: usize,
    back: usize,
}

impl<'a> Iterator for Bytes<'a> {
    type Item = u8;

    fn next(&mut self) -> Option<u8> {
        if self.front < self.back {
            self.front += 1;
            Some(self.bytes[self.front - 1])
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let n = self.back - self.front;
        (n, Some(n))
    }
}

impl<'a> ExactSizeIterator for Bytes<'a> {}

impl<'a> Clone for Bytes<'a> {
    fn clone(&self) -> Bytes<'a> {
        Bytes { bytes: self.bytes, front: self.front, back: self.back }
    }
}

impl<'a> DoubleEndedIterator for Bytes<'a> {
    fn next_back(&mut self) -> Option<u8> {
        if self.front < self.back {
            self.back -= 1;
            Some(self.bytes[self.back])
        } else {
            None
        }
    }
}

/// An iterator over a string's characters: what [`str::chars`] gives.
pub struct Chars<'a> {
    bytes: &'a [u8],
    front: usize,
    back: usize,
}

impl<'a> Iterator for Chars<'a> {
    type Item = char;

    fn next(&mut self) -> Option<char> {
        if self.front < self.back {
            let (c, width) = decode_at(self.bytes, self.front);
            self.front += width;
            Some(c)
        } else {
            None
        }
    }

    /// At least a character per four bytes, at most one per byte.
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.back - self.front;
        ((len + 3) / 4, Some(len))
    }
}

impl<'a> Chars<'a> {
    /// The part of the string not yet gone through.
    pub fn as_str(&self) -> &'a str {
        unsafe { from_utf8_unchecked(&self.bytes[self.front..self.back]) }
    }
}

impl<'a> Clone for Chars<'a> {
    fn clone(&self) -> Chars<'a> {
        Chars { bytes: self.bytes, front: self.front, back: self.back }
    }
}

impl<'a> DoubleEndedIterator for Chars<'a> {
    fn next_back(&mut self) -> Option<char> {
        if self.front < self.back {
            let mut start = self.back - 1;
            while is_continuation(self.bytes[start]) {
                start -= 1;
            }
            self.back = start;
            Some(decode_at(self.bytes, start).0)
        } else {
            None
        }
    }
}

/// An iterator over a string's characters and their byte offsets: what
/// [`str::char_indices`] gives.
pub struct CharIndices<'a> {
    chars: Chars<'a>,
}

impl<'a> Iterator for CharIndices<'a> {
    type Item = (usize, char);

    fn next(&mut self) -> Option<(usize, char)> {
        let at = self.chars.front;
        match self.chars.next() {
            Some(c) => Some((at, c)),
            None => None,
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.chars.size_hint()
    }
}

impl<'a> CharIndices<'a> {
    /// The part of the string not yet gone through.
    pub fn as_str(&self) -> &'a str {
        self.chars.as_str()
    }

    /// The byte offset of the next character, or the length if there
    /// is none.
    pub fn offset(&self) -> usize {
        self.chars.front
    }
}

impl<'a> Clone for CharIndices<'a> {
    fn clone(&self) -> CharIndices<'a> {
        CharIndices { chars: self.chars.clone() }
    }
}

impl<'a> DoubleEndedIterator for CharIndices<'a> {
    fn next_back(&mut self) -> Option<(usize, char)> {
        match self.chars.next_back() {
            Some(c) => Some((self.chars.back, c)),
            None => None,
        }
    }
}

impl PartialEq for str {
    fn eq(&self, other: &str) -> bool {
        let (a, b) = (self.as_bytes(), other.as_bytes());
        a.len() == b.len() && bytes_at(a, 0, b)
    }
}

impl Eq for str {}

impl PartialOrd for str {
    fn partial_cmp(&self, other: &str) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Byte by byte, which for UTF-8 is by code point.
impl Ord for str {
    fn cmp(&self, other: &str) -> Ordering {
        let (a, b) = (self.as_bytes(), other.as_bytes());
        let mut i = 0;
        while i < a.len() && i < b.len() {
            if a[i] != b[i] {
                return if a[i] < b[i] { Less } else { Greater };
            }
            i += 1;
        }
        if a.len() < b.len() {
            Less
        } else if a.len() == b.len() {
            Equal
        } else {
            Greater
        }
    }
}

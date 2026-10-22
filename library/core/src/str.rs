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
use crate::iter::{DoubleEndedIterator, Iterator};
use crate::marker::Copy;
use crate::ops::{Index, Range, RangeFrom, RangeFull, RangeInclusive, RangeTo, RangeToInclusive};
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

/// What `starts_with`, `ends_with`, `contains`, `find` and `split` look
/// for: a `&str` or a `char`. Rust's `Pattern` is unstable; programs
/// only use its implementations.
pub trait Pattern: Sized {
    /// Where the pattern first occurs in `haystack`.
    fn find_in(self, haystack: &str) -> Option<usize>;
    /// How many bytes a match is long.
    fn match_len(&self) -> usize;
    /// Whether `haystack` starts with the pattern.
    fn is_prefix_of(self, haystack: &str) -> bool;
    /// Whether `haystack` ends with the pattern.
    fn is_suffix_of(self, haystack: &str) -> bool;
}

impl Pattern for &str {
    fn find_in(self, haystack: &str) -> Option<usize> {
        find_bytes(haystack.as_bytes(), self.as_bytes())
    }

    fn match_len(&self) -> usize {
        self.len()
    }

    fn is_prefix_of(self, haystack: &str) -> bool {
        bytes_at(haystack.as_bytes(), 0, self.as_bytes())
    }

    fn is_suffix_of(self, haystack: &str) -> bool {
        let (h, n) = (haystack.as_bytes(), self.as_bytes());
        n.len() <= h.len() && bytes_at(h, h.len() - n.len(), n)
    }
}

impl Pattern for char {
    fn match_len(&self) -> usize {
        self.len_utf8()
    }

    fn find_in(self, haystack: &str) -> Option<usize> {
        let mut iter = haystack.char_indices();
        while let Some((at, c)) = iter.next() {
            if c == self {
                return Some(at);
            }
        }
        None
    }

    fn is_prefix_of(self, haystack: &str) -> bool {
        match haystack.chars().next() {
            Some(c) => c == self,
            None => false,
        }
    }

    fn is_suffix_of(self, haystack: &str) -> bool {
        match haystack.chars().next_back() {
            Some(c) => c == self,
            None => false,
        }
    }
}

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

    /// Whether the string starts with `pat`, a `&str` or a `char`.
    pub fn starts_with<P: Pattern>(&self, pat: P) -> bool {
        pat.is_prefix_of(self)
    }

    /// Whether the string ends with `pat`, a `&str` or a `char`.
    pub fn ends_with<P: Pattern>(&self, pat: P) -> bool {
        pat.is_suffix_of(self)
    }

    /// Whether `pat`, a `&str` or a `char`, occurs in the string.
    pub fn contains<P: Pattern>(&self, pat: P) -> bool {
        pat.find_in(self).is_some()
    }

    /// The byte offset where `pat`, a `&str` or a `char`, first occurs.
    pub fn find<P: Pattern>(&self, pat: P) -> Option<usize> {
        pat.find_in(self)
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

    /// The parts of the string between the matches of `pat`, a `&str` or
    /// a `char`.
    pub fn split<P: Pattern + Copy>(&self, pat: P) -> Split<'_, P> {
        Split {
            haystack: self,
            pat,
            start: 0,
            at: 0,
            finished: false,
        }
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

/// An iterator over the parts of a string between the matches of a
/// pattern: what [`str::split`] gives.
pub struct Split<'a, P> {
    haystack: &'a str,
    pat: P,
    /// Where the next part starts.
    start: usize,
    /// Where to look for the next match: past the last, and past a
    /// character after an empty one.
    at: usize,
    finished: bool,
}

impl<'a, P: Pattern + Copy> Iterator for Split<'a, P> {
    type Item = &'a str;

    fn next(&mut self) -> Option<&'a str> {
        if self.finished {
            return None;
        }
        let len = self.haystack.len();
        if self.at <= len
            && let Some(i) = self.pat.find_in(substr(self.haystack, self.at, len))
        {
            let found = self.at + i;
            let n = self.pat.match_len();
            let part = substr(self.haystack, self.start, found);
            self.start = found + n;
            self.at = if n > 0 {
                found + n
            } else if found < len {
                found + decode_at(self.haystack.as_bytes(), found).1
            } else {
                len + 1
            };
            return Some(part);
        }
        self.finished = true;
        Some(substr(self.haystack, self.start, len))
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

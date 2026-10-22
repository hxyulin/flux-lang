//! `char`'s constants and methods, and its conversions.
//!
//! Only what needs no Unicode tables: the ASCII methods, digits,
//! `is_whitespace` (whose set is small) and UTF-8 lengths. A `char` is
//! held as its `u32` value.

use crate::convert::From;
use crate::iter::Step;
use crate::option::Option::{self, None, Some};

/// The `char` for a digit `num` in `radix`, if it is one: `char::from_digit`.
pub const fn from_digit(num: u32, radix: u32) -> Option<char> {
    char::from_digit(num, radix)
}

/// The `char` whose value is `i`, if there is one: `char::from_u32`.
pub const fn from_u32(i: u32) -> Option<char> {
    char::from_u32(i)
}

/// The `char` whose value is `i`, which must be one:
/// `char::from_u32_unchecked`.
pub const unsafe fn from_u32_unchecked(i: u32) -> char {
    unsafe { char::from_u32_unchecked(i) }
}

impl char {
    /// The highest code point.
    pub const MAX: char = '\u{10FFFF}';
    /// `U+FFFD`, which decoding puts in place of what is not UTF-8.
    pub const REPLACEMENT_CHARACTER: char = '\u{FFFD}';

    /// The `char` whose value is `i`, if there is one: not past
    /// `char::MAX`, nor a surrogate (`0xD800..=0xDFFF`).
    pub const fn from_u32(i: u32) -> Option<char> {
        if i > 0x10FFFF || (i >= 0xD800 && i <= 0xDFFF) {
            None
        } else {
            Some(unsafe { char::from_u32_unchecked(i) })
        }
    }

    /// The `char` whose value is `i`, which must be one.
    pub const unsafe fn from_u32_unchecked(i: u32) -> char {
        // A `char` is held as its `u32` value.
        unsafe { *(&i as *const u32 as *const char) }
    }

    /// The `char` for a digit `num` in `radix`, lowercase past 9, if
    /// `num` is less than `radix`. Panics if `radix` is not between 2
    /// and 36.
    #[track_caller]
    pub const fn from_digit(num: u32, radix: u32) -> Option<char> {
        if radix < 2 || radix > 36 {
            crate::panicking::panic("from_digit: radix is too high (maximum 36)");
        }
        if num < radix {
            let num = num as u8;
            if num < 10 { Some((b'0' + num) as char) } else { Some((b'a' + num - 10) as char) }
        } else {
            None
        }
    }

    /// The value of the digit in `radix`, if the `char` is one: `0` to
    /// `9`, then `a` to `z` or `A` to `Z`. Panics if `radix` is not
    /// between 2 and 36.
    pub const fn to_digit(self, radix: u32) -> Option<u32> {
        if radix < 2 || radix > 36 {
            crate::panicking::panic(
                "to_digit: invalid radix -- radix must be in the range 2 to 36 inclusive",
            );
        }
        let mut digit = (self as u32).wrapping_sub('0' as u32);
        if radix > 10 && digit >= 10 {
            // Setting bit 5 makes ASCII letters lowercase.
            let letter = ((self as u32) | 0x20).wrapping_sub('a' as u32);
            digit = if letter > u32::MAX - 10 { u32::MAX } else { letter + 10 };
        }
        if digit < radix { Some(digit) } else { None }
    }

    /// Whether the `char` is a digit in `radix`.
    pub const fn is_digit(self, radix: u32) -> bool {
        self.to_digit(radix).is_some()
    }

    /// How many bytes UTF-8 takes for the `char`: 1 to 4.
    pub const fn len_utf8(self) -> usize {
        let code = self as u32;
        if code < 0x80 {
            1
        } else if code < 0x800 {
            2
        } else if code < 0x10000 {
            3
        } else {
            4
        }
    }

    /// Writes the `char`'s UTF-8 bytes at the start of `dst`, which must
    /// be long enough (or this panics), and gives them as a `str`.
    #[track_caller]
    pub fn encode_utf8(self, dst: &mut [u8]) -> &mut str {
        let len = self.len_utf8();
        if dst.len() < len {
            crate::panicking::panic("encode_utf8: buffer too small")
        }
        let bytes = crate::str::encode_utf8(self);
        let mut i = 0;
        while i < len {
            dst[i] = bytes[i];
            i += 1;
        }
        unsafe { crate::str::from_utf8_unchecked_mut(&mut dst[..len]) }
    }

    /// How many `u16`s UTF-16 takes for the `char`: 1 or 2.
    pub const fn len_utf16(self) -> usize {
        if (self as u32) < 0x10000 { 1 } else { 2 }
    }

    /// Whether the `char` has Unicode's `White_Space` property.
    pub const fn is_whitespace(self) -> bool {
        match self {
            '\t'..='\r' | ' ' | '\u{85}' | '\u{A0}' | '\u{1680}' | '\u{2000}'..='\u{200A}'
            | '\u{2028}' | '\u{2029}' | '\u{202F}' | '\u{205F}' | '\u{3000}' => true,
            _ => false,
        }
    }

    /// Whether the `char` is ASCII: below 128.
    pub const fn is_ascii(&self) -> bool {
        (*self as u32) < 128
    }

    /// `A` to `Z` and `a` to `z`.
    pub const fn is_ascii_alphabetic(&self) -> bool {
        match *self {
            'A'..='Z' | 'a'..='z' => true,
            _ => false,
        }
    }

    /// `A` to `Z`.
    pub const fn is_ascii_uppercase(&self) -> bool {
        match *self {
            'A'..='Z' => true,
            _ => false,
        }
    }

    /// `a` to `z`.
    pub const fn is_ascii_lowercase(&self) -> bool {
        match *self {
            'a'..='z' => true,
            _ => false,
        }
    }

    /// `0` to `9`, `A` to `Z` and `a` to `z`.
    pub const fn is_ascii_alphanumeric(&self) -> bool {
        match *self {
            '0'..='9' | 'A'..='Z' | 'a'..='z' => true,
            _ => false,
        }
    }

    /// `0` to `9`.
    pub const fn is_ascii_digit(&self) -> bool {
        match *self {
            '0'..='9' => true,
            _ => false,
        }
    }

    /// `0` to `9`, `A` to `F` and `a` to `f`.
    pub const fn is_ascii_hexdigit(&self) -> bool {
        match *self {
            '0'..='9' | 'A'..='F' | 'a'..='f' => true,
            _ => false,
        }
    }

    /// The printable characters other than letters, digits and space.
    pub const fn is_ascii_punctuation(&self) -> bool {
        match *self {
            '!'..='/' | ':'..='@' | '['..='`' | '{'..='~' => true,
            _ => false,
        }
    }

    /// The printable characters other than space: `!` to `~`.
    pub const fn is_ascii_graphic(&self) -> bool {
        match *self {
            '!'..='~' => true,
            _ => false,
        }
    }

    /// Space, tab, line feed, form feed and carriage return.
    pub const fn is_ascii_whitespace(&self) -> bool {
        match *self {
            '\t' | '\n' | '\x0C' | '\r' | ' ' => true,
            _ => false,
        }
    }

    /// The control characters: below space, and delete.
    pub const fn is_ascii_control(&self) -> bool {
        match *self {
            '\0'..='\x1F' | '\x7F' => true,
            _ => false,
        }
    }

    /// The `char`, with `a` to `z` made `A` to `Z`.
    pub const fn to_ascii_uppercase(&self) -> char {
        if self.is_ascii_lowercase() { (*self as u8 - 32) as char } else { *self }
    }

    /// The `char`, with `A` to `Z` made `a` to `z`.
    pub const fn to_ascii_lowercase(&self) -> char {
        if self.is_ascii_uppercase() { (*self as u8 + 32) as char } else { *self }
    }

    /// Whether the `char`s are equal ignoring ASCII case.
    pub const fn eq_ignore_ascii_case(&self, other: &char) -> bool {
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

/// Why a `u32` is not a `char`: what `char::try_from` gives for a
/// surrogate or a value past `char::MAX`.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct CharTryFromError(pub(crate) ());

/// Why a `char` is not a `u8` or `u16`: it is past its range.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct TryFromCharError(pub(crate) ());

impl From<u8> for char {
    fn from(byte: u8) -> char {
        byte as char
    }
}

impl From<char> for u32 {
    fn from(c: char) -> u32 {
        c as u32
    }
}

impl From<char> for u64 {
    fn from(c: char) -> u64 {
        c as u64
    }
}

impl From<char> for u128 {
    fn from(c: char) -> u128 {
        c as u128
    }
}

/// `'a'..='z'` iterates, skipping the surrogates.
impl Step for char {
    fn forward_one(self) -> char {
        let next = match self as u32 {
            0xD7FF => 0xE000,
            code => code + 1,
        };
        unsafe { char::from_u32_unchecked(next) }
    }

    fn backward_one(self) -> char {
        let before = match self as u32 {
            0xE000 => 0xD7FF,
            code => code.wrapping_sub(1),
        };
        unsafe { char::from_u32_unchecked(before) }
    }

    /// The characters between, without the surrogates' gap.
    fn steps_between(start: &char, end: &char) -> (usize, Option<usize>) {
        let (start, end) = (*start as u32, *end as u32);
        if start <= end {
            let mut count = end - start;
            if start < 0xD800 && 0xE000 <= end {
                count -= 0x800;
            }
            (count as usize, Some(count as usize))
        } else {
            (0, None)
        }
    }
}

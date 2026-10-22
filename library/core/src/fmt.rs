//! Formatting: the traits `{}`, `{:?}` and the other placeholders name,
//! `Formatter`, which writes through to where text goes with the
//! placeholder's options, `Write`, which such places implement, and
//! `Arguments`, what `format_args!` makes.
//!
//! As rustc's, `Formatter` writes through a `&mut dyn Write`, and the
//! builders' methods (`field`, `entry`) take `&dyn Debug`; the values
//! `Arguments` holds are their addresses and functions that format them.

use crate::marker::PhantomData;
use crate::option::Option::{self, None, Some};
use crate::result::Result::{Err, Ok};

mod builders;
mod num;
pub mod rt;

pub use builders::{DebugList, DebugMap, DebugSet, DebugStruct, DebugTuple};

/// What formatting may fail with.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Debug)]
pub struct Error;

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        Display::fmt("an error occurred when formatting an argument", f)
    }
}

/// What formatting gives.
pub type Result = crate::result::Result<(), Error>;

/// Where `{:<}`, `{:>}` and `{:^}` place a value in its width.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Alignment {
    Left,
    Right,
    Center,
}

/// Places text can be written to: `write!` into a buffer, or a
/// `Formatter`.
pub trait Write {
    /// Writes `s`.
    fn write_str(&mut self, s: &str) -> Result;

    /// Writes `c`.
    fn write_char(&mut self, c: char) -> Result {
        let mut bytes = [0u8; 4];
        self.write_str(c.encode_utf8(&mut bytes))
    }

    /// Writes `args` (`write!(w, ..)`).
    fn write_fmt(&mut self, args: Arguments<'_>) -> Result {
        // Through `&mut Self`, which is sized, as rustc's does.
        let mut this = self;
        write(&mut this, args)
    }
}

impl<W: Write + ?Sized> Write for &mut W {
    fn write_str(&mut self, s: &str) -> Result {
        (**self).write_str(s)
    }

    fn write_char(&mut self, c: char) -> Result {
        (**self).write_char(c)
    }

    fn write_fmt(&mut self, args: Arguments<'_>) -> Result {
        (**self).write_fmt(args)
    }
}

/// rustc's layout of a placeholder's options: the fill character in the
/// low 21 bits of `flags`, then the flags below.
pub(crate) mod flags {
    pub(crate) const SIGN_PLUS_FLAG: u32 = 1 << 21;
    pub(crate) const SIGN_MINUS_FLAG: u32 = 1 << 22;
    pub(crate) const ALTERNATE_FLAG: u32 = 1 << 23;
    pub(crate) const SIGN_AWARE_ZERO_PAD_FLAG: u32 = 1 << 24;
    pub(crate) const DEBUG_LOWER_HEX_FLAG: u32 = 1 << 25;
    pub(crate) const DEBUG_UPPER_HEX_FLAG: u32 = 1 << 26;
    pub(crate) const WIDTH_FLAG: u32 = 1 << 27;
    pub(crate) const PRECISION_FLAG: u32 = 1 << 28;
    pub(crate) const ALIGN_BITS: u32 = 0b11 << 29;
    pub(crate) const ALIGN_LEFT: u32 = 0 << 29;
    pub(crate) const ALIGN_RIGHT: u32 = 1 << 29;
    pub(crate) const ALIGN_CENTER: u32 = 2 << 29;
    pub(crate) const ALIGN_UNKNOWN: u32 = 3 << 29;
}

/// How a placeholder asks for its value to be written.
#[derive(Clone, Copy)]
pub(crate) struct FormattingOptions {
    pub(crate) flags: u32,
    pub(crate) width: u16,
    pub(crate) precision: u16,
}

impl FormattingOptions {
    /// `{}`'s: no flags, filled with spaces, no alignment, width or
    /// precision.
    pub(crate) const fn new() -> FormattingOptions {
        FormattingOptions {
            flags: ' ' as u32 | flags::ALIGN_UNKNOWN,
            width: 0,
            precision: 0,
        }
    }
}

/// Where formatted text goes, and how: a placeholder's options and the
/// writer.
pub struct Formatter<'a> {
    pub(crate) options: FormattingOptions,
    pub(crate) buf: &'a mut (dyn Write + 'a),
}

impl<'a> Formatter<'a> {
    /// A formatter writing to `out`, with `{}`'s options.
    pub(crate) fn new<W: Write>(out: &'a mut W) -> Formatter<'a> {
        Formatter {
            options: FormattingOptions::new(),
            buf: out,
        }
    }

    /// Writes `s`, ignoring the options.
    pub fn write_str(&mut self, data: &str) -> Result {
        self.buf.write_str(data)
    }

    /// Writes `fmt` (`write!(f, ..)`), with each placeholder's own
    /// options.
    pub fn write_fmt(&mut self, fmt: Arguments<'_>) -> Result {
        match fmt.as_statically_known_str() {
            Some(s) => self.buf.write_str(s),
            None => write(self.buf, fmt),
        }
    }

    /// The fill character, `' '` unless the placeholder says otherwise.
    pub fn fill(&self) -> char {
        unsafe { crate::char::from_u32_unchecked(self.options.flags & 0x1F_FFFF) }
    }

    /// The alignment the placeholder asks for, if any.
    pub fn align(&self) -> Option<Alignment> {
        match self.options.flags & flags::ALIGN_BITS {
            flags::ALIGN_LEFT => Some(Alignment::Left),
            flags::ALIGN_RIGHT => Some(Alignment::Right),
            flags::ALIGN_CENTER => Some(Alignment::Center),
            _ => None,
        }
    }

    /// The minimum width the placeholder asks for, if any.
    pub fn width(&self) -> Option<usize> {
        if self.options.flags & flags::WIDTH_FLAG == 0 {
            None
        } else {
            Some(self.options.width as usize)
        }
    }

    /// The precision the placeholder asks for, if any: for strings, the
    /// most characters to write.
    pub fn precision(&self) -> Option<usize> {
        if self.options.flags & flags::PRECISION_FLAG == 0 {
            None
        } else {
            Some(self.options.precision as usize)
        }
    }

    /// `{:+}`
    pub fn sign_plus(&self) -> bool {
        self.options.flags & flags::SIGN_PLUS_FLAG != 0
    }

    /// `{:-}`
    pub fn sign_minus(&self) -> bool {
        self.options.flags & flags::SIGN_MINUS_FLAG != 0
    }

    /// `{:#}`
    pub fn alternate(&self) -> bool {
        self.options.flags & flags::ALTERNATE_FLAG != 0
    }

    /// `{:0}`
    pub fn sign_aware_zero_pad(&self) -> bool {
        self.options.flags & flags::SIGN_AWARE_ZERO_PAD_FLAG != 0
    }

    pub(crate) fn debug_lower_hex(&self) -> bool {
        self.options.flags & flags::DEBUG_LOWER_HEX_FLAG != 0
    }

    pub(crate) fn debug_upper_hex(&self) -> bool {
        self.options.flags & flags::DEBUG_UPPER_HEX_FLAG != 0
    }

    pub(crate) fn set_fill(&mut self, fill: char) {
        self.options.flags = (self.options.flags & !0x1F_FFFF) | fill as u32;
    }

    pub(crate) fn set_align(&mut self, align: u32) {
        self.options.flags = (self.options.flags & !flags::ALIGN_BITS) | align;
    }

    /// Writes an integer's digits `buf`, after its sign (`-` unless
    /// `is_nonnegative`, `+` for `{:+}`) and, for `{:#}`, `prefix`,
    /// padded to the width as integers are: `{:0}` puts zeros after the
    /// sign and prefix.
    pub fn pad_integral(&mut self, is_nonnegative: bool, prefix: &str, buf: &str) -> Result {
        let mut width = buf.len();
        let mut sign = None;
        if !is_nonnegative {
            sign = Some('-');
            width += 1;
        } else if self.sign_plus() {
            sign = Some('+');
            width += 1;
        }
        let prefix = if self.alternate() {
            width += char_count(prefix);
            Some(prefix)
        } else {
            None
        };
        let min = self.options.width as usize;
        if width >= min {
            write_prefix(self, sign, prefix)?;
            self.buf.write_str(buf)
        } else if self.sign_aware_zero_pad() {
            let old = self.options;
            self.set_fill('0');
            self.set_align(flags::ALIGN_RIGHT);
            write_prefix(self, sign, prefix)?;
            let post = self.padding(min - width, Alignment::Right)?;
            self.buf.write_str(buf)?;
            post.write(self)?;
            self.options = old;
            Ok(())
        } else {
            let post = self.padding(min - width, Alignment::Right)?;
            write_prefix(self, sign, prefix)?;
            self.buf.write_str(buf)?;
            post.write(self)
        }
    }

    /// Writes `s` padded to the width, and cut to the precision in
    /// characters: what `{}` of a `str` does.
    pub fn pad(&mut self, s: &str) -> Result {
        if self.options.flags & (flags::WIDTH_FLAG | flags::PRECISION_FLAG) == 0 {
            return self.buf.write_str(s);
        }
        let mut s = s;
        let count;
        if self.options.flags & flags::PRECISION_FLAG != 0 {
            let max = self.options.precision as usize;
            let bytes = s.as_bytes();
            let mut chars = 0;
            let mut end = 0;
            while end < bytes.len() && chars < max {
                end += utf8_width(bytes[end]);
                chars += 1;
            }
            s = unsafe { crate::str::from_utf8_unchecked(crate::intrinsics::subslice(bytes, 0, end)) };
            count = chars;
        } else {
            count = char_count(s);
        }
        let width = self.options.width as usize;
        if count < width {
            let post = self.padding(width - count, Alignment::Left)?;
            self.buf.write_str(s)?;
            post.write(self)
        } else {
            self.buf.write_str(s)
        }
    }

    /// Writes the fill before a value `padding` characters short of the
    /// width, as its alignment (`default` if none) places it, and gives
    /// what goes after.
    pub(crate) fn padding(
        &mut self,
        padding: usize,
        default: Alignment,
    ) -> crate::result::Result<PostPadding, Error> {
        let align = match self.align() {
            Some(align) => align,
            None => default,
        };
        let fill = self.fill();
        let left = match align {
            Alignment::Left => 0,
            Alignment::Right => padding,
            Alignment::Center => padding / 2,
        };
        let mut i = 0;
        while i < left {
            self.buf.write_char(fill)?;
            i += 1;
        }
        Ok(PostPadding {
            fill,
            padding: padding - left,
        })
    }

    /// `name { field: value, .. }`, as `#[derive(Debug)]` writes a
    /// struct.
    pub fn debug_struct<'b>(&'b mut self, name: &str) -> DebugStruct<'b, 'a> {
        builders::debug_struct_new(self, name)
    }

    /// `name(value, ..)`, as `#[derive(Debug)]` writes a tuple struct.
    pub fn debug_tuple<'b>(&'b mut self, name: &str) -> DebugTuple<'b, 'a> {
        builders::debug_tuple_new(self, name)
    }

    /// `[entry, ..]`
    pub fn debug_list<'b>(&'b mut self) -> DebugList<'b, 'a> {
        builders::debug_list_new(self)
    }

    /// `{entry, ..}`
    pub fn debug_set<'b>(&'b mut self) -> DebugSet<'b, 'a> {
        builders::debug_set_new(self)
    }

    /// `{key: value, ..}`
    pub fn debug_map<'b>(&'b mut self) -> DebugMap<'b, 'a> {
        builders::debug_map_new(self)
    }
}

/// The sign, then the prefix, of an integer.
fn write_prefix(f: &mut Formatter<'_>, sign: Option<char>, prefix: Option<&str>) -> Result {
    if let Some(c) = sign {
        f.buf.write_char(c)?;
    }
    match prefix {
        Some(prefix) => f.buf.write_str(prefix),
        None => Ok(()),
    }
}

/// How many bytes the UTF-8 sequence starting with `first` takes.
fn utf8_width(first: u8) -> usize {
    if first < 0x80 {
        1
    } else if first < 0xE0 {
        2
    } else if first < 0xF0 {
        3
    } else {
        4
    }
}

/// How many `char`s `s` has.
pub(crate) fn char_count(s: &str) -> usize {
    let bytes = s.as_bytes();
    let mut count = 0;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] & 0xC0 != 0x80 {
            count += 1;
        }
        i += 1;
    }
    count
}

/// The fill a value still needs after it.
pub(crate) struct PostPadding {
    fill: char,
    padding: usize,
}

impl PostPadding {
    pub(crate) fn write(self, f: &mut Formatter<'_>) -> Result {
        let mut i = 0;
        while i < self.padding {
            f.buf.write_char(self.fill)?;
            i += 1;
        }
        Ok(())
    }
}

impl Write for Formatter<'_> {
    fn write_str(&mut self, s: &str) -> Result {
        self.buf.write_str(s)
    }

    fn write_char(&mut self, c: char) -> Result {
        self.buf.write_char(c)
    }

    fn write_fmt(&mut self, args: Arguments<'_>) -> Result {
        Formatter::write_fmt(self, args)
    }
}

/// What `format_args!` makes: the literal pieces of its format string,
/// each before a placeholder, the values the placeholders show, and
/// their options where some are not `{}`'s.
#[derive(Clone, Copy)]
pub struct Arguments<'a> {
    pieces: &'a [&'static str],
    fmt: Option<&'a [rt::Placeholder]>,
    args: &'a [rt::Argument<'a>],
}

impl<'a> Arguments<'a> {
    /// A format string without placeholders.
    #[doc(hidden)]
    pub const fn new_const(pieces: &'a [&'static str]) -> Arguments<'a> {
        Arguments {
            pieces,
            fmt: None,
            args: &[],
        }
    }

    /// Placeholders that are all `{}`-like, one per argument in turn.
    /// The arguments are an array, as rustc's, since `format_args!`
    /// borrows a `match` that makes them.
    #[doc(hidden)]
    pub fn new_v1<const N: usize>(
        pieces: &'a [&'static str],
        args: &'a [rt::Argument<'a>; N],
    ) -> Arguments<'a> {
        Arguments {
            pieces,
            fmt: None,
            args,
        }
    }

    /// Placeholders with options.
    #[doc(hidden)]
    pub fn new_v1_formatted<const N: usize>(
        pieces: &'a [&'static str],
        args: &'a [rt::Argument<'a>; N],
        placeholders: &'a [rt::Placeholder],
    ) -> Arguments<'a> {
        Arguments {
            pieces,
            fmt: Some(placeholders),
            args,
        }
    }

    /// The text, if it needs no formatting.
    pub const fn as_str(&self) -> Option<&'static str> {
        if !self.args.is_empty() {
            return None;
        }
        match self.pieces.len() {
            0 => Some(""),
            1 => Some(self.pieces[0]),
            _ => None,
        }
    }

    fn as_statically_known_str(&self) -> Option<&'static str> {
        self.as_str()
    }
}

impl Display for Arguments<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write(f.buf, *self)
    }
}

impl Debug for Arguments<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        Display::fmt(self, f)
    }
}

/// Writes `args` to `buf`: each piece, then its placeholder's value,
/// written with a formatter of its own options.
pub fn write(buf: &mut dyn Write, args: Arguments<'_>) -> Result {
    if let Some(s) = args.as_str() {
        return buf.write_str(s);
    }
    let mut idx = 0;
    match args.fmt {
        None => {
            while idx < args.args.len() {
                let piece = args.pieces[idx];
                if !piece.is_empty() {
                    buf.write_str(piece)?;
                }
                let mut f = Formatter {
                    options: FormattingOptions::new(),
                    buf: &mut *buf,
                };
                args.args[idx].fmt(&mut f)?;
                idx += 1;
            }
        }
        Some(placeholders) => {
            while idx < placeholders.len() {
                if idx < args.pieces.len() {
                    let piece = args.pieces[idx];
                    if !piece.is_empty() {
                        buf.write_str(piece)?;
                    }
                }
                let p = &placeholders[idx];
                let mut options = FormattingOptions {
                    flags: p.flags,
                    width: 0,
                    precision: 0,
                };
                if let Some(width) = p.width.get(args.args) {
                    options.width = width;
                    options.flags |= flags::WIDTH_FLAG;
                }
                if let Some(precision) = p.precision.get(args.args) {
                    options.precision = precision;
                    options.flags |= flags::PRECISION_FLAG;
                }
                let mut f = Formatter {
                    options,
                    buf: &mut *buf,
                };
                args.args[p.position].fmt(&mut f)?;
                idx += 1;
            }
        }
    }
    if idx < args.pieces.len() {
        buf.write_str(args.pieces[idx])?;
    }
    Ok(())
}

/// Types that can be shown to users, as `{}` would. A failing bound
/// reads as rustc's.
#[diagnostic::on_unimplemented(message = "`{Self}` doesn't implement `std::fmt::Display`")]
pub trait Display {
    /// Writes the value to `f`.
    fn fmt(&self, f: &mut Formatter<'_>) -> Result;
}

/// Types that can be shown for debugging, as `{:?}` would. A failing
/// bound reads "`T` doesn't implement `Debug`", as in rustc.
#[lang = "Debug"]
#[diagnostic::on_unimplemented(message = "`{Self}` doesn't implement `{This}`")]
pub trait Debug {
    /// Writes the value to `f`.
    fn fmt(&self, f: &mut Formatter<'_>) -> Result;
}

/// `{:x}`
pub trait LowerHex {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result;
}

/// `{:X}`
pub trait UpperHex {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result;
}

/// `{:o}`
pub trait Octal {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result;
}

/// `{:b}`
pub trait Binary {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result;
}

/// `{:e}`, which integers implement (flux has no floats).
pub trait LowerExp {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result;
}

/// `{:E}`
pub trait UpperExp {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result;
}

/// `{:p}`
#[diagnostic::on_unimplemented(message = "the trait bound `{Self}: std::fmt::Pointer` is not satisfied")]
pub trait Pointer {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result;
}

// References show what they refer to.
macro_rules! deref_impls {
    ($($tr:ident),*) => {
        $(
            impl<T: ?Sized + $tr> $tr for &T {
                fn fmt(&self, f: &mut Formatter<'_>) -> Result {
                    $tr::fmt(&**self, f)
                }
            }

            impl<T: ?Sized + $tr> $tr for &mut T {
                fn fmt(&self, f: &mut Formatter<'_>) -> Result {
                    $tr::fmt(&**self, f)
                }
            }
        )*
    };
}

deref_impls!(Display, Debug, LowerHex, UpperHex, Octal, Binary, LowerExp, UpperExp);

/// An address as `{:#x}` would write it, as rustc writes pointers.
pub(crate) fn pointer_fmt_inner(addr: usize, f: &mut Formatter<'_>) -> Result {
    let old = f.options;
    if f.alternate() {
        f.options.flags |= flags::SIGN_AWARE_ZERO_PAD_FLAG;
        if f.width().is_none() {
            f.options.width = 18;
            f.options.flags |= flags::WIDTH_FLAG;
        }
    }
    f.options.flags |= flags::ALTERNATE_FLAG;
    let ret = LowerHex::fmt(&addr, f);
    f.options = old;
    ret
}

impl<T: ?Sized> Pointer for *const T {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        pointer_fmt_inner(*self as *const u8 as usize, f)
    }
}

impl<T: ?Sized> Pointer for *mut T {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        pointer_fmt_inner(*self as *const u8 as usize, f)
    }
}

impl<T: ?Sized> Pointer for &T {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        pointer_fmt_inner(*self as *const T as *const u8 as usize, f)
    }
}

impl<T: ?Sized> Pointer for &mut T {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        pointer_fmt_inner(&**self as *const T as *const u8 as usize, f)
    }
}

impl<T: ?Sized> Debug for *const T {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        Pointer::fmt(self, f)
    }
}

impl<T: ?Sized> Debug for *mut T {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        Pointer::fmt(self, f)
    }
}

impl Display for bool {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        Display::fmt(if *self { "true" } else { "false" }, f)
    }
}

impl Debug for bool {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        Display::fmt(self, f)
    }
}

impl Display for str {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        f.pad(self)
    }
}

impl Debug for str {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        f.write_char('"')?;
        let bytes = self.as_bytes();
        // The start of the text not yet written, which needs no escapes.
        let mut start = 0;
        let mut i = 0;
        while i < bytes.len() {
            let b = bytes[i];
            if b >= 0x20 && b < 0x7F && b != b'\\' && b != b'"' {
                i += 1;
                continue;
            }
            let width = utf8_width(b);
            let c = decode_char(bytes, i);
            if let Some(escape) = escape_debug(c, true, false) {
                f.write_str(sub(self, start, i))?;
                escape.write(f)?;
                start = i + width;
            }
            i += width;
        }
        f.write_str(sub(self, start, bytes.len()))?;
        f.write_char('"')
    }
}

impl Display for char {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        if f.options.flags & (flags::WIDTH_FLAG | flags::PRECISION_FLAG) == 0 {
            f.write_char(*self)
        } else {
            let mut bytes = [0u8; 4];
            f.pad(self.encode_utf8(&mut bytes))
        }
    }
}

impl Debug for char {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        f.write_char('\'')?;
        match escape_debug(*self, false, true) {
            Some(escape) => escape.write(f)?,
            None => f.write_char(*self)?,
        }
        f.write_char('\'')
    }
}

impl Debug for () {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        f.pad("()")
    }
}

impl<T> Debug for PhantomData<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        f.write_str("PhantomData")
    }
}

/// `s[start..end]`, at character boundaries.
fn sub(s: &str, start: usize, end: usize) -> &str {
    unsafe { crate::str::from_utf8_unchecked(crate::intrinsics::subslice(s.as_bytes(), start, end)) }
}

/// The `char` whose UTF-8 bytes start at `bytes[i]`.
pub(crate) fn decode_char(bytes: &[u8], i: usize) -> char {
    let b = bytes[i] as u32;
    let c = if b < 0x80 {
        b
    } else if b < 0xE0 {
        ((b & 0x1F) << 6) | (bytes[i + 1] as u32 & 0x3F)
    } else if b < 0xF0 {
        ((b & 0x0F) << 12) | ((bytes[i + 1] as u32 & 0x3F) << 6) | (bytes[i + 2] as u32 & 0x3F)
    } else {
        ((b & 0x07) << 18)
            | ((bytes[i + 1] as u32 & 0x3F) << 12)
            | ((bytes[i + 2] as u32 & 0x3F) << 6)
            | (bytes[i + 3] as u32 & 0x3F)
    };
    unsafe { crate::char::from_u32_unchecked(c) }
}

/// Writes byte `b` as `u8::escape_ascii` does.
pub(crate) fn escape_ascii(b: u8, f: &mut Formatter<'_>) -> Result {
    match b {
        b'\t' => f.write_str("\\t"),
        b'\r' => f.write_str("\\r"),
        b'\n' => f.write_str("\\n"),
        b'\\' => f.write_str("\\\\"),
        b'\'' => f.write_str("\\'"),
        b'"' => f.write_str("\\\""),
        0x20..=0x7E => f.write_char(b as char),
        _ => {
            let hex = b"0123456789abcdef";
            f.write_str("\\x")?;
            f.write_char(hex[(b >> 4) as usize] as char)?;
            f.write_char(hex[(b & 0xF) as usize] as char)
        }
    }
}

/// How `{:?}` escapes a `char`: a backslash and a letter, or `\u{..}`.
pub(crate) enum Escape {
    Backslash(char),
    Unicode(u32),
}

impl Escape {
    pub(crate) fn write(self, f: &mut Formatter<'_>) -> Result {
        match self {
            Escape::Backslash(c) => {
                f.write_char('\\')?;
                f.write_char(c)
            }
            Escape::Unicode(c) => {
                f.write_str("\\u{")?;
                let mut digits = [0u8; 8];
                let mut n = 0;
                let mut v = c;
                loop {
                    let d = (v & 0xF) as u8;
                    digits[n] = if d < 10 { b'0' + d } else { b'a' + d - 10 };
                    n += 1;
                    v >>= 4;
                    if v == 0 {
                        break;
                    }
                }
                while n > 0 {
                    n -= 1;
                    f.write_char(digits[n] as char)?;
                }
                f.write_char('}')
            }
        }
    }
}

/// How `{:?}` escapes `c` in a string (`in_str`) or a `char` literal,
/// if it does: rustc's `char::escape_debug_ext`, with its Unicode
/// tables approximated by the ranges below.
pub(crate) fn escape_debug(c: char, escape_double: bool, escape_single: bool) -> Option<Escape> {
    match c {
        '"' if escape_double => Some(Escape::Backslash('"')),
        '\'' if escape_single => Some(Escape::Backslash('\'')),
        '\\' => Some(Escape::Backslash('\\')),
        '\n' => Some(Escape::Backslash('n')),
        '\t' => Some(Escape::Backslash('t')),
        '\r' => Some(Escape::Backslash('r')),
        '\0' => Some(Escape::Backslash('0')),
        '\x20'..='\x7E' | '\u{FF9E}' | '\u{FF9F}' => None,
        _ if needs_unicode_escape(c as u32) => Some(Escape::Unicode(c as u32)),
        _ => None,
    }
}

/// Control, private-use, whitespace, grapheme-extending, default
/// ignorable and format characters, as far as these ranges cover them.
fn needs_unicode_escape(c: u32) -> bool {
    c < 0x20
        || (0x7F..=0x9F).contains(&c)
        || c == 0xA0
        || c == 0xAD
        || (0x300..=0x36F).contains(&c)
        || (0x483..=0x489).contains(&c)
        || (0x591..=0x5BD).contains(&c)
        || c == 0x61C
        || (0x610..=0x61A).contains(&c)
        || (0x64B..=0x65F).contains(&c)
        || c == 0x1680
        || (0x1AB0..=0x1AFF).contains(&c)
        || (0x1DC0..=0x1DFF).contains(&c)
        || (0x2000..=0x200F).contains(&c)
        || (0x2028..=0x202F).contains(&c)
        || (0x205F..=0x206F).contains(&c)
        || (0x20D0..=0x20FF).contains(&c)
        || c == 0x3000
        || (0xD800..=0xDFFF).contains(&c)
        || (0xE000..=0xF8FF).contains(&c)
        || (0xFE00..=0xFE0F).contains(&c)
        || (0xFE20..=0xFE2F).contains(&c)
        || c == 0xFEFF
        || (0xFFF0..=0xFFFB).contains(&c)
        || (0xE0000..=0xE0FFF).contains(&c)
        || c >= 0xF0000
}

impl<T: Debug> Debug for [T] {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

// Tuples, as `(a, b)`, and `(a,)` for one element.
macro_rules! tuple_debug {
    ($(($($name:ident $idx:tt),+))*) => {
        $(
            impl<$($name: Debug),+> Debug for ($($name,)+) {
                fn fmt(&self, f: &mut Formatter<'_>) -> Result {
                    let mut builder = f.debug_tuple("");
                    $(builder.field(&self.$idx);)+
                    builder.finish()
                }
            }
        )*
    };
}

tuple_debug! {
    (A 0)
    (A 0, B 1)
    (A 0, B 1, C 2)
    (A 0, B 1, C 2, D 3)
    (A 0, B 1, C 2, D 3, E 4)
    (A 0, B 1, C 2, D 3, E 4, F 5)
    (A 0, B 1, C 2, D 3, E 4, F 5, G 6)
    (A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7)
    (A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8)
    (A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8, J 9)
    (A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8, J 9, K 10)
    (A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8, J 9, K 10, L 11)
}

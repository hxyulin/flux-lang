//! The types generated programs use, and literals of them.

use crate::rng::Source;

/// The integer types.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum IntTy {
    I8,
    I16,
    I32,
    I64,
    I128,
    Isize,
    U8,
    U16,
    U32,
    U64,
    U128,
    Usize,
}

impl IntTy {
    pub const ALL: [IntTy; 12] = [
        IntTy::I32,
        IntTy::U8,
        IntTy::I64,
        IntTy::U32,
        IntTy::Usize,
        IntTy::I8,
        IntTy::U16,
        IntTy::I16,
        IntTy::U64,
        IntTy::Isize,
        IntTy::I128,
        IntTy::U128,
    ];

    pub fn name(self) -> &'static str {
        match self {
            IntTy::I8 => "i8",
            IntTy::I16 => "i16",
            IntTy::I32 => "i32",
            IntTy::I64 => "i64",
            IntTy::I128 => "i128",
            IntTy::Isize => "isize",
            IntTy::U8 => "u8",
            IntTy::U16 => "u16",
            IntTy::U32 => "u32",
            IntTy::U64 => "u64",
            IntTy::U128 => "u128",
            IntTy::Usize => "usize",
        }
    }

    pub fn bits(self) -> u32 {
        match self {
            IntTy::I8 | IntTy::U8 => 8,
            IntTy::I16 | IntTy::U16 => 16,
            IntTy::I32 | IntTy::U32 => 32,
            IntTy::I64 | IntTy::U64 | IntTy::Isize | IntTy::Usize => 64,
            IntTy::I128 | IntTy::U128 => 128,
        }
    }

    pub fn signed(self) -> bool {
        matches!(
            self,
            IntTy::I8 | IntTy::I16 | IntTy::I32 | IntTy::I64 | IntTy::I128 | IntTy::Isize
        )
    }

    /// Masks `v` to this type's width (as an unsigned bit pattern).
    pub fn truncate(self, v: u128) -> u128 {
        if self.bits() == 128 {
            v
        } else {
            v & ((1u128 << self.bits()) - 1)
        }
    }

    /// The value of the bit pattern `v` as a signed number, for signed
    /// types; unsigned types' values are `v` itself.
    pub fn value(self, v: u128) -> i128 {
        let v = self.truncate(v);
        if self.signed() && self.bits() < 128 && v >> (self.bits() - 1) == 1 {
            (v as i128) - (1i128 << self.bits())
        } else {
            v as i128
        }
    }

    /// Orders two bit patterns as values of this type.
    pub fn cmp(self, a: u128, b: u128) -> std::cmp::Ordering {
        if self.signed() {
            self.value(a).cmp(&self.value(b))
        } else {
            self.truncate(a).cmp(&self.truncate(b))
        }
    }

    pub fn min(self) -> i128 {
        if self.signed() {
            if self.bits() == 128 {
                i128::MIN
            } else {
                -(1i128 << (self.bits() - 1))
            }
        } else {
            0
        }
    }

    /// The maximum, as a bit pattern (u128::MAX does not fit an i128).
    pub fn max_bits(self) -> u128 {
        if self.signed() {
            self.truncate(u128::MAX) >> 1
        } else {
            self.truncate(u128::MAX)
        }
    }
}

/// A literal of `ty` with the bit pattern `v`, as an expression: suffixed,
/// negative ones parenthesized, the minimum of a signed type as `T::MIN`
/// (whose negation would not fit).
pub fn int_literal(ty: IntTy, v: u128) -> String {
    let lit = int_pattern_literal(ty, v);
    if lit.starts_with('-') {
        format!("({lit})")
    } else {
        lit
    }
}

/// A literal of `ty` as a pattern: like [`int_literal`] without parentheses.
pub fn int_pattern_literal(ty: IntTy, v: u128) -> String {
    let n = ty.value(v);
    if ty.signed() && n == ty.min() {
        return format!("{}::MIN", ty.name());
    }
    if !ty.signed() && ty.truncate(v) == ty.max_bits() && ty.bits() >= 64 {
        return format!("{}::MAX", ty.name());
    }
    int_numeric_literal(ty, v)
}

/// A literal of `ty` written as a number, never a path (which a pattern
/// matched through a reference cannot use).
pub fn int_numeric_literal(ty: IntTy, v: u128) -> String {
    let n = ty.value(v);
    if ty.signed() {
        format!("{n}{}", ty.name())
    } else {
        format!("{}{}", ty.truncate(v), ty.name())
    }
}

/// An interesting bit pattern for `ty`: often 0, small, or near a limit.
pub fn int_value(src: &mut Source, ty: IntTy) -> u128 {
    let bits = ty.bits();
    match src.weighted(&[6, 10, 3, 3, 3, 2, 3, 4]) {
        0 => 0,
        1 => src.below(17) as u128,
        2 => ty.truncate((-(src.range(1, 16) as i128)) as u128),
        3 => ty.max_bits() - src.below(3) as u128,
        4 => ty.truncate(ty.min() as u128) + src.below(3) as u128,
        5 => {
            let shift = src.below(bits);
            let delta = src.below(3) as i128 - 1;
            ty.truncate(((1u128 << shift) as i128 + delta) as u128)
        }
        6 => ty.truncate(src.bits64() as u128 | ((src.bits64() as u128) << 64)),
        _ => src.below(256) as u128,
    }
}

/// A type in a generated program. Structs and enums are named by their
/// index in the program (`S3`, `E1`).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Ty {
    Int(IntTy),
    Bool,
    Char,
    Unit,
    Tuple(Vec<Ty>),
    Array(Box<Ty>, usize),
    Struct(usize),
    Enum(usize),
    Option(Box<Ty>),
    Result(Box<Ty>, Box<Ty>),
    Ref(Box<Ty>),
    RefMut(Box<Ty>),
    /// The type parameter `T` of a generic function, bounded by the
    /// program's trait.
    Param,
    /// A closure or function pointer: only ever a variable's type, never
    /// chosen at random.
    Fn(Vec<Ty>, Box<Ty>),
}

impl Ty {
    pub fn name(&self) -> String {
        match self {
            Ty::Int(i) => i.name().to_owned(),
            Ty::Bool => "bool".to_owned(),
            Ty::Char => "char".to_owned(),
            Ty::Unit => "()".to_owned(),
            Ty::Tuple(elems) => {
                let names: Vec<String> = elems.iter().map(Ty::name).collect();
                if names.len() == 1 {
                    format!("({},)", names[0])
                } else {
                    format!("({})", names.join(", "))
                }
            }
            Ty::Array(elem, n) => format!("[{}; {n}]", elem.name()),
            Ty::Struct(i) => format!("S{i}"),
            Ty::Enum(i) => format!("E{i}"),
            Ty::Option(t) => format!("Option<{}>", t.name()),
            Ty::Result(t, e) => format!("Result<{}, {}>", t.name(), e.name()),
            Ty::Ref(t) => format!("&{}", t.name()),
            Ty::RefMut(t) => format!("&mut {}", t.name()),
            Ty::Param => "T".to_owned(),
            Ty::Fn(params, ret) => {
                let names: Vec<String> = params.iter().map(Ty::name).collect();
                if **ret == Ty::Unit {
                    format!("fn({})", names.join(", "))
                } else {
                    format!("fn({}) -> {}", names.join(", "), ret.name())
                }
            }
        }
    }

    pub fn int(&self) -> Option<IntTy> {
        match self {
            Ty::Int(i) => Some(*i),
            _ => None,
        }
    }

    /// Integers, `bool` and `char`: types with literals, `as` casts and
    /// comparisons built in.
    pub fn is_scalar(&self) -> bool {
        matches!(self, Ty::Int(_) | Ty::Bool | Ty::Char)
    }

    /// The type behind any number of references.
    pub fn strip_refs(&self) -> &Ty {
        match self {
            Ty::Ref(t) | Ty::RefMut(t) => t.strip_refs(),
            t => t,
        }
    }

    pub fn contains_param(&self) -> bool {
        match self {
            Ty::Param => true,
            Ty::Tuple(e) => e.iter().any(Ty::contains_param),
            Ty::Array(t, _) | Ty::Option(t) | Ty::Ref(t) | Ty::RefMut(t) => t.contains_param(),
            Ty::Result(t, e) => t.contains_param() || e.contains_param(),
            Ty::Fn(p, r) => p.iter().any(Ty::contains_param) || r.contains_param(),
            _ => false,
        }
    }
}

/// A `char` literal: ASCII mostly, sometimes multi-byte.
pub fn char_literal(src: &mut Source) -> (char, String) {
    let c = match src.weighted(&[6, 2, 2, 1]) {
        0 => (b'a' + src.below(26) as u8) as char,
        1 => (b'0' + src.below(10) as u8) as char,
        2 => [
            'é',
            'ß',
            '€',
            'λ',
            '中',
            '😀',
            '\u{10FFFF}',
            '\u{7f}',
            '\u{80}',
            '\u{7ff}',
            '\u{800}',
            '\u{ffff}',
            '\u{10000}',
        ][src.below(13) as usize],
        _ => [' ', '!', 'A', 'Z', '_', '~', '\0', '\n'][src.below(8) as usize],
    };
    (c, char_text(c))
}

pub fn char_text(c: char) -> String {
    if c.is_ascii_graphic() && c != '\'' && c != '\\' || c == ' ' {
        format!("'{c}'")
    } else {
        format!("'\\u{{{:x}}}'", c as u32)
    }
}

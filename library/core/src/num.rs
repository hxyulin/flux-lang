//! The integer types' constants and methods.
//!
//! Written in flux, which compiles with overflow checks off, so
//! arithmetic wraps, during constant evaluation too, except in methods
//! marked `#[rustc_inherit_overflow_checks]`, which check where their
//! caller does (in constants), as Rust's `core` does. `min` and `max`
//! are `Ord`'s, as in Rust.

/// Why an integer is not of another integer type: what `TryFrom`
/// gives where the value is out of its range, too large or too small.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct TryFromIntError(pub(crate) IntErrorKind);

/// rustc's: "number too large to fit in target type" and the like.
impl crate::fmt::Display for TryFromIntError {
    fn fmt(&self, f: &mut crate::fmt::Formatter<'_>) -> crate::fmt::Result {
        f.pad(match self.0 {
            IntErrorKind::NegOverflow => "number too small to fit in target type",
            _ => "number too large to fit in target type",
        })
    }
}

/// Why converting or parsing an integer failed.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum IntErrorKind {
    /// Nothing to parse.
    Empty,
    /// A character that is not a digit.
    InvalidDigit,
    /// Too large for the type.
    PosOverflow,
    /// Too small for the type.
    NegOverflow,
    /// Zero, for a non-zero type.
    Zero,
}

impl i8 {
    pub const MIN: i8 = -128;
    pub const MAX: i8 = 127;
    pub const BITS: u32 = 8;

    pub const fn wrapping_add(self, rhs: i8) -> i8 {
        self + rhs
    }

    pub const fn wrapping_sub(self, rhs: i8) -> i8 {
        self - rhs
    }

    pub const fn wrapping_mul(self, rhs: i8) -> i8 {
        self * rhs
    }

    pub const fn wrapping_neg(self) -> i8 {
        0 - self
    }

    #[rustc_inherit_overflow_checks]
    pub const fn pow(self, exp: u32) -> i8 {
        let mut base = self;
        let mut exp = exp;
        let mut acc: i8 = 1;
        while exp > 0 {
            if exp & 1 == 1 {
                acc = acc * base;
            }
            exp >>= 1;
            if exp > 0 {
                base = base * base;
            }
        }
        acc
    }

    pub const fn count_ones(self) -> u32 {
        let mut x = self as u8;
        let mut n = 0;
        while x != 0 {
            n += (x & 1) as u32;
            x >>= 1;
        }
        n
    }

    pub const fn count_zeros(self) -> u32 {
        8 - self.count_ones()
    }

    pub const fn leading_zeros(self) -> u32 {
        let x = self as u8;
        let mut n = 0;
        let mut bit = 8;
        while bit > 0 {
            bit -= 1;
            if (x >> bit) & 1 == 1 {
                return n;
            }
            n += 1;
        }
        n
    }

    pub const fn trailing_zeros(self) -> u32 {
        let mut x = self as u8;
        if x == 0 {
            return 8;
        }
        let mut n = 0;
        while x & 1 == 0 {
            x >>= 1;
            n += 1;
        }
        n
    }

    pub const fn rotate_left(self, n: u32) -> i8 {
        let x = self as u8;
        let n = n % 8;
        if n == 0 {
            self
        } else {
            ((x << n) | (x >> (8 - n))) as i8
        }
    }

    pub const fn rotate_right(self, n: u32) -> i8 {
        self.rotate_left((8 - n % 8) % 8)
    }

    pub const fn abs_diff(self, other: i8) -> u8 {
        if self < other {
            (other as u8) - (self as u8)
        } else {
            (self as u8) - (other as u8)
        }
    }

    #[rustc_inherit_overflow_checks]
    pub const fn abs(self) -> i8 {
        if self < 0 { -self } else { self }
    }

    pub const fn unsigned_abs(self) -> u8 {
        if self < 0 {
            0 - (self as u8)
        } else {
            self as u8
        }
    }

    pub const fn signum(self) -> i8 {
        if self < 0 {
            -1
        } else if self == 0 {
            0
        } else {
            1
        }
    }

    pub const fn is_positive(self) -> bool {
        self > 0
    }

    pub const fn is_negative(self) -> bool {
        self < 0
    }

    pub const fn rem_euclid(self, rhs: i8) -> i8 {
        let r = self % rhs;
        if r < 0 {
            if rhs < 0 { r - rhs } else { r + rhs }
        } else {
            r
        }
    }
}

impl i16 {
    pub const MIN: i16 = -32768;
    pub const MAX: i16 = 32767;
    pub const BITS: u32 = 16;

    pub const fn wrapping_add(self, rhs: i16) -> i16 {
        self + rhs
    }

    pub const fn wrapping_sub(self, rhs: i16) -> i16 {
        self - rhs
    }

    pub const fn wrapping_mul(self, rhs: i16) -> i16 {
        self * rhs
    }

    pub const fn wrapping_neg(self) -> i16 {
        0 - self
    }

    #[rustc_inherit_overflow_checks]
    pub const fn pow(self, exp: u32) -> i16 {
        let mut base = self;
        let mut exp = exp;
        let mut acc: i16 = 1;
        while exp > 0 {
            if exp & 1 == 1 {
                acc = acc * base;
            }
            exp >>= 1;
            if exp > 0 {
                base = base * base;
            }
        }
        acc
    }

    pub const fn count_ones(self) -> u32 {
        let mut x = self as u16;
        let mut n = 0;
        while x != 0 {
            n += (x & 1) as u32;
            x >>= 1;
        }
        n
    }

    pub const fn count_zeros(self) -> u32 {
        16 - self.count_ones()
    }

    pub const fn leading_zeros(self) -> u32 {
        let x = self as u16;
        let mut n = 0;
        let mut bit = 16;
        while bit > 0 {
            bit -= 1;
            if (x >> bit) & 1 == 1 {
                return n;
            }
            n += 1;
        }
        n
    }

    pub const fn trailing_zeros(self) -> u32 {
        let mut x = self as u16;
        if x == 0 {
            return 16;
        }
        let mut n = 0;
        while x & 1 == 0 {
            x >>= 1;
            n += 1;
        }
        n
    }

    pub const fn rotate_left(self, n: u32) -> i16 {
        let x = self as u16;
        let n = n % 16;
        if n == 0 {
            self
        } else {
            ((x << n) | (x >> (16 - n))) as i16
        }
    }

    pub const fn rotate_right(self, n: u32) -> i16 {
        self.rotate_left((16 - n % 16) % 16)
    }

    pub const fn abs_diff(self, other: i16) -> u16 {
        if self < other {
            (other as u16) - (self as u16)
        } else {
            (self as u16) - (other as u16)
        }
    }

    #[rustc_inherit_overflow_checks]
    pub const fn abs(self) -> i16 {
        if self < 0 { -self } else { self }
    }

    pub const fn unsigned_abs(self) -> u16 {
        if self < 0 {
            0 - (self as u16)
        } else {
            self as u16
        }
    }

    pub const fn signum(self) -> i16 {
        if self < 0 {
            -1
        } else if self == 0 {
            0
        } else {
            1
        }
    }

    pub const fn is_positive(self) -> bool {
        self > 0
    }

    pub const fn is_negative(self) -> bool {
        self < 0
    }

    pub const fn rem_euclid(self, rhs: i16) -> i16 {
        let r = self % rhs;
        if r < 0 {
            if rhs < 0 { r - rhs } else { r + rhs }
        } else {
            r
        }
    }
}

impl i32 {
    pub const MIN: i32 = -2147483648;
    pub const MAX: i32 = 2147483647;
    pub const BITS: u32 = 32;

    pub const fn wrapping_add(self, rhs: i32) -> i32 {
        self + rhs
    }

    pub const fn wrapping_sub(self, rhs: i32) -> i32 {
        self - rhs
    }

    pub const fn wrapping_mul(self, rhs: i32) -> i32 {
        self * rhs
    }

    pub const fn wrapping_neg(self) -> i32 {
        0 - self
    }

    #[rustc_inherit_overflow_checks]
    pub const fn pow(self, exp: u32) -> i32 {
        let mut base = self;
        let mut exp = exp;
        let mut acc: i32 = 1;
        while exp > 0 {
            if exp & 1 == 1 {
                acc = acc * base;
            }
            exp >>= 1;
            if exp > 0 {
                base = base * base;
            }
        }
        acc
    }

    pub const fn count_ones(self) -> u32 {
        let mut x = self as u32;
        let mut n = 0;
        while x != 0 {
            n += (x & 1) as u32;
            x >>= 1;
        }
        n
    }

    pub const fn count_zeros(self) -> u32 {
        32 - self.count_ones()
    }

    pub const fn leading_zeros(self) -> u32 {
        let x = self as u32;
        let mut n = 0;
        let mut bit = 32;
        while bit > 0 {
            bit -= 1;
            if (x >> bit) & 1 == 1 {
                return n;
            }
            n += 1;
        }
        n
    }

    pub const fn trailing_zeros(self) -> u32 {
        let mut x = self as u32;
        if x == 0 {
            return 32;
        }
        let mut n = 0;
        while x & 1 == 0 {
            x >>= 1;
            n += 1;
        }
        n
    }

    pub const fn rotate_left(self, n: u32) -> i32 {
        let x = self as u32;
        let n = n % 32;
        if n == 0 {
            self
        } else {
            ((x << n) | (x >> (32 - n))) as i32
        }
    }

    pub const fn rotate_right(self, n: u32) -> i32 {
        self.rotate_left((32 - n % 32) % 32)
    }

    pub const fn abs_diff(self, other: i32) -> u32 {
        if self < other {
            (other as u32) - (self as u32)
        } else {
            (self as u32) - (other as u32)
        }
    }

    #[rustc_inherit_overflow_checks]
    pub const fn abs(self) -> i32 {
        if self < 0 { -self } else { self }
    }

    pub const fn unsigned_abs(self) -> u32 {
        if self < 0 {
            0 - (self as u32)
        } else {
            self as u32
        }
    }

    pub const fn signum(self) -> i32 {
        if self < 0 {
            -1
        } else if self == 0 {
            0
        } else {
            1
        }
    }

    pub const fn is_positive(self) -> bool {
        self > 0
    }

    pub const fn is_negative(self) -> bool {
        self < 0
    }

    pub const fn rem_euclid(self, rhs: i32) -> i32 {
        let r = self % rhs;
        if r < 0 {
            if rhs < 0 { r - rhs } else { r + rhs }
        } else {
            r
        }
    }
}

impl i64 {
    pub const MIN: i64 = -9223372036854775808;
    pub const MAX: i64 = 9223372036854775807;
    pub const BITS: u32 = 64;

    pub const fn wrapping_add(self, rhs: i64) -> i64 {
        self + rhs
    }

    pub const fn wrapping_sub(self, rhs: i64) -> i64 {
        self - rhs
    }

    pub const fn wrapping_mul(self, rhs: i64) -> i64 {
        self * rhs
    }

    pub const fn wrapping_neg(self) -> i64 {
        0 - self
    }

    #[rustc_inherit_overflow_checks]
    pub const fn pow(self, exp: u32) -> i64 {
        let mut base = self;
        let mut exp = exp;
        let mut acc: i64 = 1;
        while exp > 0 {
            if exp & 1 == 1 {
                acc = acc * base;
            }
            exp >>= 1;
            if exp > 0 {
                base = base * base;
            }
        }
        acc
    }

    pub const fn count_ones(self) -> u32 {
        let mut x = self as u64;
        let mut n = 0;
        while x != 0 {
            n += (x & 1) as u32;
            x >>= 1;
        }
        n
    }

    pub const fn count_zeros(self) -> u32 {
        64 - self.count_ones()
    }

    pub const fn leading_zeros(self) -> u32 {
        let x = self as u64;
        let mut n = 0;
        let mut bit = 64;
        while bit > 0 {
            bit -= 1;
            if (x >> bit) & 1 == 1 {
                return n;
            }
            n += 1;
        }
        n
    }

    pub const fn trailing_zeros(self) -> u32 {
        let mut x = self as u64;
        if x == 0 {
            return 64;
        }
        let mut n = 0;
        while x & 1 == 0 {
            x >>= 1;
            n += 1;
        }
        n
    }

    pub const fn rotate_left(self, n: u32) -> i64 {
        let x = self as u64;
        let n = n % 64;
        if n == 0 {
            self
        } else {
            ((x << n) | (x >> (64 - n))) as i64
        }
    }

    pub const fn rotate_right(self, n: u32) -> i64 {
        self.rotate_left((64 - n % 64) % 64)
    }

    pub const fn abs_diff(self, other: i64) -> u64 {
        if self < other {
            (other as u64) - (self as u64)
        } else {
            (self as u64) - (other as u64)
        }
    }

    #[rustc_inherit_overflow_checks]
    pub const fn abs(self) -> i64 {
        if self < 0 { -self } else { self }
    }

    pub const fn unsigned_abs(self) -> u64 {
        if self < 0 {
            0 - (self as u64)
        } else {
            self as u64
        }
    }

    pub const fn signum(self) -> i64 {
        if self < 0 {
            -1
        } else if self == 0 {
            0
        } else {
            1
        }
    }

    pub const fn is_positive(self) -> bool {
        self > 0
    }

    pub const fn is_negative(self) -> bool {
        self < 0
    }

    pub const fn rem_euclid(self, rhs: i64) -> i64 {
        let r = self % rhs;
        if r < 0 {
            if rhs < 0 { r - rhs } else { r + rhs }
        } else {
            r
        }
    }
}

impl i128 {
    pub const MIN: i128 = -170141183460469231731687303715884105728;
    pub const MAX: i128 = 170141183460469231731687303715884105727;
    pub const BITS: u32 = 128;

    pub const fn wrapping_add(self, rhs: i128) -> i128 {
        self + rhs
    }

    pub const fn wrapping_sub(self, rhs: i128) -> i128 {
        self - rhs
    }

    pub const fn wrapping_mul(self, rhs: i128) -> i128 {
        self * rhs
    }

    pub const fn wrapping_neg(self) -> i128 {
        0 - self
    }

    #[rustc_inherit_overflow_checks]
    pub const fn pow(self, exp: u32) -> i128 {
        let mut base = self;
        let mut exp = exp;
        let mut acc: i128 = 1;
        while exp > 0 {
            if exp & 1 == 1 {
                acc = acc * base;
            }
            exp >>= 1;
            if exp > 0 {
                base = base * base;
            }
        }
        acc
    }

    pub const fn count_ones(self) -> u32 {
        let mut x = self as u128;
        let mut n = 0;
        while x != 0 {
            n += (x & 1) as u32;
            x >>= 1;
        }
        n
    }

    pub const fn count_zeros(self) -> u32 {
        128 - self.count_ones()
    }

    pub const fn leading_zeros(self) -> u32 {
        let x = self as u128;
        let mut n = 0;
        let mut bit = 128;
        while bit > 0 {
            bit -= 1;
            if (x >> bit) & 1 == 1 {
                return n;
            }
            n += 1;
        }
        n
    }

    pub const fn trailing_zeros(self) -> u32 {
        let mut x = self as u128;
        if x == 0 {
            return 128;
        }
        let mut n = 0;
        while x & 1 == 0 {
            x >>= 1;
            n += 1;
        }
        n
    }

    pub const fn rotate_left(self, n: u32) -> i128 {
        let x = self as u128;
        let n = n % 128;
        if n == 0 {
            self
        } else {
            ((x << n) | (x >> (128 - n))) as i128
        }
    }

    pub const fn rotate_right(self, n: u32) -> i128 {
        self.rotate_left((128 - n % 128) % 128)
    }

    pub const fn abs_diff(self, other: i128) -> u128 {
        if self < other {
            (other as u128) - (self as u128)
        } else {
            (self as u128) - (other as u128)
        }
    }

    #[rustc_inherit_overflow_checks]
    pub const fn abs(self) -> i128 {
        if self < 0 { -self } else { self }
    }

    pub const fn unsigned_abs(self) -> u128 {
        if self < 0 {
            0 - (self as u128)
        } else {
            self as u128
        }
    }

    pub const fn signum(self) -> i128 {
        if self < 0 {
            -1
        } else if self == 0 {
            0
        } else {
            1
        }
    }

    pub const fn is_positive(self) -> bool {
        self > 0
    }

    pub const fn is_negative(self) -> bool {
        self < 0
    }

    pub const fn rem_euclid(self, rhs: i128) -> i128 {
        let r = self % rhs;
        if r < 0 {
            if rhs < 0 { r - rhs } else { r + rhs }
        } else {
            r
        }
    }
}

impl isize {
    pub const MIN: isize = -9223372036854775808;
    pub const MAX: isize = 9223372036854775807;
    pub const BITS: u32 = 64;

    pub const fn wrapping_add(self, rhs: isize) -> isize {
        self + rhs
    }

    pub const fn wrapping_sub(self, rhs: isize) -> isize {
        self - rhs
    }

    pub const fn wrapping_mul(self, rhs: isize) -> isize {
        self * rhs
    }

    pub const fn wrapping_neg(self) -> isize {
        0 - self
    }

    #[rustc_inherit_overflow_checks]
    pub const fn pow(self, exp: u32) -> isize {
        let mut base = self;
        let mut exp = exp;
        let mut acc: isize = 1;
        while exp > 0 {
            if exp & 1 == 1 {
                acc = acc * base;
            }
            exp >>= 1;
            if exp > 0 {
                base = base * base;
            }
        }
        acc
    }

    pub const fn count_ones(self) -> u32 {
        let mut x = self as usize;
        let mut n = 0;
        while x != 0 {
            n += (x & 1) as u32;
            x >>= 1;
        }
        n
    }

    pub const fn count_zeros(self) -> u32 {
        64 - self.count_ones()
    }

    pub const fn leading_zeros(self) -> u32 {
        let x = self as usize;
        let mut n = 0;
        let mut bit = 64;
        while bit > 0 {
            bit -= 1;
            if (x >> bit) & 1 == 1 {
                return n;
            }
            n += 1;
        }
        n
    }

    pub const fn trailing_zeros(self) -> u32 {
        let mut x = self as usize;
        if x == 0 {
            return 64;
        }
        let mut n = 0;
        while x & 1 == 0 {
            x >>= 1;
            n += 1;
        }
        n
    }

    pub const fn rotate_left(self, n: u32) -> isize {
        let x = self as usize;
        let n = n % 64;
        if n == 0 {
            self
        } else {
            ((x << n) | (x >> (64 - n))) as isize
        }
    }

    pub const fn rotate_right(self, n: u32) -> isize {
        self.rotate_left((64 - n % 64) % 64)
    }

    pub const fn abs_diff(self, other: isize) -> usize {
        if self < other {
            (other as usize) - (self as usize)
        } else {
            (self as usize) - (other as usize)
        }
    }

    #[rustc_inherit_overflow_checks]
    pub const fn abs(self) -> isize {
        if self < 0 { -self } else { self }
    }

    pub const fn unsigned_abs(self) -> usize {
        if self < 0 {
            0 - (self as usize)
        } else {
            self as usize
        }
    }

    pub const fn signum(self) -> isize {
        if self < 0 {
            -1
        } else if self == 0 {
            0
        } else {
            1
        }
    }

    pub const fn is_positive(self) -> bool {
        self > 0
    }

    pub const fn is_negative(self) -> bool {
        self < 0
    }

    pub const fn rem_euclid(self, rhs: isize) -> isize {
        let r = self % rhs;
        if r < 0 {
            if rhs < 0 { r - rhs } else { r + rhs }
        } else {
            r
        }
    }
}

impl u8 {
    pub const MIN: u8 = 0;
    pub const MAX: u8 = 255;
    pub const BITS: u32 = 8;

    pub const fn wrapping_add(self, rhs: u8) -> u8 {
        self + rhs
    }

    pub const fn wrapping_sub(self, rhs: u8) -> u8 {
        self - rhs
    }

    pub const fn wrapping_mul(self, rhs: u8) -> u8 {
        self * rhs
    }

    pub const fn wrapping_neg(self) -> u8 {
        0 - self
    }

    #[rustc_inherit_overflow_checks]
    pub const fn pow(self, exp: u32) -> u8 {
        let mut base = self;
        let mut exp = exp;
        let mut acc: u8 = 1;
        while exp > 0 {
            if exp & 1 == 1 {
                acc = acc * base;
            }
            exp >>= 1;
            if exp > 0 {
                base = base * base;
            }
        }
        acc
    }

    pub const fn count_ones(self) -> u32 {
        let mut x = self as u8;
        let mut n = 0;
        while x != 0 {
            n += (x & 1) as u32;
            x >>= 1;
        }
        n
    }

    pub const fn count_zeros(self) -> u32 {
        8 - self.count_ones()
    }

    pub const fn leading_zeros(self) -> u32 {
        let x = self as u8;
        let mut n = 0;
        let mut bit = 8;
        while bit > 0 {
            bit -= 1;
            if (x >> bit) & 1 == 1 {
                return n;
            }
            n += 1;
        }
        n
    }

    pub const fn trailing_zeros(self) -> u32 {
        let mut x = self as u8;
        if x == 0 {
            return 8;
        }
        let mut n = 0;
        while x & 1 == 0 {
            x >>= 1;
            n += 1;
        }
        n
    }

    pub const fn rotate_left(self, n: u32) -> u8 {
        let x = self as u8;
        let n = n % 8;
        if n == 0 {
            self
        } else {
            ((x << n) | (x >> (8 - n))) as u8
        }
    }

    pub const fn rotate_right(self, n: u32) -> u8 {
        self.rotate_left((8 - n % 8) % 8)
    }

    pub const fn abs_diff(self, other: u8) -> u8 {
        if self < other {
            (other as u8) - (self as u8)
        } else {
            (self as u8) - (other as u8)
        }
    }

    pub const fn is_power_of_two(self) -> bool {
        self != 0 && self & (self - 1) == 0
    }
}

impl u16 {
    pub const MIN: u16 = 0;
    pub const MAX: u16 = 65535;
    pub const BITS: u32 = 16;

    pub const fn wrapping_add(self, rhs: u16) -> u16 {
        self + rhs
    }

    pub const fn wrapping_sub(self, rhs: u16) -> u16 {
        self - rhs
    }

    pub const fn wrapping_mul(self, rhs: u16) -> u16 {
        self * rhs
    }

    pub const fn wrapping_neg(self) -> u16 {
        0 - self
    }

    #[rustc_inherit_overflow_checks]
    pub const fn pow(self, exp: u32) -> u16 {
        let mut base = self;
        let mut exp = exp;
        let mut acc: u16 = 1;
        while exp > 0 {
            if exp & 1 == 1 {
                acc = acc * base;
            }
            exp >>= 1;
            if exp > 0 {
                base = base * base;
            }
        }
        acc
    }

    pub const fn count_ones(self) -> u32 {
        let mut x = self as u16;
        let mut n = 0;
        while x != 0 {
            n += (x & 1) as u32;
            x >>= 1;
        }
        n
    }

    pub const fn count_zeros(self) -> u32 {
        16 - self.count_ones()
    }

    pub const fn leading_zeros(self) -> u32 {
        let x = self as u16;
        let mut n = 0;
        let mut bit = 16;
        while bit > 0 {
            bit -= 1;
            if (x >> bit) & 1 == 1 {
                return n;
            }
            n += 1;
        }
        n
    }

    pub const fn trailing_zeros(self) -> u32 {
        let mut x = self as u16;
        if x == 0 {
            return 16;
        }
        let mut n = 0;
        while x & 1 == 0 {
            x >>= 1;
            n += 1;
        }
        n
    }

    pub const fn rotate_left(self, n: u32) -> u16 {
        let x = self as u16;
        let n = n % 16;
        if n == 0 {
            self
        } else {
            ((x << n) | (x >> (16 - n))) as u16
        }
    }

    pub const fn rotate_right(self, n: u32) -> u16 {
        self.rotate_left((16 - n % 16) % 16)
    }

    pub const fn abs_diff(self, other: u16) -> u16 {
        if self < other {
            (other as u16) - (self as u16)
        } else {
            (self as u16) - (other as u16)
        }
    }

    pub const fn is_power_of_two(self) -> bool {
        self != 0 && self & (self - 1) == 0
    }
}

impl u32 {
    pub const MIN: u32 = 0;
    pub const MAX: u32 = 4294967295;
    pub const BITS: u32 = 32;

    pub const fn wrapping_add(self, rhs: u32) -> u32 {
        self + rhs
    }

    pub const fn wrapping_sub(self, rhs: u32) -> u32 {
        self - rhs
    }

    pub const fn wrapping_mul(self, rhs: u32) -> u32 {
        self * rhs
    }

    pub const fn wrapping_neg(self) -> u32 {
        0 - self
    }

    #[rustc_inherit_overflow_checks]
    pub const fn pow(self, exp: u32) -> u32 {
        let mut base = self;
        let mut exp = exp;
        let mut acc: u32 = 1;
        while exp > 0 {
            if exp & 1 == 1 {
                acc = acc * base;
            }
            exp >>= 1;
            if exp > 0 {
                base = base * base;
            }
        }
        acc
    }

    pub const fn count_ones(self) -> u32 {
        let mut x = self as u32;
        let mut n = 0;
        while x != 0 {
            n += (x & 1) as u32;
            x >>= 1;
        }
        n
    }

    pub const fn count_zeros(self) -> u32 {
        32 - self.count_ones()
    }

    pub const fn leading_zeros(self) -> u32 {
        let x = self as u32;
        let mut n = 0;
        let mut bit = 32;
        while bit > 0 {
            bit -= 1;
            if (x >> bit) & 1 == 1 {
                return n;
            }
            n += 1;
        }
        n
    }

    pub const fn trailing_zeros(self) -> u32 {
        let mut x = self as u32;
        if x == 0 {
            return 32;
        }
        let mut n = 0;
        while x & 1 == 0 {
            x >>= 1;
            n += 1;
        }
        n
    }

    pub const fn rotate_left(self, n: u32) -> u32 {
        let x = self as u32;
        let n = n % 32;
        if n == 0 {
            self
        } else {
            ((x << n) | (x >> (32 - n))) as u32
        }
    }

    pub const fn rotate_right(self, n: u32) -> u32 {
        self.rotate_left((32 - n % 32) % 32)
    }

    pub const fn abs_diff(self, other: u32) -> u32 {
        if self < other {
            (other as u32) - (self as u32)
        } else {
            (self as u32) - (other as u32)
        }
    }

    pub const fn is_power_of_two(self) -> bool {
        self != 0 && self & (self - 1) == 0
    }
}

impl u64 {
    pub const MIN: u64 = 0;
    pub const MAX: u64 = 18446744073709551615;
    pub const BITS: u32 = 64;

    pub const fn wrapping_add(self, rhs: u64) -> u64 {
        self + rhs
    }

    pub const fn wrapping_sub(self, rhs: u64) -> u64 {
        self - rhs
    }

    pub const fn wrapping_mul(self, rhs: u64) -> u64 {
        self * rhs
    }

    pub const fn wrapping_neg(self) -> u64 {
        0 - self
    }

    #[rustc_inherit_overflow_checks]
    pub const fn pow(self, exp: u32) -> u64 {
        let mut base = self;
        let mut exp = exp;
        let mut acc: u64 = 1;
        while exp > 0 {
            if exp & 1 == 1 {
                acc = acc * base;
            }
            exp >>= 1;
            if exp > 0 {
                base = base * base;
            }
        }
        acc
    }

    pub const fn count_ones(self) -> u32 {
        let mut x = self as u64;
        let mut n = 0;
        while x != 0 {
            n += (x & 1) as u32;
            x >>= 1;
        }
        n
    }

    pub const fn count_zeros(self) -> u32 {
        64 - self.count_ones()
    }

    pub const fn leading_zeros(self) -> u32 {
        let x = self as u64;
        let mut n = 0;
        let mut bit = 64;
        while bit > 0 {
            bit -= 1;
            if (x >> bit) & 1 == 1 {
                return n;
            }
            n += 1;
        }
        n
    }

    pub const fn trailing_zeros(self) -> u32 {
        let mut x = self as u64;
        if x == 0 {
            return 64;
        }
        let mut n = 0;
        while x & 1 == 0 {
            x >>= 1;
            n += 1;
        }
        n
    }

    pub const fn rotate_left(self, n: u32) -> u64 {
        let x = self as u64;
        let n = n % 64;
        if n == 0 {
            self
        } else {
            ((x << n) | (x >> (64 - n))) as u64
        }
    }

    pub const fn rotate_right(self, n: u32) -> u64 {
        self.rotate_left((64 - n % 64) % 64)
    }

    pub const fn abs_diff(self, other: u64) -> u64 {
        if self < other {
            (other as u64) - (self as u64)
        } else {
            (self as u64) - (other as u64)
        }
    }

    pub const fn is_power_of_two(self) -> bool {
        self != 0 && self & (self - 1) == 0
    }
}

impl u128 {
    pub const MIN: u128 = 0;
    pub const MAX: u128 = 340282366920938463463374607431768211455;
    pub const BITS: u32 = 128;

    pub const fn wrapping_add(self, rhs: u128) -> u128 {
        self + rhs
    }

    pub const fn wrapping_sub(self, rhs: u128) -> u128 {
        self - rhs
    }

    pub const fn wrapping_mul(self, rhs: u128) -> u128 {
        self * rhs
    }

    pub const fn wrapping_neg(self) -> u128 {
        0 - self
    }

    #[rustc_inherit_overflow_checks]
    pub const fn pow(self, exp: u32) -> u128 {
        let mut base = self;
        let mut exp = exp;
        let mut acc: u128 = 1;
        while exp > 0 {
            if exp & 1 == 1 {
                acc = acc * base;
            }
            exp >>= 1;
            if exp > 0 {
                base = base * base;
            }
        }
        acc
    }

    pub const fn count_ones(self) -> u32 {
        let mut x = self as u128;
        let mut n = 0;
        while x != 0 {
            n += (x & 1) as u32;
            x >>= 1;
        }
        n
    }

    pub const fn count_zeros(self) -> u32 {
        128 - self.count_ones()
    }

    pub const fn leading_zeros(self) -> u32 {
        let x = self as u128;
        let mut n = 0;
        let mut bit = 128;
        while bit > 0 {
            bit -= 1;
            if (x >> bit) & 1 == 1 {
                return n;
            }
            n += 1;
        }
        n
    }

    pub const fn trailing_zeros(self) -> u32 {
        let mut x = self as u128;
        if x == 0 {
            return 128;
        }
        let mut n = 0;
        while x & 1 == 0 {
            x >>= 1;
            n += 1;
        }
        n
    }

    pub const fn rotate_left(self, n: u32) -> u128 {
        let x = self as u128;
        let n = n % 128;
        if n == 0 {
            self
        } else {
            ((x << n) | (x >> (128 - n))) as u128
        }
    }

    pub const fn rotate_right(self, n: u32) -> u128 {
        self.rotate_left((128 - n % 128) % 128)
    }

    pub const fn abs_diff(self, other: u128) -> u128 {
        if self < other {
            (other as u128) - (self as u128)
        } else {
            (self as u128) - (other as u128)
        }
    }

    pub const fn is_power_of_two(self) -> bool {
        self != 0 && self & (self - 1) == 0
    }
}

impl usize {
    pub const MIN: usize = 0;
    pub const MAX: usize = 18446744073709551615;
    pub const BITS: u32 = 64;

    pub const fn wrapping_add(self, rhs: usize) -> usize {
        self + rhs
    }

    pub const fn wrapping_sub(self, rhs: usize) -> usize {
        self - rhs
    }

    pub const fn wrapping_mul(self, rhs: usize) -> usize {
        self * rhs
    }

    pub const fn wrapping_neg(self) -> usize {
        0 - self
    }

    #[rustc_inherit_overflow_checks]
    pub const fn pow(self, exp: u32) -> usize {
        let mut base = self;
        let mut exp = exp;
        let mut acc: usize = 1;
        while exp > 0 {
            if exp & 1 == 1 {
                acc = acc * base;
            }
            exp >>= 1;
            if exp > 0 {
                base = base * base;
            }
        }
        acc
    }

    pub const fn count_ones(self) -> u32 {
        let mut x = self as usize;
        let mut n = 0;
        while x != 0 {
            n += (x & 1) as u32;
            x >>= 1;
        }
        n
    }

    pub const fn count_zeros(self) -> u32 {
        64 - self.count_ones()
    }

    pub const fn leading_zeros(self) -> u32 {
        let x = self as usize;
        let mut n = 0;
        let mut bit = 64;
        while bit > 0 {
            bit -= 1;
            if (x >> bit) & 1 == 1 {
                return n;
            }
            n += 1;
        }
        n
    }

    pub const fn trailing_zeros(self) -> u32 {
        let mut x = self as usize;
        if x == 0 {
            return 64;
        }
        let mut n = 0;
        while x & 1 == 0 {
            x >>= 1;
            n += 1;
        }
        n
    }

    pub const fn rotate_left(self, n: u32) -> usize {
        let x = self as usize;
        let n = n % 64;
        if n == 0 {
            self
        } else {
            ((x << n) | (x >> (64 - n))) as usize
        }
    }

    pub const fn rotate_right(self, n: u32) -> usize {
        self.rotate_left((64 - n % 64) % 64)
    }

    pub const fn abs_diff(self, other: usize) -> usize {
        if self < other {
            (other as usize) - (self as usize)
        } else {
            (self as usize) - (other as usize)
        }
    }

    pub const fn is_power_of_two(self) -> bool {
        self != 0 && self & (self - 1) == 0
    }
}

// Checked, wrapping, saturating and overflowing arithmetic, Euclidean
// division, logarithms and square roots, and byte order, as rustc's
// `core` has them for each integer type. Division by zero panics in
// those that divide, as `/` does; the `checked_` ones give `None`.

use crate::option::Option::{self, None, Some};

impl i8 {
    pub const fn checked_add(self, rhs: i8) -> Option<i8> {
        let (r, o) = self.overflowing_add(rhs);
        if o { None } else { Some(r) }
    }

    pub const fn checked_sub(self, rhs: i8) -> Option<i8> {
        let (r, o) = self.overflowing_sub(rhs);
        if o { None } else { Some(r) }
    }

    pub const fn checked_mul(self, rhs: i8) -> Option<i8> {
        let (r, o) = self.overflowing_mul(rhs);
        if o { None } else { Some(r) }
    }

    pub const fn checked_div(self, rhs: i8) -> Option<i8> {
        if rhs == 0 || (self == i8::MIN && rhs == -1) {
            None
        } else {
            Some(self / rhs)
        }
    }

    pub const fn checked_rem(self, rhs: i8) -> Option<i8> {
        if rhs == 0 || (self == i8::MIN && rhs == -1) {
            None
        } else {
            Some(self % rhs)
        }
    }

    pub const fn checked_div_euclid(self, rhs: i8) -> Option<i8> {
        if rhs == 0 || (self == i8::MIN && rhs == -1) {
            None
        } else {
            Some(self.div_euclid(rhs))
        }
    }

    pub const fn checked_rem_euclid(self, rhs: i8) -> Option<i8> {
        if rhs == 0 || (self == i8::MIN && rhs == -1) {
            None
        } else {
            Some(self.rem_euclid(rhs))
        }
    }

    pub const fn checked_neg(self) -> Option<i8> {
        let (r, o) = self.overflowing_neg();
        if o { None } else { Some(r) }
    }

    pub const fn checked_shl(self, rhs: u32) -> Option<i8> {
        if rhs < 8 { Some(self << rhs) } else { None }
    }

    pub const fn checked_shr(self, rhs: u32) -> Option<i8> {
        if rhs < 8 { Some(self >> rhs) } else { None }
    }

    pub const fn checked_pow(self, exp: u32) -> Option<i8> {
        if exp == 0 {
            return Some(1);
        }
        let mut base = self;
        let mut exp = exp;
        let mut acc: i8 = 1;
        loop {
            if exp & 1 == 1 {
                acc = match acc.checked_mul(base) {
                    Some(x) => x,
                    None => return None,
                };
                if exp == 1 {
                    return Some(acc);
                }
            }
            exp /= 2;
            base = match base.checked_mul(base) {
                Some(x) => x,
                None => return None,
            };
        }
    }

    pub const fn checked_ilog2(self) -> Option<u32> {
        if self <= 0 { None } else { Some(8 - 1 - self.leading_zeros()) }
    }

    pub const fn checked_ilog10(self) -> Option<u32> {
        self.checked_ilog(10)
    }

    pub const fn checked_ilog(self, base: i8) -> Option<u32> {
        if self <= 0 || base <= 1 {
            None
        } else {
            let mut n = 0;
            let mut r = self;
            while r >= base {
                r /= base;
                n += 1;
            }
            Some(n)
        }
    }

    #[track_caller]
    pub const fn ilog2(self) -> u32 {
        match self.checked_ilog2() {
            Some(n) => n,
            None => crate::panicking::panic("argument of integer logarithm must be positive"),
        }
    }

    #[track_caller]
    pub const fn ilog10(self) -> u32 {
        match self.checked_ilog10() {
            Some(n) => n,
            None => crate::panicking::panic("argument of integer logarithm must be positive"),
        }
    }

    #[track_caller]
    pub const fn ilog(self, base: i8) -> u32 {
        if base < 2 {
            crate::panicking::panic("base of integer logarithm must be at least 2");
        }
        match self.checked_ilog(base) {
            Some(n) => n,
            None => crate::panicking::panic("argument of integer logarithm must be positive"),
        }
    }

    pub const fn wrapping_div(self, rhs: i8) -> i8 {
        self.overflowing_div(rhs).0
    }

    pub const fn wrapping_rem(self, rhs: i8) -> i8 {
        self.overflowing_rem(rhs).0
    }

    pub const fn wrapping_div_euclid(self, rhs: i8) -> i8 {
        self.overflowing_div_euclid(rhs).0
    }

    pub const fn wrapping_rem_euclid(self, rhs: i8) -> i8 {
        self.overflowing_rem_euclid(rhs).0
    }

    pub const fn wrapping_shl(self, rhs: u32) -> i8 {
        self << (rhs & 7)
    }

    pub const fn wrapping_shr(self, rhs: u32) -> i8 {
        self >> (rhs & 7)
    }

    pub const fn wrapping_pow(self, exp: u32) -> i8 {
        if exp == 0 {
            return 1;
        }
        let mut base = self;
        let mut exp = exp;
        let mut acc: i8 = 1;
        loop {
            if exp & 1 == 1 {
                acc = acc.wrapping_mul(base);
                if exp == 1 {
                    return acc;
                }
            }
            exp /= 2;
            base = base.wrapping_mul(base);
        }
    }

    pub const fn overflowing_add(self, rhs: i8) -> (i8, bool) {
        let r = self.wrapping_add(rhs);
        (r, (r < self) != (rhs < 0))
    }

    pub const fn overflowing_sub(self, rhs: i8) -> (i8, bool) {
        let r = self.wrapping_sub(rhs);
        (r, (r > self) != (rhs < 0))
    }

    pub const fn overflowing_mul(self, rhs: i8) -> (i8, bool) {
        let r = self.wrapping_mul(rhs);

        let o = if self == -1 {
            rhs == i8::MIN
        } else if rhs == -1 {
            self == i8::MIN
        } else {
            self != 0 && r / self != rhs
        };
        (r, o)
    }

    pub const fn overflowing_div(self, rhs: i8) -> (i8, bool) {
        if self == i8::MIN && rhs == -1 { (self, true) } else { (self / rhs, false) }
    }

    pub const fn overflowing_rem(self, rhs: i8) -> (i8, bool) {
        if rhs == -1 { (0, self == i8::MIN) } else { (self % rhs, false) }
    }

    pub const fn overflowing_div_euclid(self, rhs: i8) -> (i8, bool) {
        if self == i8::MIN && rhs == -1 { (self, true) } else { (self.div_euclid(rhs), false) }
    }

    pub const fn overflowing_rem_euclid(self, rhs: i8) -> (i8, bool) {
        if rhs == -1 { (0, self == i8::MIN) } else { (self.rem_euclid(rhs), false) }
    }

    pub const fn overflowing_neg(self) -> (i8, bool) {
        (self.wrapping_neg(), self == i8::MIN)
    }

    pub const fn overflowing_shl(self, rhs: u32) -> (i8, bool) {
        (self.wrapping_shl(rhs), rhs >= 8)
    }

    pub const fn overflowing_shr(self, rhs: u32) -> (i8, bool) {
        (self.wrapping_shr(rhs), rhs >= 8)
    }

    pub const fn overflowing_pow(self, exp: u32) -> (i8, bool) {
        if exp == 0 {
            return (1, false);
        }
        let mut base = self;
        let mut exp = exp;
        let mut acc: i8 = 1;
        let mut overflown = false;
        loop {
            if exp & 1 == 1 {
                let (r, o) = acc.overflowing_mul(base);
                if exp == 1 {
                    return (r, o || overflown);
                }
                acc = r;
                overflown = overflown || o;
            }
            exp /= 2;
            let (r, o) = base.overflowing_mul(base);
            base = r;
            overflown = overflown || o;
        }
    }

    pub const fn saturating_add(self, rhs: i8) -> i8 {
        match self.checked_add(rhs) {
            Some(x) => x,
            None => if rhs < 0 { i8::MIN } else { i8::MAX },
        }
    }

    pub const fn saturating_sub(self, rhs: i8) -> i8 {
        match self.checked_sub(rhs) {
            Some(x) => x,
            None => if rhs < 0 { i8::MAX } else { i8::MIN },
        }
    }

    pub const fn saturating_mul(self, rhs: i8) -> i8 {
        match self.checked_mul(rhs) {
            Some(x) => x,
            None => if (self < 0) == (rhs < 0) { i8::MAX } else { i8::MIN },
        }
    }

    pub const fn saturating_div(self, rhs: i8) -> i8 {
        if self == i8::MIN && rhs == -1 { i8::MAX } else { self / rhs }
    }

    pub const fn saturating_pow(self, exp: u32) -> i8 {
        match self.checked_pow(exp) {
            Some(x) => x,
            None => if self < 0 && exp % 2 == 1 { i8::MIN } else { i8::MAX },
        }
    }

    pub const fn div_euclid(self, rhs: i8) -> i8 {
        let q = self / rhs;
        if self % rhs < 0 {
            return if rhs > 0 { q - 1 } else { q + 1 };
        }
        q
    }

    pub const fn checked_abs(self) -> Option<i8> {
        if self == i8::MIN { None } else { Some(self.wrapping_abs()) }
    }

    pub const fn wrapping_abs(self) -> i8 {
        if self < 0 { self.wrapping_neg() } else { self }
    }

    pub const fn overflowing_abs(self) -> (i8, bool) {
        (self.wrapping_abs(), self == i8::MIN)
    }

    pub const fn saturating_abs(self) -> i8 {
        if self == i8::MIN { i8::MAX } else { self.wrapping_abs() }
    }

    pub const fn saturating_neg(self) -> i8 {
        if self == i8::MIN { i8::MAX } else { 0 - self }
    }

    pub const fn checked_isqrt(self) -> Option<i8> {
        if self < 0 { None } else { Some((self as u8).isqrt() as i8) }
    }

    #[track_caller]
    pub const fn isqrt(self) -> i8 {
        match self.checked_isqrt() {
            Some(x) => x,
            None => crate::panicking::panic("argument of integer square root cannot be negative"),
        }
    }

    pub const fn midpoint(self, rhs: i8) -> i8 {
        let t = ((self ^ rhs) >> 1) + (self & rhs);
        t + (if t < 0 { 1 } else { 0 } & (self ^ rhs))
    }

    pub const fn cast_unsigned(self) -> u8 {
        self as u8
    }

    pub const fn checked_add_unsigned(self, rhs: u8) -> Option<i8> {
        let (r, o) = self.overflowing_add_unsigned(rhs);
        if o { None } else { Some(r) }
    }

    pub const fn checked_sub_unsigned(self, rhs: u8) -> Option<i8> {
        let (r, o) = self.overflowing_sub_unsigned(rhs);
        if o { None } else { Some(r) }
    }

    pub const fn wrapping_add_unsigned(self, rhs: u8) -> i8 {
        self.wrapping_add(rhs as i8)
    }

    pub const fn wrapping_sub_unsigned(self, rhs: u8) -> i8 {
        self.wrapping_sub(rhs as i8)
    }

    pub const fn saturating_add_unsigned(self, rhs: u8) -> i8 {
        match self.checked_add_unsigned(rhs) {
            Some(x) => x,
            None => i8::MAX,
        }
    }

    pub const fn saturating_sub_unsigned(self, rhs: u8) -> i8 {
        match self.checked_sub_unsigned(rhs) {
            Some(x) => x,
            None => i8::MIN,
        }
    }

    pub const fn overflowing_add_unsigned(self, rhs: u8) -> (i8, bool) {
        let rhs = rhs as i8;
        let (res, overflowed) = self.overflowing_add(rhs);
        (res, overflowed != (rhs < 0))
    }

    pub const fn overflowing_sub_unsigned(self, rhs: u8) -> (i8, bool) {
        let rhs = rhs as i8;
        let (res, overflowed) = self.overflowing_sub(rhs);
        (res, overflowed != (rhs < 0))
    }

    pub const fn unbounded_shl(self, rhs: u32) -> i8 {
        if rhs < 8 { self << rhs } else { 0 }
    }

    pub const fn unbounded_shr(self, rhs: u32) -> i8 {
        if rhs < 8 {
            self >> rhs
        } else if self < 0 {
            -1
        } else {
            0
        }
    }

    pub const fn leading_ones(self) -> u32 {
        (!self).leading_zeros()
    }

    pub const fn trailing_ones(self) -> u32 {
        (!self).trailing_zeros()
    }

    pub const fn swap_bytes(self) -> i8 {
        let mut x = self as u8;
        let mut r: u8 = 0;
        let mut i = 0;
        while i < 1 {
            r = (r << 8) | (x & 0xff);
            x >>= 8;
            i += 1;
        }
        r as i8
    }

    pub const fn reverse_bits(self) -> i8 {
        let mut x = self as u8;
        let mut r: u8 = 0;
        let mut i = 0;
        while i < 8 {
            r = (r << 1) | (x & 1);
            x >>= 1;
            i += 1;
        }
        r as i8
    }

    pub const fn to_be(self) -> i8 {
        self.swap_bytes()
    }

    pub const fn to_le(self) -> i8 {
        self
    }

    pub const fn from_be(x: i8) -> i8 {
        x.swap_bytes()
    }

    pub const fn from_le(x: i8) -> i8 {
        x
    }

    pub const fn to_le_bytes(self) -> [u8; 1] {
        let x = self as u8;
        [x as u8]
    }

    pub const fn to_be_bytes(self) -> [u8; 1] {
        self.swap_bytes().to_le_bytes()
    }

    pub const fn to_ne_bytes(self) -> [u8; 1] {
        self.to_le_bytes()
    }

    pub const fn from_le_bytes(bytes: [u8; 1]) -> i8 {
        (bytes[0] as u8) as i8
    }

    pub const fn from_be_bytes(bytes: [u8; 1]) -> i8 {
        i8::from_le_bytes(bytes).swap_bytes()
    }

    pub const fn from_ne_bytes(bytes: [u8; 1]) -> i8 {
        i8::from_le_bytes(bytes)
    }
}

impl i16 {
    pub const fn checked_add(self, rhs: i16) -> Option<i16> {
        let (r, o) = self.overflowing_add(rhs);
        if o { None } else { Some(r) }
    }

    pub const fn checked_sub(self, rhs: i16) -> Option<i16> {
        let (r, o) = self.overflowing_sub(rhs);
        if o { None } else { Some(r) }
    }

    pub const fn checked_mul(self, rhs: i16) -> Option<i16> {
        let (r, o) = self.overflowing_mul(rhs);
        if o { None } else { Some(r) }
    }

    pub const fn checked_div(self, rhs: i16) -> Option<i16> {
        if rhs == 0 || (self == i16::MIN && rhs == -1) {
            None
        } else {
            Some(self / rhs)
        }
    }

    pub const fn checked_rem(self, rhs: i16) -> Option<i16> {
        if rhs == 0 || (self == i16::MIN && rhs == -1) {
            None
        } else {
            Some(self % rhs)
        }
    }

    pub const fn checked_div_euclid(self, rhs: i16) -> Option<i16> {
        if rhs == 0 || (self == i16::MIN && rhs == -1) {
            None
        } else {
            Some(self.div_euclid(rhs))
        }
    }

    pub const fn checked_rem_euclid(self, rhs: i16) -> Option<i16> {
        if rhs == 0 || (self == i16::MIN && rhs == -1) {
            None
        } else {
            Some(self.rem_euclid(rhs))
        }
    }

    pub const fn checked_neg(self) -> Option<i16> {
        let (r, o) = self.overflowing_neg();
        if o { None } else { Some(r) }
    }

    pub const fn checked_shl(self, rhs: u32) -> Option<i16> {
        if rhs < 16 { Some(self << rhs) } else { None }
    }

    pub const fn checked_shr(self, rhs: u32) -> Option<i16> {
        if rhs < 16 { Some(self >> rhs) } else { None }
    }

    pub const fn checked_pow(self, exp: u32) -> Option<i16> {
        if exp == 0 {
            return Some(1);
        }
        let mut base = self;
        let mut exp = exp;
        let mut acc: i16 = 1;
        loop {
            if exp & 1 == 1 {
                acc = match acc.checked_mul(base) {
                    Some(x) => x,
                    None => return None,
                };
                if exp == 1 {
                    return Some(acc);
                }
            }
            exp /= 2;
            base = match base.checked_mul(base) {
                Some(x) => x,
                None => return None,
            };
        }
    }

    pub const fn checked_ilog2(self) -> Option<u32> {
        if self <= 0 { None } else { Some(16 - 1 - self.leading_zeros()) }
    }

    pub const fn checked_ilog10(self) -> Option<u32> {
        self.checked_ilog(10)
    }

    pub const fn checked_ilog(self, base: i16) -> Option<u32> {
        if self <= 0 || base <= 1 {
            None
        } else {
            let mut n = 0;
            let mut r = self;
            while r >= base {
                r /= base;
                n += 1;
            }
            Some(n)
        }
    }

    #[track_caller]
    pub const fn ilog2(self) -> u32 {
        match self.checked_ilog2() {
            Some(n) => n,
            None => crate::panicking::panic("argument of integer logarithm must be positive"),
        }
    }

    #[track_caller]
    pub const fn ilog10(self) -> u32 {
        match self.checked_ilog10() {
            Some(n) => n,
            None => crate::panicking::panic("argument of integer logarithm must be positive"),
        }
    }

    #[track_caller]
    pub const fn ilog(self, base: i16) -> u32 {
        if base < 2 {
            crate::panicking::panic("base of integer logarithm must be at least 2");
        }
        match self.checked_ilog(base) {
            Some(n) => n,
            None => crate::panicking::panic("argument of integer logarithm must be positive"),
        }
    }

    pub const fn wrapping_div(self, rhs: i16) -> i16 {
        self.overflowing_div(rhs).0
    }

    pub const fn wrapping_rem(self, rhs: i16) -> i16 {
        self.overflowing_rem(rhs).0
    }

    pub const fn wrapping_div_euclid(self, rhs: i16) -> i16 {
        self.overflowing_div_euclid(rhs).0
    }

    pub const fn wrapping_rem_euclid(self, rhs: i16) -> i16 {
        self.overflowing_rem_euclid(rhs).0
    }

    pub const fn wrapping_shl(self, rhs: u32) -> i16 {
        self << (rhs & 15)
    }

    pub const fn wrapping_shr(self, rhs: u32) -> i16 {
        self >> (rhs & 15)
    }

    pub const fn wrapping_pow(self, exp: u32) -> i16 {
        if exp == 0 {
            return 1;
        }
        let mut base = self;
        let mut exp = exp;
        let mut acc: i16 = 1;
        loop {
            if exp & 1 == 1 {
                acc = acc.wrapping_mul(base);
                if exp == 1 {
                    return acc;
                }
            }
            exp /= 2;
            base = base.wrapping_mul(base);
        }
    }

    pub const fn overflowing_add(self, rhs: i16) -> (i16, bool) {
        let r = self.wrapping_add(rhs);
        (r, (r < self) != (rhs < 0))
    }

    pub const fn overflowing_sub(self, rhs: i16) -> (i16, bool) {
        let r = self.wrapping_sub(rhs);
        (r, (r > self) != (rhs < 0))
    }

    pub const fn overflowing_mul(self, rhs: i16) -> (i16, bool) {
        let r = self.wrapping_mul(rhs);

        let o = if self == -1 {
            rhs == i16::MIN
        } else if rhs == -1 {
            self == i16::MIN
        } else {
            self != 0 && r / self != rhs
        };
        (r, o)
    }

    pub const fn overflowing_div(self, rhs: i16) -> (i16, bool) {
        if self == i16::MIN && rhs == -1 { (self, true) } else { (self / rhs, false) }
    }

    pub const fn overflowing_rem(self, rhs: i16) -> (i16, bool) {
        if rhs == -1 { (0, self == i16::MIN) } else { (self % rhs, false) }
    }

    pub const fn overflowing_div_euclid(self, rhs: i16) -> (i16, bool) {
        if self == i16::MIN && rhs == -1 { (self, true) } else { (self.div_euclid(rhs), false) }
    }

    pub const fn overflowing_rem_euclid(self, rhs: i16) -> (i16, bool) {
        if rhs == -1 { (0, self == i16::MIN) } else { (self.rem_euclid(rhs), false) }
    }

    pub const fn overflowing_neg(self) -> (i16, bool) {
        (self.wrapping_neg(), self == i16::MIN)
    }

    pub const fn overflowing_shl(self, rhs: u32) -> (i16, bool) {
        (self.wrapping_shl(rhs), rhs >= 16)
    }

    pub const fn overflowing_shr(self, rhs: u32) -> (i16, bool) {
        (self.wrapping_shr(rhs), rhs >= 16)
    }

    pub const fn overflowing_pow(self, exp: u32) -> (i16, bool) {
        if exp == 0 {
            return (1, false);
        }
        let mut base = self;
        let mut exp = exp;
        let mut acc: i16 = 1;
        let mut overflown = false;
        loop {
            if exp & 1 == 1 {
                let (r, o) = acc.overflowing_mul(base);
                if exp == 1 {
                    return (r, o || overflown);
                }
                acc = r;
                overflown = overflown || o;
            }
            exp /= 2;
            let (r, o) = base.overflowing_mul(base);
            base = r;
            overflown = overflown || o;
        }
    }

    pub const fn saturating_add(self, rhs: i16) -> i16 {
        match self.checked_add(rhs) {
            Some(x) => x,
            None => if rhs < 0 { i16::MIN } else { i16::MAX },
        }
    }

    pub const fn saturating_sub(self, rhs: i16) -> i16 {
        match self.checked_sub(rhs) {
            Some(x) => x,
            None => if rhs < 0 { i16::MAX } else { i16::MIN },
        }
    }

    pub const fn saturating_mul(self, rhs: i16) -> i16 {
        match self.checked_mul(rhs) {
            Some(x) => x,
            None => if (self < 0) == (rhs < 0) { i16::MAX } else { i16::MIN },
        }
    }

    pub const fn saturating_div(self, rhs: i16) -> i16 {
        if self == i16::MIN && rhs == -1 { i16::MAX } else { self / rhs }
    }

    pub const fn saturating_pow(self, exp: u32) -> i16 {
        match self.checked_pow(exp) {
            Some(x) => x,
            None => if self < 0 && exp % 2 == 1 { i16::MIN } else { i16::MAX },
        }
    }

    pub const fn div_euclid(self, rhs: i16) -> i16 {
        let q = self / rhs;
        if self % rhs < 0 {
            return if rhs > 0 { q - 1 } else { q + 1 };
        }
        q
    }

    pub const fn checked_abs(self) -> Option<i16> {
        if self == i16::MIN { None } else { Some(self.wrapping_abs()) }
    }

    pub const fn wrapping_abs(self) -> i16 {
        if self < 0 { self.wrapping_neg() } else { self }
    }

    pub const fn overflowing_abs(self) -> (i16, bool) {
        (self.wrapping_abs(), self == i16::MIN)
    }

    pub const fn saturating_abs(self) -> i16 {
        if self == i16::MIN { i16::MAX } else { self.wrapping_abs() }
    }

    pub const fn saturating_neg(self) -> i16 {
        if self == i16::MIN { i16::MAX } else { 0 - self }
    }

    pub const fn checked_isqrt(self) -> Option<i16> {
        if self < 0 { None } else { Some((self as u16).isqrt() as i16) }
    }

    #[track_caller]
    pub const fn isqrt(self) -> i16 {
        match self.checked_isqrt() {
            Some(x) => x,
            None => crate::panicking::panic("argument of integer square root cannot be negative"),
        }
    }

    pub const fn midpoint(self, rhs: i16) -> i16 {
        let t = ((self ^ rhs) >> 1) + (self & rhs);
        t + (if t < 0 { 1 } else { 0 } & (self ^ rhs))
    }

    pub const fn cast_unsigned(self) -> u16 {
        self as u16
    }

    pub const fn checked_add_unsigned(self, rhs: u16) -> Option<i16> {
        let (r, o) = self.overflowing_add_unsigned(rhs);
        if o { None } else { Some(r) }
    }

    pub const fn checked_sub_unsigned(self, rhs: u16) -> Option<i16> {
        let (r, o) = self.overflowing_sub_unsigned(rhs);
        if o { None } else { Some(r) }
    }

    pub const fn wrapping_add_unsigned(self, rhs: u16) -> i16 {
        self.wrapping_add(rhs as i16)
    }

    pub const fn wrapping_sub_unsigned(self, rhs: u16) -> i16 {
        self.wrapping_sub(rhs as i16)
    }

    pub const fn saturating_add_unsigned(self, rhs: u16) -> i16 {
        match self.checked_add_unsigned(rhs) {
            Some(x) => x,
            None => i16::MAX,
        }
    }

    pub const fn saturating_sub_unsigned(self, rhs: u16) -> i16 {
        match self.checked_sub_unsigned(rhs) {
            Some(x) => x,
            None => i16::MIN,
        }
    }

    pub const fn overflowing_add_unsigned(self, rhs: u16) -> (i16, bool) {
        let rhs = rhs as i16;
        let (res, overflowed) = self.overflowing_add(rhs);
        (res, overflowed != (rhs < 0))
    }

    pub const fn overflowing_sub_unsigned(self, rhs: u16) -> (i16, bool) {
        let rhs = rhs as i16;
        let (res, overflowed) = self.overflowing_sub(rhs);
        (res, overflowed != (rhs < 0))
    }

    pub const fn unbounded_shl(self, rhs: u32) -> i16 {
        if rhs < 16 { self << rhs } else { 0 }
    }

    pub const fn unbounded_shr(self, rhs: u32) -> i16 {
        if rhs < 16 {
            self >> rhs
        } else if self < 0 {
            -1
        } else {
            0
        }
    }

    pub const fn leading_ones(self) -> u32 {
        (!self).leading_zeros()
    }

    pub const fn trailing_ones(self) -> u32 {
        (!self).trailing_zeros()
    }

    pub const fn swap_bytes(self) -> i16 {
        let mut x = self as u16;
        let mut r: u16 = 0;
        let mut i = 0;
        while i < 2 {
            r = (r << 8) | (x & 0xff);
            x >>= 8;
            i += 1;
        }
        r as i16
    }

    pub const fn reverse_bits(self) -> i16 {
        let mut x = self as u16;
        let mut r: u16 = 0;
        let mut i = 0;
        while i < 16 {
            r = (r << 1) | (x & 1);
            x >>= 1;
            i += 1;
        }
        r as i16
    }

    pub const fn to_be(self) -> i16 {
        self.swap_bytes()
    }

    pub const fn to_le(self) -> i16 {
        self
    }

    pub const fn from_be(x: i16) -> i16 {
        x.swap_bytes()
    }

    pub const fn from_le(x: i16) -> i16 {
        x
    }

    pub const fn to_le_bytes(self) -> [u8; 2] {
        let x = self as u16;
        [x as u8, (x >> 8) as u8]
    }

    pub const fn to_be_bytes(self) -> [u8; 2] {
        self.swap_bytes().to_le_bytes()
    }

    pub const fn to_ne_bytes(self) -> [u8; 2] {
        self.to_le_bytes()
    }

    pub const fn from_le_bytes(bytes: [u8; 2]) -> i16 {
        (bytes[0] as u16 | (bytes[1] as u16) << 8) as i16
    }

    pub const fn from_be_bytes(bytes: [u8; 2]) -> i16 {
        i16::from_le_bytes(bytes).swap_bytes()
    }

    pub const fn from_ne_bytes(bytes: [u8; 2]) -> i16 {
        i16::from_le_bytes(bytes)
    }
}

impl i32 {
    pub const fn checked_add(self, rhs: i32) -> Option<i32> {
        let (r, o) = self.overflowing_add(rhs);
        if o { None } else { Some(r) }
    }

    pub const fn checked_sub(self, rhs: i32) -> Option<i32> {
        let (r, o) = self.overflowing_sub(rhs);
        if o { None } else { Some(r) }
    }

    pub const fn checked_mul(self, rhs: i32) -> Option<i32> {
        let (r, o) = self.overflowing_mul(rhs);
        if o { None } else { Some(r) }
    }

    pub const fn checked_div(self, rhs: i32) -> Option<i32> {
        if rhs == 0 || (self == i32::MIN && rhs == -1) {
            None
        } else {
            Some(self / rhs)
        }
    }

    pub const fn checked_rem(self, rhs: i32) -> Option<i32> {
        if rhs == 0 || (self == i32::MIN && rhs == -1) {
            None
        } else {
            Some(self % rhs)
        }
    }

    pub const fn checked_div_euclid(self, rhs: i32) -> Option<i32> {
        if rhs == 0 || (self == i32::MIN && rhs == -1) {
            None
        } else {
            Some(self.div_euclid(rhs))
        }
    }

    pub const fn checked_rem_euclid(self, rhs: i32) -> Option<i32> {
        if rhs == 0 || (self == i32::MIN && rhs == -1) {
            None
        } else {
            Some(self.rem_euclid(rhs))
        }
    }

    pub const fn checked_neg(self) -> Option<i32> {
        let (r, o) = self.overflowing_neg();
        if o { None } else { Some(r) }
    }

    pub const fn checked_shl(self, rhs: u32) -> Option<i32> {
        if rhs < 32 { Some(self << rhs) } else { None }
    }

    pub const fn checked_shr(self, rhs: u32) -> Option<i32> {
        if rhs < 32 { Some(self >> rhs) } else { None }
    }

    pub const fn checked_pow(self, exp: u32) -> Option<i32> {
        if exp == 0 {
            return Some(1);
        }
        let mut base = self;
        let mut exp = exp;
        let mut acc: i32 = 1;
        loop {
            if exp & 1 == 1 {
                acc = match acc.checked_mul(base) {
                    Some(x) => x,
                    None => return None,
                };
                if exp == 1 {
                    return Some(acc);
                }
            }
            exp /= 2;
            base = match base.checked_mul(base) {
                Some(x) => x,
                None => return None,
            };
        }
    }

    pub const fn checked_ilog2(self) -> Option<u32> {
        if self <= 0 { None } else { Some(32 - 1 - self.leading_zeros()) }
    }

    pub const fn checked_ilog10(self) -> Option<u32> {
        self.checked_ilog(10)
    }

    pub const fn checked_ilog(self, base: i32) -> Option<u32> {
        if self <= 0 || base <= 1 {
            None
        } else {
            let mut n = 0;
            let mut r = self;
            while r >= base {
                r /= base;
                n += 1;
            }
            Some(n)
        }
    }

    #[track_caller]
    pub const fn ilog2(self) -> u32 {
        match self.checked_ilog2() {
            Some(n) => n,
            None => crate::panicking::panic("argument of integer logarithm must be positive"),
        }
    }

    #[track_caller]
    pub const fn ilog10(self) -> u32 {
        match self.checked_ilog10() {
            Some(n) => n,
            None => crate::panicking::panic("argument of integer logarithm must be positive"),
        }
    }

    #[track_caller]
    pub const fn ilog(self, base: i32) -> u32 {
        if base < 2 {
            crate::panicking::panic("base of integer logarithm must be at least 2");
        }
        match self.checked_ilog(base) {
            Some(n) => n,
            None => crate::panicking::panic("argument of integer logarithm must be positive"),
        }
    }

    pub const fn wrapping_div(self, rhs: i32) -> i32 {
        self.overflowing_div(rhs).0
    }

    pub const fn wrapping_rem(self, rhs: i32) -> i32 {
        self.overflowing_rem(rhs).0
    }

    pub const fn wrapping_div_euclid(self, rhs: i32) -> i32 {
        self.overflowing_div_euclid(rhs).0
    }

    pub const fn wrapping_rem_euclid(self, rhs: i32) -> i32 {
        self.overflowing_rem_euclid(rhs).0
    }

    pub const fn wrapping_shl(self, rhs: u32) -> i32 {
        self << (rhs & 31)
    }

    pub const fn wrapping_shr(self, rhs: u32) -> i32 {
        self >> (rhs & 31)
    }

    pub const fn wrapping_pow(self, exp: u32) -> i32 {
        if exp == 0 {
            return 1;
        }
        let mut base = self;
        let mut exp = exp;
        let mut acc: i32 = 1;
        loop {
            if exp & 1 == 1 {
                acc = acc.wrapping_mul(base);
                if exp == 1 {
                    return acc;
                }
            }
            exp /= 2;
            base = base.wrapping_mul(base);
        }
    }

    pub const fn overflowing_add(self, rhs: i32) -> (i32, bool) {
        let r = self.wrapping_add(rhs);
        (r, (r < self) != (rhs < 0))
    }

    pub const fn overflowing_sub(self, rhs: i32) -> (i32, bool) {
        let r = self.wrapping_sub(rhs);
        (r, (r > self) != (rhs < 0))
    }

    pub const fn overflowing_mul(self, rhs: i32) -> (i32, bool) {
        let r = self.wrapping_mul(rhs);

        let o = if self == -1 {
            rhs == i32::MIN
        } else if rhs == -1 {
            self == i32::MIN
        } else {
            self != 0 && r / self != rhs
        };
        (r, o)
    }

    pub const fn overflowing_div(self, rhs: i32) -> (i32, bool) {
        if self == i32::MIN && rhs == -1 { (self, true) } else { (self / rhs, false) }
    }

    pub const fn overflowing_rem(self, rhs: i32) -> (i32, bool) {
        if rhs == -1 { (0, self == i32::MIN) } else { (self % rhs, false) }
    }

    pub const fn overflowing_div_euclid(self, rhs: i32) -> (i32, bool) {
        if self == i32::MIN && rhs == -1 { (self, true) } else { (self.div_euclid(rhs), false) }
    }

    pub const fn overflowing_rem_euclid(self, rhs: i32) -> (i32, bool) {
        if rhs == -1 { (0, self == i32::MIN) } else { (self.rem_euclid(rhs), false) }
    }

    pub const fn overflowing_neg(self) -> (i32, bool) {
        (self.wrapping_neg(), self == i32::MIN)
    }

    pub const fn overflowing_shl(self, rhs: u32) -> (i32, bool) {
        (self.wrapping_shl(rhs), rhs >= 32)
    }

    pub const fn overflowing_shr(self, rhs: u32) -> (i32, bool) {
        (self.wrapping_shr(rhs), rhs >= 32)
    }

    pub const fn overflowing_pow(self, exp: u32) -> (i32, bool) {
        if exp == 0 {
            return (1, false);
        }
        let mut base = self;
        let mut exp = exp;
        let mut acc: i32 = 1;
        let mut overflown = false;
        loop {
            if exp & 1 == 1 {
                let (r, o) = acc.overflowing_mul(base);
                if exp == 1 {
                    return (r, o || overflown);
                }
                acc = r;
                overflown = overflown || o;
            }
            exp /= 2;
            let (r, o) = base.overflowing_mul(base);
            base = r;
            overflown = overflown || o;
        }
    }

    pub const fn saturating_add(self, rhs: i32) -> i32 {
        match self.checked_add(rhs) {
            Some(x) => x,
            None => if rhs < 0 { i32::MIN } else { i32::MAX },
        }
    }

    pub const fn saturating_sub(self, rhs: i32) -> i32 {
        match self.checked_sub(rhs) {
            Some(x) => x,
            None => if rhs < 0 { i32::MAX } else { i32::MIN },
        }
    }

    pub const fn saturating_mul(self, rhs: i32) -> i32 {
        match self.checked_mul(rhs) {
            Some(x) => x,
            None => if (self < 0) == (rhs < 0) { i32::MAX } else { i32::MIN },
        }
    }

    pub const fn saturating_div(self, rhs: i32) -> i32 {
        if self == i32::MIN && rhs == -1 { i32::MAX } else { self / rhs }
    }

    pub const fn saturating_pow(self, exp: u32) -> i32 {
        match self.checked_pow(exp) {
            Some(x) => x,
            None => if self < 0 && exp % 2 == 1 { i32::MIN } else { i32::MAX },
        }
    }

    pub const fn div_euclid(self, rhs: i32) -> i32 {
        let q = self / rhs;
        if self % rhs < 0 {
            return if rhs > 0 { q - 1 } else { q + 1 };
        }
        q
    }

    pub const fn checked_abs(self) -> Option<i32> {
        if self == i32::MIN { None } else { Some(self.wrapping_abs()) }
    }

    pub const fn wrapping_abs(self) -> i32 {
        if self < 0 { self.wrapping_neg() } else { self }
    }

    pub const fn overflowing_abs(self) -> (i32, bool) {
        (self.wrapping_abs(), self == i32::MIN)
    }

    pub const fn saturating_abs(self) -> i32 {
        if self == i32::MIN { i32::MAX } else { self.wrapping_abs() }
    }

    pub const fn saturating_neg(self) -> i32 {
        if self == i32::MIN { i32::MAX } else { 0 - self }
    }

    pub const fn checked_isqrt(self) -> Option<i32> {
        if self < 0 { None } else { Some((self as u32).isqrt() as i32) }
    }

    #[track_caller]
    pub const fn isqrt(self) -> i32 {
        match self.checked_isqrt() {
            Some(x) => x,
            None => crate::panicking::panic("argument of integer square root cannot be negative"),
        }
    }

    pub const fn midpoint(self, rhs: i32) -> i32 {
        let t = ((self ^ rhs) >> 1) + (self & rhs);
        t + (if t < 0 { 1 } else { 0 } & (self ^ rhs))
    }

    pub const fn cast_unsigned(self) -> u32 {
        self as u32
    }

    pub const fn checked_add_unsigned(self, rhs: u32) -> Option<i32> {
        let (r, o) = self.overflowing_add_unsigned(rhs);
        if o { None } else { Some(r) }
    }

    pub const fn checked_sub_unsigned(self, rhs: u32) -> Option<i32> {
        let (r, o) = self.overflowing_sub_unsigned(rhs);
        if o { None } else { Some(r) }
    }

    pub const fn wrapping_add_unsigned(self, rhs: u32) -> i32 {
        self.wrapping_add(rhs as i32)
    }

    pub const fn wrapping_sub_unsigned(self, rhs: u32) -> i32 {
        self.wrapping_sub(rhs as i32)
    }

    pub const fn saturating_add_unsigned(self, rhs: u32) -> i32 {
        match self.checked_add_unsigned(rhs) {
            Some(x) => x,
            None => i32::MAX,
        }
    }

    pub const fn saturating_sub_unsigned(self, rhs: u32) -> i32 {
        match self.checked_sub_unsigned(rhs) {
            Some(x) => x,
            None => i32::MIN,
        }
    }

    pub const fn overflowing_add_unsigned(self, rhs: u32) -> (i32, bool) {
        let rhs = rhs as i32;
        let (res, overflowed) = self.overflowing_add(rhs);
        (res, overflowed != (rhs < 0))
    }

    pub const fn overflowing_sub_unsigned(self, rhs: u32) -> (i32, bool) {
        let rhs = rhs as i32;
        let (res, overflowed) = self.overflowing_sub(rhs);
        (res, overflowed != (rhs < 0))
    }

    pub const fn unbounded_shl(self, rhs: u32) -> i32 {
        if rhs < 32 { self << rhs } else { 0 }
    }

    pub const fn unbounded_shr(self, rhs: u32) -> i32 {
        if rhs < 32 {
            self >> rhs
        } else if self < 0 {
            -1
        } else {
            0
        }
    }

    pub const fn leading_ones(self) -> u32 {
        (!self).leading_zeros()
    }

    pub const fn trailing_ones(self) -> u32 {
        (!self).trailing_zeros()
    }

    pub const fn swap_bytes(self) -> i32 {
        let mut x = self as u32;
        let mut r: u32 = 0;
        let mut i = 0;
        while i < 4 {
            r = (r << 8) | (x & 0xff);
            x >>= 8;
            i += 1;
        }
        r as i32
    }

    pub const fn reverse_bits(self) -> i32 {
        let mut x = self as u32;
        let mut r: u32 = 0;
        let mut i = 0;
        while i < 32 {
            r = (r << 1) | (x & 1);
            x >>= 1;
            i += 1;
        }
        r as i32
    }

    pub const fn to_be(self) -> i32 {
        self.swap_bytes()
    }

    pub const fn to_le(self) -> i32 {
        self
    }

    pub const fn from_be(x: i32) -> i32 {
        x.swap_bytes()
    }

    pub const fn from_le(x: i32) -> i32 {
        x
    }

    pub const fn to_le_bytes(self) -> [u8; 4] {
        let x = self as u32;
        [x as u8, (x >> 8) as u8, (x >> 16) as u8, (x >> 24) as u8]
    }

    pub const fn to_be_bytes(self) -> [u8; 4] {
        self.swap_bytes().to_le_bytes()
    }

    pub const fn to_ne_bytes(self) -> [u8; 4] {
        self.to_le_bytes()
    }

    pub const fn from_le_bytes(bytes: [u8; 4]) -> i32 {
        (bytes[0] as u32 | (bytes[1] as u32) << 8 | (bytes[2] as u32) << 16 | (bytes[3] as u32) << 24) as i32
    }

    pub const fn from_be_bytes(bytes: [u8; 4]) -> i32 {
        i32::from_le_bytes(bytes).swap_bytes()
    }

    pub const fn from_ne_bytes(bytes: [u8; 4]) -> i32 {
        i32::from_le_bytes(bytes)
    }
}

impl i64 {
    pub const fn checked_add(self, rhs: i64) -> Option<i64> {
        let (r, o) = self.overflowing_add(rhs);
        if o { None } else { Some(r) }
    }

    pub const fn checked_sub(self, rhs: i64) -> Option<i64> {
        let (r, o) = self.overflowing_sub(rhs);
        if o { None } else { Some(r) }
    }

    pub const fn checked_mul(self, rhs: i64) -> Option<i64> {
        let (r, o) = self.overflowing_mul(rhs);
        if o { None } else { Some(r) }
    }

    pub const fn checked_div(self, rhs: i64) -> Option<i64> {
        if rhs == 0 || (self == i64::MIN && rhs == -1) {
            None
        } else {
            Some(self / rhs)
        }
    }

    pub const fn checked_rem(self, rhs: i64) -> Option<i64> {
        if rhs == 0 || (self == i64::MIN && rhs == -1) {
            None
        } else {
            Some(self % rhs)
        }
    }

    pub const fn checked_div_euclid(self, rhs: i64) -> Option<i64> {
        if rhs == 0 || (self == i64::MIN && rhs == -1) {
            None
        } else {
            Some(self.div_euclid(rhs))
        }
    }

    pub const fn checked_rem_euclid(self, rhs: i64) -> Option<i64> {
        if rhs == 0 || (self == i64::MIN && rhs == -1) {
            None
        } else {
            Some(self.rem_euclid(rhs))
        }
    }

    pub const fn checked_neg(self) -> Option<i64> {
        let (r, o) = self.overflowing_neg();
        if o { None } else { Some(r) }
    }

    pub const fn checked_shl(self, rhs: u32) -> Option<i64> {
        if rhs < 64 { Some(self << rhs) } else { None }
    }

    pub const fn checked_shr(self, rhs: u32) -> Option<i64> {
        if rhs < 64 { Some(self >> rhs) } else { None }
    }

    pub const fn checked_pow(self, exp: u32) -> Option<i64> {
        if exp == 0 {
            return Some(1);
        }
        let mut base = self;
        let mut exp = exp;
        let mut acc: i64 = 1;
        loop {
            if exp & 1 == 1 {
                acc = match acc.checked_mul(base) {
                    Some(x) => x,
                    None => return None,
                };
                if exp == 1 {
                    return Some(acc);
                }
            }
            exp /= 2;
            base = match base.checked_mul(base) {
                Some(x) => x,
                None => return None,
            };
        }
    }

    pub const fn checked_ilog2(self) -> Option<u32> {
        if self <= 0 { None } else { Some(64 - 1 - self.leading_zeros()) }
    }

    pub const fn checked_ilog10(self) -> Option<u32> {
        self.checked_ilog(10)
    }

    pub const fn checked_ilog(self, base: i64) -> Option<u32> {
        if self <= 0 || base <= 1 {
            None
        } else {
            let mut n = 0;
            let mut r = self;
            while r >= base {
                r /= base;
                n += 1;
            }
            Some(n)
        }
    }

    #[track_caller]
    pub const fn ilog2(self) -> u32 {
        match self.checked_ilog2() {
            Some(n) => n,
            None => crate::panicking::panic("argument of integer logarithm must be positive"),
        }
    }

    #[track_caller]
    pub const fn ilog10(self) -> u32 {
        match self.checked_ilog10() {
            Some(n) => n,
            None => crate::panicking::panic("argument of integer logarithm must be positive"),
        }
    }

    #[track_caller]
    pub const fn ilog(self, base: i64) -> u32 {
        if base < 2 {
            crate::panicking::panic("base of integer logarithm must be at least 2");
        }
        match self.checked_ilog(base) {
            Some(n) => n,
            None => crate::panicking::panic("argument of integer logarithm must be positive"),
        }
    }

    pub const fn wrapping_div(self, rhs: i64) -> i64 {
        self.overflowing_div(rhs).0
    }

    pub const fn wrapping_rem(self, rhs: i64) -> i64 {
        self.overflowing_rem(rhs).0
    }

    pub const fn wrapping_div_euclid(self, rhs: i64) -> i64 {
        self.overflowing_div_euclid(rhs).0
    }

    pub const fn wrapping_rem_euclid(self, rhs: i64) -> i64 {
        self.overflowing_rem_euclid(rhs).0
    }

    pub const fn wrapping_shl(self, rhs: u32) -> i64 {
        self << (rhs & 63)
    }

    pub const fn wrapping_shr(self, rhs: u32) -> i64 {
        self >> (rhs & 63)
    }

    pub const fn wrapping_pow(self, exp: u32) -> i64 {
        if exp == 0 {
            return 1;
        }
        let mut base = self;
        let mut exp = exp;
        let mut acc: i64 = 1;
        loop {
            if exp & 1 == 1 {
                acc = acc.wrapping_mul(base);
                if exp == 1 {
                    return acc;
                }
            }
            exp /= 2;
            base = base.wrapping_mul(base);
        }
    }

    pub const fn overflowing_add(self, rhs: i64) -> (i64, bool) {
        let r = self.wrapping_add(rhs);
        (r, (r < self) != (rhs < 0))
    }

    pub const fn overflowing_sub(self, rhs: i64) -> (i64, bool) {
        let r = self.wrapping_sub(rhs);
        (r, (r > self) != (rhs < 0))
    }

    pub const fn overflowing_mul(self, rhs: i64) -> (i64, bool) {
        let r = self.wrapping_mul(rhs);

        let o = if self == -1 {
            rhs == i64::MIN
        } else if rhs == -1 {
            self == i64::MIN
        } else {
            self != 0 && r / self != rhs
        };
        (r, o)
    }

    pub const fn overflowing_div(self, rhs: i64) -> (i64, bool) {
        if self == i64::MIN && rhs == -1 { (self, true) } else { (self / rhs, false) }
    }

    pub const fn overflowing_rem(self, rhs: i64) -> (i64, bool) {
        if rhs == -1 { (0, self == i64::MIN) } else { (self % rhs, false) }
    }

    pub const fn overflowing_div_euclid(self, rhs: i64) -> (i64, bool) {
        if self == i64::MIN && rhs == -1 { (self, true) } else { (self.div_euclid(rhs), false) }
    }

    pub const fn overflowing_rem_euclid(self, rhs: i64) -> (i64, bool) {
        if rhs == -1 { (0, self == i64::MIN) } else { (self.rem_euclid(rhs), false) }
    }

    pub const fn overflowing_neg(self) -> (i64, bool) {
        (self.wrapping_neg(), self == i64::MIN)
    }

    pub const fn overflowing_shl(self, rhs: u32) -> (i64, bool) {
        (self.wrapping_shl(rhs), rhs >= 64)
    }

    pub const fn overflowing_shr(self, rhs: u32) -> (i64, bool) {
        (self.wrapping_shr(rhs), rhs >= 64)
    }

    pub const fn overflowing_pow(self, exp: u32) -> (i64, bool) {
        if exp == 0 {
            return (1, false);
        }
        let mut base = self;
        let mut exp = exp;
        let mut acc: i64 = 1;
        let mut overflown = false;
        loop {
            if exp & 1 == 1 {
                let (r, o) = acc.overflowing_mul(base);
                if exp == 1 {
                    return (r, o || overflown);
                }
                acc = r;
                overflown = overflown || o;
            }
            exp /= 2;
            let (r, o) = base.overflowing_mul(base);
            base = r;
            overflown = overflown || o;
        }
    }

    pub const fn saturating_add(self, rhs: i64) -> i64 {
        match self.checked_add(rhs) {
            Some(x) => x,
            None => if rhs < 0 { i64::MIN } else { i64::MAX },
        }
    }

    pub const fn saturating_sub(self, rhs: i64) -> i64 {
        match self.checked_sub(rhs) {
            Some(x) => x,
            None => if rhs < 0 { i64::MAX } else { i64::MIN },
        }
    }

    pub const fn saturating_mul(self, rhs: i64) -> i64 {
        match self.checked_mul(rhs) {
            Some(x) => x,
            None => if (self < 0) == (rhs < 0) { i64::MAX } else { i64::MIN },
        }
    }

    pub const fn saturating_div(self, rhs: i64) -> i64 {
        if self == i64::MIN && rhs == -1 { i64::MAX } else { self / rhs }
    }

    pub const fn saturating_pow(self, exp: u32) -> i64 {
        match self.checked_pow(exp) {
            Some(x) => x,
            None => if self < 0 && exp % 2 == 1 { i64::MIN } else { i64::MAX },
        }
    }

    pub const fn div_euclid(self, rhs: i64) -> i64 {
        let q = self / rhs;
        if self % rhs < 0 {
            return if rhs > 0 { q - 1 } else { q + 1 };
        }
        q
    }

    pub const fn checked_abs(self) -> Option<i64> {
        if self == i64::MIN { None } else { Some(self.wrapping_abs()) }
    }

    pub const fn wrapping_abs(self) -> i64 {
        if self < 0 { self.wrapping_neg() } else { self }
    }

    pub const fn overflowing_abs(self) -> (i64, bool) {
        (self.wrapping_abs(), self == i64::MIN)
    }

    pub const fn saturating_abs(self) -> i64 {
        if self == i64::MIN { i64::MAX } else { self.wrapping_abs() }
    }

    pub const fn saturating_neg(self) -> i64 {
        if self == i64::MIN { i64::MAX } else { 0 - self }
    }

    pub const fn checked_isqrt(self) -> Option<i64> {
        if self < 0 { None } else { Some((self as u64).isqrt() as i64) }
    }

    #[track_caller]
    pub const fn isqrt(self) -> i64 {
        match self.checked_isqrt() {
            Some(x) => x,
            None => crate::panicking::panic("argument of integer square root cannot be negative"),
        }
    }

    pub const fn midpoint(self, rhs: i64) -> i64 {
        let t = ((self ^ rhs) >> 1) + (self & rhs);
        t + (if t < 0 { 1 } else { 0 } & (self ^ rhs))
    }

    pub const fn cast_unsigned(self) -> u64 {
        self as u64
    }

    pub const fn checked_add_unsigned(self, rhs: u64) -> Option<i64> {
        let (r, o) = self.overflowing_add_unsigned(rhs);
        if o { None } else { Some(r) }
    }

    pub const fn checked_sub_unsigned(self, rhs: u64) -> Option<i64> {
        let (r, o) = self.overflowing_sub_unsigned(rhs);
        if o { None } else { Some(r) }
    }

    pub const fn wrapping_add_unsigned(self, rhs: u64) -> i64 {
        self.wrapping_add(rhs as i64)
    }

    pub const fn wrapping_sub_unsigned(self, rhs: u64) -> i64 {
        self.wrapping_sub(rhs as i64)
    }

    pub const fn saturating_add_unsigned(self, rhs: u64) -> i64 {
        match self.checked_add_unsigned(rhs) {
            Some(x) => x,
            None => i64::MAX,
        }
    }

    pub const fn saturating_sub_unsigned(self, rhs: u64) -> i64 {
        match self.checked_sub_unsigned(rhs) {
            Some(x) => x,
            None => i64::MIN,
        }
    }

    pub const fn overflowing_add_unsigned(self, rhs: u64) -> (i64, bool) {
        let rhs = rhs as i64;
        let (res, overflowed) = self.overflowing_add(rhs);
        (res, overflowed != (rhs < 0))
    }

    pub const fn overflowing_sub_unsigned(self, rhs: u64) -> (i64, bool) {
        let rhs = rhs as i64;
        let (res, overflowed) = self.overflowing_sub(rhs);
        (res, overflowed != (rhs < 0))
    }

    pub const fn unbounded_shl(self, rhs: u32) -> i64 {
        if rhs < 64 { self << rhs } else { 0 }
    }

    pub const fn unbounded_shr(self, rhs: u32) -> i64 {
        if rhs < 64 {
            self >> rhs
        } else if self < 0 {
            -1
        } else {
            0
        }
    }

    pub const fn leading_ones(self) -> u32 {
        (!self).leading_zeros()
    }

    pub const fn trailing_ones(self) -> u32 {
        (!self).trailing_zeros()
    }

    pub const fn swap_bytes(self) -> i64 {
        let mut x = self as u64;
        let mut r: u64 = 0;
        let mut i = 0;
        while i < 8 {
            r = (r << 8) | (x & 0xff);
            x >>= 8;
            i += 1;
        }
        r as i64
    }

    pub const fn reverse_bits(self) -> i64 {
        let mut x = self as u64;
        let mut r: u64 = 0;
        let mut i = 0;
        while i < 64 {
            r = (r << 1) | (x & 1);
            x >>= 1;
            i += 1;
        }
        r as i64
    }

    pub const fn to_be(self) -> i64 {
        self.swap_bytes()
    }

    pub const fn to_le(self) -> i64 {
        self
    }

    pub const fn from_be(x: i64) -> i64 {
        x.swap_bytes()
    }

    pub const fn from_le(x: i64) -> i64 {
        x
    }

    pub const fn to_le_bytes(self) -> [u8; 8] {
        let x = self as u64;
        [x as u8, (x >> 8) as u8, (x >> 16) as u8, (x >> 24) as u8, (x >> 32) as u8, (x >> 40) as u8, (x >> 48) as u8, (x >> 56) as u8]
    }

    pub const fn to_be_bytes(self) -> [u8; 8] {
        self.swap_bytes().to_le_bytes()
    }

    pub const fn to_ne_bytes(self) -> [u8; 8] {
        self.to_le_bytes()
    }

    pub const fn from_le_bytes(bytes: [u8; 8]) -> i64 {
        (bytes[0] as u64 | (bytes[1] as u64) << 8 | (bytes[2] as u64) << 16 | (bytes[3] as u64) << 24 | (bytes[4] as u64) << 32 | (bytes[5] as u64) << 40 | (bytes[6] as u64) << 48 | (bytes[7] as u64) << 56) as i64
    }

    pub const fn from_be_bytes(bytes: [u8; 8]) -> i64 {
        i64::from_le_bytes(bytes).swap_bytes()
    }

    pub const fn from_ne_bytes(bytes: [u8; 8]) -> i64 {
        i64::from_le_bytes(bytes)
    }
}

impl i128 {
    pub const fn checked_add(self, rhs: i128) -> Option<i128> {
        let (r, o) = self.overflowing_add(rhs);
        if o { None } else { Some(r) }
    }

    pub const fn checked_sub(self, rhs: i128) -> Option<i128> {
        let (r, o) = self.overflowing_sub(rhs);
        if o { None } else { Some(r) }
    }

    pub const fn checked_mul(self, rhs: i128) -> Option<i128> {
        let (r, o) = self.overflowing_mul(rhs);
        if o { None } else { Some(r) }
    }

    pub const fn checked_div(self, rhs: i128) -> Option<i128> {
        if rhs == 0 || (self == i128::MIN && rhs == -1) {
            None
        } else {
            Some(self / rhs)
        }
    }

    pub const fn checked_rem(self, rhs: i128) -> Option<i128> {
        if rhs == 0 || (self == i128::MIN && rhs == -1) {
            None
        } else {
            Some(self % rhs)
        }
    }

    pub const fn checked_div_euclid(self, rhs: i128) -> Option<i128> {
        if rhs == 0 || (self == i128::MIN && rhs == -1) {
            None
        } else {
            Some(self.div_euclid(rhs))
        }
    }

    pub const fn checked_rem_euclid(self, rhs: i128) -> Option<i128> {
        if rhs == 0 || (self == i128::MIN && rhs == -1) {
            None
        } else {
            Some(self.rem_euclid(rhs))
        }
    }

    pub const fn checked_neg(self) -> Option<i128> {
        let (r, o) = self.overflowing_neg();
        if o { None } else { Some(r) }
    }

    pub const fn checked_shl(self, rhs: u32) -> Option<i128> {
        if rhs < 128 { Some(self << rhs) } else { None }
    }

    pub const fn checked_shr(self, rhs: u32) -> Option<i128> {
        if rhs < 128 { Some(self >> rhs) } else { None }
    }

    pub const fn checked_pow(self, exp: u32) -> Option<i128> {
        if exp == 0 {
            return Some(1);
        }
        let mut base = self;
        let mut exp = exp;
        let mut acc: i128 = 1;
        loop {
            if exp & 1 == 1 {
                acc = match acc.checked_mul(base) {
                    Some(x) => x,
                    None => return None,
                };
                if exp == 1 {
                    return Some(acc);
                }
            }
            exp /= 2;
            base = match base.checked_mul(base) {
                Some(x) => x,
                None => return None,
            };
        }
    }

    pub const fn checked_ilog2(self) -> Option<u32> {
        if self <= 0 { None } else { Some(128 - 1 - self.leading_zeros()) }
    }

    pub const fn checked_ilog10(self) -> Option<u32> {
        self.checked_ilog(10)
    }

    pub const fn checked_ilog(self, base: i128) -> Option<u32> {
        if self <= 0 || base <= 1 {
            None
        } else {
            let mut n = 0;
            let mut r = self;
            while r >= base {
                r /= base;
                n += 1;
            }
            Some(n)
        }
    }

    #[track_caller]
    pub const fn ilog2(self) -> u32 {
        match self.checked_ilog2() {
            Some(n) => n,
            None => crate::panicking::panic("argument of integer logarithm must be positive"),
        }
    }

    #[track_caller]
    pub const fn ilog10(self) -> u32 {
        match self.checked_ilog10() {
            Some(n) => n,
            None => crate::panicking::panic("argument of integer logarithm must be positive"),
        }
    }

    #[track_caller]
    pub const fn ilog(self, base: i128) -> u32 {
        if base < 2 {
            crate::panicking::panic("base of integer logarithm must be at least 2");
        }
        match self.checked_ilog(base) {
            Some(n) => n,
            None => crate::panicking::panic("argument of integer logarithm must be positive"),
        }
    }

    pub const fn wrapping_div(self, rhs: i128) -> i128 {
        self.overflowing_div(rhs).0
    }

    pub const fn wrapping_rem(self, rhs: i128) -> i128 {
        self.overflowing_rem(rhs).0
    }

    pub const fn wrapping_div_euclid(self, rhs: i128) -> i128 {
        self.overflowing_div_euclid(rhs).0
    }

    pub const fn wrapping_rem_euclid(self, rhs: i128) -> i128 {
        self.overflowing_rem_euclid(rhs).0
    }

    pub const fn wrapping_shl(self, rhs: u32) -> i128 {
        self << (rhs & 127)
    }

    pub const fn wrapping_shr(self, rhs: u32) -> i128 {
        self >> (rhs & 127)
    }

    pub const fn wrapping_pow(self, exp: u32) -> i128 {
        if exp == 0 {
            return 1;
        }
        let mut base = self;
        let mut exp = exp;
        let mut acc: i128 = 1;
        loop {
            if exp & 1 == 1 {
                acc = acc.wrapping_mul(base);
                if exp == 1 {
                    return acc;
                }
            }
            exp /= 2;
            base = base.wrapping_mul(base);
        }
    }

    pub const fn overflowing_add(self, rhs: i128) -> (i128, bool) {
        let r = self.wrapping_add(rhs);
        (r, (r < self) != (rhs < 0))
    }

    pub const fn overflowing_sub(self, rhs: i128) -> (i128, bool) {
        let r = self.wrapping_sub(rhs);
        (r, (r > self) != (rhs < 0))
    }

    pub const fn overflowing_mul(self, rhs: i128) -> (i128, bool) {
        let r = self.wrapping_mul(rhs);

        let o = if self == -1 {
            rhs == i128::MIN
        } else if rhs == -1 {
            self == i128::MIN
        } else {
            self != 0 && r / self != rhs
        };
        (r, o)
    }

    pub const fn overflowing_div(self, rhs: i128) -> (i128, bool) {
        if self == i128::MIN && rhs == -1 { (self, true) } else { (self / rhs, false) }
    }

    pub const fn overflowing_rem(self, rhs: i128) -> (i128, bool) {
        if rhs == -1 { (0, self == i128::MIN) } else { (self % rhs, false) }
    }

    pub const fn overflowing_div_euclid(self, rhs: i128) -> (i128, bool) {
        if self == i128::MIN && rhs == -1 { (self, true) } else { (self.div_euclid(rhs), false) }
    }

    pub const fn overflowing_rem_euclid(self, rhs: i128) -> (i128, bool) {
        if rhs == -1 { (0, self == i128::MIN) } else { (self.rem_euclid(rhs), false) }
    }

    pub const fn overflowing_neg(self) -> (i128, bool) {
        (self.wrapping_neg(), self == i128::MIN)
    }

    pub const fn overflowing_shl(self, rhs: u32) -> (i128, bool) {
        (self.wrapping_shl(rhs), rhs >= 128)
    }

    pub const fn overflowing_shr(self, rhs: u32) -> (i128, bool) {
        (self.wrapping_shr(rhs), rhs >= 128)
    }

    pub const fn overflowing_pow(self, exp: u32) -> (i128, bool) {
        if exp == 0 {
            return (1, false);
        }
        let mut base = self;
        let mut exp = exp;
        let mut acc: i128 = 1;
        let mut overflown = false;
        loop {
            if exp & 1 == 1 {
                let (r, o) = acc.overflowing_mul(base);
                if exp == 1 {
                    return (r, o || overflown);
                }
                acc = r;
                overflown = overflown || o;
            }
            exp /= 2;
            let (r, o) = base.overflowing_mul(base);
            base = r;
            overflown = overflown || o;
        }
    }

    pub const fn saturating_add(self, rhs: i128) -> i128 {
        match self.checked_add(rhs) {
            Some(x) => x,
            None => if rhs < 0 { i128::MIN } else { i128::MAX },
        }
    }

    pub const fn saturating_sub(self, rhs: i128) -> i128 {
        match self.checked_sub(rhs) {
            Some(x) => x,
            None => if rhs < 0 { i128::MAX } else { i128::MIN },
        }
    }

    pub const fn saturating_mul(self, rhs: i128) -> i128 {
        match self.checked_mul(rhs) {
            Some(x) => x,
            None => if (self < 0) == (rhs < 0) { i128::MAX } else { i128::MIN },
        }
    }

    pub const fn saturating_div(self, rhs: i128) -> i128 {
        if self == i128::MIN && rhs == -1 { i128::MAX } else { self / rhs }
    }

    pub const fn saturating_pow(self, exp: u32) -> i128 {
        match self.checked_pow(exp) {
            Some(x) => x,
            None => if self < 0 && exp % 2 == 1 { i128::MIN } else { i128::MAX },
        }
    }

    pub const fn div_euclid(self, rhs: i128) -> i128 {
        let q = self / rhs;
        if self % rhs < 0 {
            return if rhs > 0 { q - 1 } else { q + 1 };
        }
        q
    }

    pub const fn checked_abs(self) -> Option<i128> {
        if self == i128::MIN { None } else { Some(self.wrapping_abs()) }
    }

    pub const fn wrapping_abs(self) -> i128 {
        if self < 0 { self.wrapping_neg() } else { self }
    }

    pub const fn overflowing_abs(self) -> (i128, bool) {
        (self.wrapping_abs(), self == i128::MIN)
    }

    pub const fn saturating_abs(self) -> i128 {
        if self == i128::MIN { i128::MAX } else { self.wrapping_abs() }
    }

    pub const fn saturating_neg(self) -> i128 {
        if self == i128::MIN { i128::MAX } else { 0 - self }
    }

    pub const fn checked_isqrt(self) -> Option<i128> {
        if self < 0 { None } else { Some((self as u128).isqrt() as i128) }
    }

    #[track_caller]
    pub const fn isqrt(self) -> i128 {
        match self.checked_isqrt() {
            Some(x) => x,
            None => crate::panicking::panic("argument of integer square root cannot be negative"),
        }
    }

    pub const fn midpoint(self, rhs: i128) -> i128 {
        let t = ((self ^ rhs) >> 1) + (self & rhs);
        t + (if t < 0 { 1 } else { 0 } & (self ^ rhs))
    }

    pub const fn cast_unsigned(self) -> u128 {
        self as u128
    }

    pub const fn checked_add_unsigned(self, rhs: u128) -> Option<i128> {
        let (r, o) = self.overflowing_add_unsigned(rhs);
        if o { None } else { Some(r) }
    }

    pub const fn checked_sub_unsigned(self, rhs: u128) -> Option<i128> {
        let (r, o) = self.overflowing_sub_unsigned(rhs);
        if o { None } else { Some(r) }
    }

    pub const fn wrapping_add_unsigned(self, rhs: u128) -> i128 {
        self.wrapping_add(rhs as i128)
    }

    pub const fn wrapping_sub_unsigned(self, rhs: u128) -> i128 {
        self.wrapping_sub(rhs as i128)
    }

    pub const fn saturating_add_unsigned(self, rhs: u128) -> i128 {
        match self.checked_add_unsigned(rhs) {
            Some(x) => x,
            None => i128::MAX,
        }
    }

    pub const fn saturating_sub_unsigned(self, rhs: u128) -> i128 {
        match self.checked_sub_unsigned(rhs) {
            Some(x) => x,
            None => i128::MIN,
        }
    }

    pub const fn overflowing_add_unsigned(self, rhs: u128) -> (i128, bool) {
        let rhs = rhs as i128;
        let (res, overflowed) = self.overflowing_add(rhs);
        (res, overflowed != (rhs < 0))
    }

    pub const fn overflowing_sub_unsigned(self, rhs: u128) -> (i128, bool) {
        let rhs = rhs as i128;
        let (res, overflowed) = self.overflowing_sub(rhs);
        (res, overflowed != (rhs < 0))
    }

    pub const fn unbounded_shl(self, rhs: u32) -> i128 {
        if rhs < 128 { self << rhs } else { 0 }
    }

    pub const fn unbounded_shr(self, rhs: u32) -> i128 {
        if rhs < 128 {
            self >> rhs
        } else if self < 0 {
            -1
        } else {
            0
        }
    }

    pub const fn leading_ones(self) -> u32 {
        (!self).leading_zeros()
    }

    pub const fn trailing_ones(self) -> u32 {
        (!self).trailing_zeros()
    }

    pub const fn swap_bytes(self) -> i128 {
        let mut x = self as u128;
        let mut r: u128 = 0;
        let mut i = 0;
        while i < 16 {
            r = (r << 8) | (x & 0xff);
            x >>= 8;
            i += 1;
        }
        r as i128
    }

    pub const fn reverse_bits(self) -> i128 {
        let mut x = self as u128;
        let mut r: u128 = 0;
        let mut i = 0;
        while i < 128 {
            r = (r << 1) | (x & 1);
            x >>= 1;
            i += 1;
        }
        r as i128
    }

    pub const fn to_be(self) -> i128 {
        self.swap_bytes()
    }

    pub const fn to_le(self) -> i128 {
        self
    }

    pub const fn from_be(x: i128) -> i128 {
        x.swap_bytes()
    }

    pub const fn from_le(x: i128) -> i128 {
        x
    }

    pub const fn to_le_bytes(self) -> [u8; 16] {
        let x = self as u128;
        [x as u8, (x >> 8) as u8, (x >> 16) as u8, (x >> 24) as u8, (x >> 32) as u8, (x >> 40) as u8, (x >> 48) as u8, (x >> 56) as u8, (x >> 64) as u8, (x >> 72) as u8, (x >> 80) as u8, (x >> 88) as u8, (x >> 96) as u8, (x >> 104) as u8, (x >> 112) as u8, (x >> 120) as u8]
    }

    pub const fn to_be_bytes(self) -> [u8; 16] {
        self.swap_bytes().to_le_bytes()
    }

    pub const fn to_ne_bytes(self) -> [u8; 16] {
        self.to_le_bytes()
    }

    pub const fn from_le_bytes(bytes: [u8; 16]) -> i128 {
        (bytes[0] as u128 | (bytes[1] as u128) << 8 | (bytes[2] as u128) << 16 | (bytes[3] as u128) << 24 | (bytes[4] as u128) << 32 | (bytes[5] as u128) << 40 | (bytes[6] as u128) << 48 | (bytes[7] as u128) << 56 | (bytes[8] as u128) << 64 | (bytes[9] as u128) << 72 | (bytes[10] as u128) << 80 | (bytes[11] as u128) << 88 | (bytes[12] as u128) << 96 | (bytes[13] as u128) << 104 | (bytes[14] as u128) << 112 | (bytes[15] as u128) << 120) as i128
    }

    pub const fn from_be_bytes(bytes: [u8; 16]) -> i128 {
        i128::from_le_bytes(bytes).swap_bytes()
    }

    pub const fn from_ne_bytes(bytes: [u8; 16]) -> i128 {
        i128::from_le_bytes(bytes)
    }
}

impl isize {
    pub const fn checked_add(self, rhs: isize) -> Option<isize> {
        let (r, o) = self.overflowing_add(rhs);
        if o { None } else { Some(r) }
    }

    pub const fn checked_sub(self, rhs: isize) -> Option<isize> {
        let (r, o) = self.overflowing_sub(rhs);
        if o { None } else { Some(r) }
    }

    pub const fn checked_mul(self, rhs: isize) -> Option<isize> {
        let (r, o) = self.overflowing_mul(rhs);
        if o { None } else { Some(r) }
    }

    pub const fn checked_div(self, rhs: isize) -> Option<isize> {
        if rhs == 0 || (self == isize::MIN && rhs == -1) {
            None
        } else {
            Some(self / rhs)
        }
    }

    pub const fn checked_rem(self, rhs: isize) -> Option<isize> {
        if rhs == 0 || (self == isize::MIN && rhs == -1) {
            None
        } else {
            Some(self % rhs)
        }
    }

    pub const fn checked_div_euclid(self, rhs: isize) -> Option<isize> {
        if rhs == 0 || (self == isize::MIN && rhs == -1) {
            None
        } else {
            Some(self.div_euclid(rhs))
        }
    }

    pub const fn checked_rem_euclid(self, rhs: isize) -> Option<isize> {
        if rhs == 0 || (self == isize::MIN && rhs == -1) {
            None
        } else {
            Some(self.rem_euclid(rhs))
        }
    }

    pub const fn checked_neg(self) -> Option<isize> {
        let (r, o) = self.overflowing_neg();
        if o { None } else { Some(r) }
    }

    pub const fn checked_shl(self, rhs: u32) -> Option<isize> {
        if rhs < 64 { Some(self << rhs) } else { None }
    }

    pub const fn checked_shr(self, rhs: u32) -> Option<isize> {
        if rhs < 64 { Some(self >> rhs) } else { None }
    }

    pub const fn checked_pow(self, exp: u32) -> Option<isize> {
        if exp == 0 {
            return Some(1);
        }
        let mut base = self;
        let mut exp = exp;
        let mut acc: isize = 1;
        loop {
            if exp & 1 == 1 {
                acc = match acc.checked_mul(base) {
                    Some(x) => x,
                    None => return None,
                };
                if exp == 1 {
                    return Some(acc);
                }
            }
            exp /= 2;
            base = match base.checked_mul(base) {
                Some(x) => x,
                None => return None,
            };
        }
    }

    pub const fn checked_ilog2(self) -> Option<u32> {
        if self <= 0 { None } else { Some(64 - 1 - self.leading_zeros()) }
    }

    pub const fn checked_ilog10(self) -> Option<u32> {
        self.checked_ilog(10)
    }

    pub const fn checked_ilog(self, base: isize) -> Option<u32> {
        if self <= 0 || base <= 1 {
            None
        } else {
            let mut n = 0;
            let mut r = self;
            while r >= base {
                r /= base;
                n += 1;
            }
            Some(n)
        }
    }

    #[track_caller]
    pub const fn ilog2(self) -> u32 {
        match self.checked_ilog2() {
            Some(n) => n,
            None => crate::panicking::panic("argument of integer logarithm must be positive"),
        }
    }

    #[track_caller]
    pub const fn ilog10(self) -> u32 {
        match self.checked_ilog10() {
            Some(n) => n,
            None => crate::panicking::panic("argument of integer logarithm must be positive"),
        }
    }

    #[track_caller]
    pub const fn ilog(self, base: isize) -> u32 {
        if base < 2 {
            crate::panicking::panic("base of integer logarithm must be at least 2");
        }
        match self.checked_ilog(base) {
            Some(n) => n,
            None => crate::panicking::panic("argument of integer logarithm must be positive"),
        }
    }

    pub const fn wrapping_div(self, rhs: isize) -> isize {
        self.overflowing_div(rhs).0
    }

    pub const fn wrapping_rem(self, rhs: isize) -> isize {
        self.overflowing_rem(rhs).0
    }

    pub const fn wrapping_div_euclid(self, rhs: isize) -> isize {
        self.overflowing_div_euclid(rhs).0
    }

    pub const fn wrapping_rem_euclid(self, rhs: isize) -> isize {
        self.overflowing_rem_euclid(rhs).0
    }

    pub const fn wrapping_shl(self, rhs: u32) -> isize {
        self << (rhs & 63)
    }

    pub const fn wrapping_shr(self, rhs: u32) -> isize {
        self >> (rhs & 63)
    }

    pub const fn wrapping_pow(self, exp: u32) -> isize {
        if exp == 0 {
            return 1;
        }
        let mut base = self;
        let mut exp = exp;
        let mut acc: isize = 1;
        loop {
            if exp & 1 == 1 {
                acc = acc.wrapping_mul(base);
                if exp == 1 {
                    return acc;
                }
            }
            exp /= 2;
            base = base.wrapping_mul(base);
        }
    }

    pub const fn overflowing_add(self, rhs: isize) -> (isize, bool) {
        let r = self.wrapping_add(rhs);
        (r, (r < self) != (rhs < 0))
    }

    pub const fn overflowing_sub(self, rhs: isize) -> (isize, bool) {
        let r = self.wrapping_sub(rhs);
        (r, (r > self) != (rhs < 0))
    }

    pub const fn overflowing_mul(self, rhs: isize) -> (isize, bool) {
        let r = self.wrapping_mul(rhs);

        let o = if self == -1 {
            rhs == isize::MIN
        } else if rhs == -1 {
            self == isize::MIN
        } else {
            self != 0 && r / self != rhs
        };
        (r, o)
    }

    pub const fn overflowing_div(self, rhs: isize) -> (isize, bool) {
        if self == isize::MIN && rhs == -1 { (self, true) } else { (self / rhs, false) }
    }

    pub const fn overflowing_rem(self, rhs: isize) -> (isize, bool) {
        if rhs == -1 { (0, self == isize::MIN) } else { (self % rhs, false) }
    }

    pub const fn overflowing_div_euclid(self, rhs: isize) -> (isize, bool) {
        if self == isize::MIN && rhs == -1 { (self, true) } else { (self.div_euclid(rhs), false) }
    }

    pub const fn overflowing_rem_euclid(self, rhs: isize) -> (isize, bool) {
        if rhs == -1 { (0, self == isize::MIN) } else { (self.rem_euclid(rhs), false) }
    }

    pub const fn overflowing_neg(self) -> (isize, bool) {
        (self.wrapping_neg(), self == isize::MIN)
    }

    pub const fn overflowing_shl(self, rhs: u32) -> (isize, bool) {
        (self.wrapping_shl(rhs), rhs >= 64)
    }

    pub const fn overflowing_shr(self, rhs: u32) -> (isize, bool) {
        (self.wrapping_shr(rhs), rhs >= 64)
    }

    pub const fn overflowing_pow(self, exp: u32) -> (isize, bool) {
        if exp == 0 {
            return (1, false);
        }
        let mut base = self;
        let mut exp = exp;
        let mut acc: isize = 1;
        let mut overflown = false;
        loop {
            if exp & 1 == 1 {
                let (r, o) = acc.overflowing_mul(base);
                if exp == 1 {
                    return (r, o || overflown);
                }
                acc = r;
                overflown = overflown || o;
            }
            exp /= 2;
            let (r, o) = base.overflowing_mul(base);
            base = r;
            overflown = overflown || o;
        }
    }

    pub const fn saturating_add(self, rhs: isize) -> isize {
        match self.checked_add(rhs) {
            Some(x) => x,
            None => if rhs < 0 { isize::MIN } else { isize::MAX },
        }
    }

    pub const fn saturating_sub(self, rhs: isize) -> isize {
        match self.checked_sub(rhs) {
            Some(x) => x,
            None => if rhs < 0 { isize::MAX } else { isize::MIN },
        }
    }

    pub const fn saturating_mul(self, rhs: isize) -> isize {
        match self.checked_mul(rhs) {
            Some(x) => x,
            None => if (self < 0) == (rhs < 0) { isize::MAX } else { isize::MIN },
        }
    }

    pub const fn saturating_div(self, rhs: isize) -> isize {
        if self == isize::MIN && rhs == -1 { isize::MAX } else { self / rhs }
    }

    pub const fn saturating_pow(self, exp: u32) -> isize {
        match self.checked_pow(exp) {
            Some(x) => x,
            None => if self < 0 && exp % 2 == 1 { isize::MIN } else { isize::MAX },
        }
    }

    pub const fn div_euclid(self, rhs: isize) -> isize {
        let q = self / rhs;
        if self % rhs < 0 {
            return if rhs > 0 { q - 1 } else { q + 1 };
        }
        q
    }

    pub const fn checked_abs(self) -> Option<isize> {
        if self == isize::MIN { None } else { Some(self.wrapping_abs()) }
    }

    pub const fn wrapping_abs(self) -> isize {
        if self < 0 { self.wrapping_neg() } else { self }
    }

    pub const fn overflowing_abs(self) -> (isize, bool) {
        (self.wrapping_abs(), self == isize::MIN)
    }

    pub const fn saturating_abs(self) -> isize {
        if self == isize::MIN { isize::MAX } else { self.wrapping_abs() }
    }

    pub const fn saturating_neg(self) -> isize {
        if self == isize::MIN { isize::MAX } else { 0 - self }
    }

    pub const fn checked_isqrt(self) -> Option<isize> {
        if self < 0 { None } else { Some((self as usize).isqrt() as isize) }
    }

    #[track_caller]
    pub const fn isqrt(self) -> isize {
        match self.checked_isqrt() {
            Some(x) => x,
            None => crate::panicking::panic("argument of integer square root cannot be negative"),
        }
    }

    pub const fn midpoint(self, rhs: isize) -> isize {
        let t = ((self ^ rhs) >> 1) + (self & rhs);
        t + (if t < 0 { 1 } else { 0 } & (self ^ rhs))
    }

    pub const fn cast_unsigned(self) -> usize {
        self as usize
    }

    pub const fn checked_add_unsigned(self, rhs: usize) -> Option<isize> {
        let (r, o) = self.overflowing_add_unsigned(rhs);
        if o { None } else { Some(r) }
    }

    pub const fn checked_sub_unsigned(self, rhs: usize) -> Option<isize> {
        let (r, o) = self.overflowing_sub_unsigned(rhs);
        if o { None } else { Some(r) }
    }

    pub const fn wrapping_add_unsigned(self, rhs: usize) -> isize {
        self.wrapping_add(rhs as isize)
    }

    pub const fn wrapping_sub_unsigned(self, rhs: usize) -> isize {
        self.wrapping_sub(rhs as isize)
    }

    pub const fn saturating_add_unsigned(self, rhs: usize) -> isize {
        match self.checked_add_unsigned(rhs) {
            Some(x) => x,
            None => isize::MAX,
        }
    }

    pub const fn saturating_sub_unsigned(self, rhs: usize) -> isize {
        match self.checked_sub_unsigned(rhs) {
            Some(x) => x,
            None => isize::MIN,
        }
    }

    pub const fn overflowing_add_unsigned(self, rhs: usize) -> (isize, bool) {
        let rhs = rhs as isize;
        let (res, overflowed) = self.overflowing_add(rhs);
        (res, overflowed != (rhs < 0))
    }

    pub const fn overflowing_sub_unsigned(self, rhs: usize) -> (isize, bool) {
        let rhs = rhs as isize;
        let (res, overflowed) = self.overflowing_sub(rhs);
        (res, overflowed != (rhs < 0))
    }

    pub const fn unbounded_shl(self, rhs: u32) -> isize {
        if rhs < 64 { self << rhs } else { 0 }
    }

    pub const fn unbounded_shr(self, rhs: u32) -> isize {
        if rhs < 64 {
            self >> rhs
        } else if self < 0 {
            -1
        } else {
            0
        }
    }

    pub const fn leading_ones(self) -> u32 {
        (!self).leading_zeros()
    }

    pub const fn trailing_ones(self) -> u32 {
        (!self).trailing_zeros()
    }

    pub const fn swap_bytes(self) -> isize {
        let mut x = self as usize;
        let mut r: usize = 0;
        let mut i = 0;
        while i < 8 {
            r = (r << 8) | (x & 0xff);
            x >>= 8;
            i += 1;
        }
        r as isize
    }

    pub const fn reverse_bits(self) -> isize {
        let mut x = self as usize;
        let mut r: usize = 0;
        let mut i = 0;
        while i < 64 {
            r = (r << 1) | (x & 1);
            x >>= 1;
            i += 1;
        }
        r as isize
    }

    pub const fn to_be(self) -> isize {
        self.swap_bytes()
    }

    pub const fn to_le(self) -> isize {
        self
    }

    pub const fn from_be(x: isize) -> isize {
        x.swap_bytes()
    }

    pub const fn from_le(x: isize) -> isize {
        x
    }

    pub const fn to_le_bytes(self) -> [u8; 8] {
        let x = self as usize;
        [x as u8, (x >> 8) as u8, (x >> 16) as u8, (x >> 24) as u8, (x >> 32) as u8, (x >> 40) as u8, (x >> 48) as u8, (x >> 56) as u8]
    }

    pub const fn to_be_bytes(self) -> [u8; 8] {
        self.swap_bytes().to_le_bytes()
    }

    pub const fn to_ne_bytes(self) -> [u8; 8] {
        self.to_le_bytes()
    }

    pub const fn from_le_bytes(bytes: [u8; 8]) -> isize {
        (bytes[0] as usize | (bytes[1] as usize) << 8 | (bytes[2] as usize) << 16 | (bytes[3] as usize) << 24 | (bytes[4] as usize) << 32 | (bytes[5] as usize) << 40 | (bytes[6] as usize) << 48 | (bytes[7] as usize) << 56) as isize
    }

    pub const fn from_be_bytes(bytes: [u8; 8]) -> isize {
        isize::from_le_bytes(bytes).swap_bytes()
    }

    pub const fn from_ne_bytes(bytes: [u8; 8]) -> isize {
        isize::from_le_bytes(bytes)
    }
}

impl u8 {
    pub const fn checked_add(self, rhs: u8) -> Option<u8> {
        let (r, o) = self.overflowing_add(rhs);
        if o { None } else { Some(r) }
    }

    pub const fn checked_sub(self, rhs: u8) -> Option<u8> {
        let (r, o) = self.overflowing_sub(rhs);
        if o { None } else { Some(r) }
    }

    pub const fn checked_mul(self, rhs: u8) -> Option<u8> {
        let (r, o) = self.overflowing_mul(rhs);
        if o { None } else { Some(r) }
    }

    pub const fn checked_div(self, rhs: u8) -> Option<u8> {
        if rhs == 0 {
            None
        } else {
            Some(self / rhs)
        }
    }

    pub const fn checked_rem(self, rhs: u8) -> Option<u8> {
        if rhs == 0 {
            None
        } else {
            Some(self % rhs)
        }
    }

    pub const fn checked_div_euclid(self, rhs: u8) -> Option<u8> {
        if rhs == 0 {
            None
        } else {
            Some(self.div_euclid(rhs))
        }
    }

    pub const fn checked_rem_euclid(self, rhs: u8) -> Option<u8> {
        if rhs == 0 {
            None
        } else {
            Some(self.rem_euclid(rhs))
        }
    }

    pub const fn checked_neg(self) -> Option<u8> {
        let (r, o) = self.overflowing_neg();
        if o { None } else { Some(r) }
    }

    pub const fn checked_shl(self, rhs: u32) -> Option<u8> {
        if rhs < 8 { Some(self << rhs) } else { None }
    }

    pub const fn checked_shr(self, rhs: u32) -> Option<u8> {
        if rhs < 8 { Some(self >> rhs) } else { None }
    }

    pub const fn checked_pow(self, exp: u32) -> Option<u8> {
        if exp == 0 {
            return Some(1);
        }
        let mut base = self;
        let mut exp = exp;
        let mut acc: u8 = 1;
        loop {
            if exp & 1 == 1 {
                acc = match acc.checked_mul(base) {
                    Some(x) => x,
                    None => return None,
                };
                if exp == 1 {
                    return Some(acc);
                }
            }
            exp /= 2;
            base = match base.checked_mul(base) {
                Some(x) => x,
                None => return None,
            };
        }
    }

    pub const fn checked_ilog2(self) -> Option<u32> {
        if self <= 0 { None } else { Some(8 - 1 - self.leading_zeros()) }
    }

    pub const fn checked_ilog10(self) -> Option<u32> {
        self.checked_ilog(10)
    }

    pub const fn checked_ilog(self, base: u8) -> Option<u32> {
        if self <= 0 || base <= 1 {
            None
        } else {
            let mut n = 0;
            let mut r = self;
            while r >= base {
                r /= base;
                n += 1;
            }
            Some(n)
        }
    }

    #[track_caller]
    pub const fn ilog2(self) -> u32 {
        match self.checked_ilog2() {
            Some(n) => n,
            None => crate::panicking::panic("argument of integer logarithm must be positive"),
        }
    }

    #[track_caller]
    pub const fn ilog10(self) -> u32 {
        match self.checked_ilog10() {
            Some(n) => n,
            None => crate::panicking::panic("argument of integer logarithm must be positive"),
        }
    }

    #[track_caller]
    pub const fn ilog(self, base: u8) -> u32 {
        if base < 2 {
            crate::panicking::panic("base of integer logarithm must be at least 2");
        }
        match self.checked_ilog(base) {
            Some(n) => n,
            None => crate::panicking::panic("argument of integer logarithm must be positive"),
        }
    }

    pub const fn wrapping_div(self, rhs: u8) -> u8 {
        self.overflowing_div(rhs).0
    }

    pub const fn wrapping_rem(self, rhs: u8) -> u8 {
        self.overflowing_rem(rhs).0
    }

    pub const fn wrapping_div_euclid(self, rhs: u8) -> u8 {
        self.overflowing_div_euclid(rhs).0
    }

    pub const fn wrapping_rem_euclid(self, rhs: u8) -> u8 {
        self.overflowing_rem_euclid(rhs).0
    }

    pub const fn wrapping_shl(self, rhs: u32) -> u8 {
        self << (rhs & 7)
    }

    pub const fn wrapping_shr(self, rhs: u32) -> u8 {
        self >> (rhs & 7)
    }

    pub const fn wrapping_pow(self, exp: u32) -> u8 {
        if exp == 0 {
            return 1;
        }
        let mut base = self;
        let mut exp = exp;
        let mut acc: u8 = 1;
        loop {
            if exp & 1 == 1 {
                acc = acc.wrapping_mul(base);
                if exp == 1 {
                    return acc;
                }
            }
            exp /= 2;
            base = base.wrapping_mul(base);
        }
    }

    pub const fn overflowing_add(self, rhs: u8) -> (u8, bool) {
        let r = self.wrapping_add(rhs);
        (r, r < self)
    }

    pub const fn overflowing_sub(self, rhs: u8) -> (u8, bool) {
        let r = self.wrapping_sub(rhs);
        (r, r > self)
    }

    pub const fn overflowing_mul(self, rhs: u8) -> (u8, bool) {
        let r = self.wrapping_mul(rhs);

        (r, self != 0 && r / self != rhs)
    }

    pub const fn overflowing_div(self, rhs: u8) -> (u8, bool) {
        (self / rhs, false)
    }

    pub const fn overflowing_rem(self, rhs: u8) -> (u8, bool) {
        (self % rhs, false)
    }

    pub const fn overflowing_div_euclid(self, rhs: u8) -> (u8, bool) {
        (self / rhs, false)
    }

    pub const fn overflowing_rem_euclid(self, rhs: u8) -> (u8, bool) {
        (self % rhs, false)
    }

    pub const fn overflowing_neg(self) -> (u8, bool) {
        (self.wrapping_neg(), self != 0)
    }

    pub const fn overflowing_shl(self, rhs: u32) -> (u8, bool) {
        (self.wrapping_shl(rhs), rhs >= 8)
    }

    pub const fn overflowing_shr(self, rhs: u32) -> (u8, bool) {
        (self.wrapping_shr(rhs), rhs >= 8)
    }

    pub const fn overflowing_pow(self, exp: u32) -> (u8, bool) {
        if exp == 0 {
            return (1, false);
        }
        let mut base = self;
        let mut exp = exp;
        let mut acc: u8 = 1;
        let mut overflown = false;
        loop {
            if exp & 1 == 1 {
                let (r, o) = acc.overflowing_mul(base);
                if exp == 1 {
                    return (r, o || overflown);
                }
                acc = r;
                overflown = overflown || o;
            }
            exp /= 2;
            let (r, o) = base.overflowing_mul(base);
            base = r;
            overflown = overflown || o;
        }
    }

    pub const fn saturating_add(self, rhs: u8) -> u8 {
        match self.checked_add(rhs) {
            Some(x) => x,
            None => u8::MAX,
        }
    }

    pub const fn saturating_sub(self, rhs: u8) -> u8 {
        match self.checked_sub(rhs) {
            Some(x) => x,
            None => u8::MIN,
        }
    }

    pub const fn saturating_mul(self, rhs: u8) -> u8 {
        match self.checked_mul(rhs) {
            Some(x) => x,
            None => u8::MAX,
        }
    }

    pub const fn saturating_div(self, rhs: u8) -> u8 {
        self / rhs
    }

    pub const fn saturating_pow(self, exp: u32) -> u8 {
        match self.checked_pow(exp) {
            Some(x) => x,
            None => u8::MAX,
        }
    }

    pub const fn div_euclid(self, rhs: u8) -> u8 {
        self / rhs
    }

    pub const fn rem_euclid(self, rhs: u8) -> u8 {
        self % rhs
    }

    pub const fn div_ceil(self, rhs: u8) -> u8 {
        let d = self / rhs;
        let r = self % rhs;
        if r > 0 { d + 1 } else { d }
    }

    #[rustc_inherit_overflow_checks]
    pub const fn next_multiple_of(self, rhs: u8) -> u8 {
        match self % rhs {
            0 => self,
            r => self + (rhs - r),
        }
    }

    pub const fn checked_next_multiple_of(self, rhs: u8) -> Option<u8> {
        match self.checked_rem(rhs) {
            None => None,
            Some(0) => Some(self),
            Some(r) => self.checked_add(rhs - r),
        }
    }

    const fn one_less_than_next_power_of_two(self) -> u8 {
        if self <= 1 {
            return 0;
        }
        let p = self - 1;
        u8::MAX >> p.leading_zeros()
    }

    #[rustc_inherit_overflow_checks]
    pub const fn next_power_of_two(self) -> u8 {
        self.one_less_than_next_power_of_two() + 1
    }

    pub const fn checked_next_power_of_two(self) -> Option<u8> {
        self.one_less_than_next_power_of_two().checked_add(1)
    }

    pub const fn isqrt(self) -> u8 {
        if self < 2 {
            return self;
        }
        let mut op = self;
        let mut res: u8 = 0;
        let mut one: u8 = 1 << 6;
        while one > op {
            one >>= 2;
        }
        while one != 0 {
            if op >= res + one {
                op -= res + one;
                res = (res >> 1) + one;
            } else {
                res >>= 1;
            }
            one >>= 2;
        }
        res
    }

    pub const fn midpoint(self, rhs: u8) -> u8 {
        (self & rhs) + ((self ^ rhs) >> 1)
    }

    pub const fn cast_signed(self) -> i8 {
        self as i8
    }

    pub const fn checked_add_signed(self, rhs: i8) -> Option<u8> {
        let (r, o) = self.overflowing_add_signed(rhs);
        if o { None } else { Some(r) }
    }

    pub const fn wrapping_add_signed(self, rhs: i8) -> u8 {
        self.wrapping_add(rhs as u8)
    }

    pub const fn saturating_add_signed(self, rhs: i8) -> u8 {
        let (res, overflow) = self.overflowing_add(rhs as u8);
        if overflow == (rhs < 0) {
            res
        } else if overflow {
            u8::MAX
        } else {
            0
        }
    }

    pub const fn overflowing_add_signed(self, rhs: i8) -> (u8, bool) {
        let (res, overflowed) = self.overflowing_add(rhs as u8);
        (res, overflowed != (rhs < 0))
    }

    pub const fn unbounded_shl(self, rhs: u32) -> u8 {
        if rhs < 8 { self << rhs } else { 0 }
    }

    pub const fn unbounded_shr(self, rhs: u32) -> u8 {
        if rhs < 8 { self >> rhs } else { 0 }
    }

    pub const fn leading_ones(self) -> u32 {
        (!self).leading_zeros()
    }

    pub const fn trailing_ones(self) -> u32 {
        (!self).trailing_zeros()
    }

    pub const fn swap_bytes(self) -> u8 {
        let mut x = self as u8;
        let mut r: u8 = 0;
        let mut i = 0;
        while i < 1 {
            r = (r << 8) | (x & 0xff);
            x >>= 8;
            i += 1;
        }
        r as u8
    }

    pub const fn reverse_bits(self) -> u8 {
        let mut x = self as u8;
        let mut r: u8 = 0;
        let mut i = 0;
        while i < 8 {
            r = (r << 1) | (x & 1);
            x >>= 1;
            i += 1;
        }
        r as u8
    }

    pub const fn to_be(self) -> u8 {
        self.swap_bytes()
    }

    pub const fn to_le(self) -> u8 {
        self
    }

    pub const fn from_be(x: u8) -> u8 {
        x.swap_bytes()
    }

    pub const fn from_le(x: u8) -> u8 {
        x
    }

    pub const fn to_le_bytes(self) -> [u8; 1] {
        let x = self as u8;
        [x as u8]
    }

    pub const fn to_be_bytes(self) -> [u8; 1] {
        self.swap_bytes().to_le_bytes()
    }

    pub const fn to_ne_bytes(self) -> [u8; 1] {
        self.to_le_bytes()
    }

    pub const fn from_le_bytes(bytes: [u8; 1]) -> u8 {
        (bytes[0] as u8) as u8
    }

    pub const fn from_be_bytes(bytes: [u8; 1]) -> u8 {
        u8::from_le_bytes(bytes).swap_bytes()
    }

    pub const fn from_ne_bytes(bytes: [u8; 1]) -> u8 {
        u8::from_le_bytes(bytes)
    }
}

impl u16 {
    pub const fn checked_add(self, rhs: u16) -> Option<u16> {
        let (r, o) = self.overflowing_add(rhs);
        if o { None } else { Some(r) }
    }

    pub const fn checked_sub(self, rhs: u16) -> Option<u16> {
        let (r, o) = self.overflowing_sub(rhs);
        if o { None } else { Some(r) }
    }

    pub const fn checked_mul(self, rhs: u16) -> Option<u16> {
        let (r, o) = self.overflowing_mul(rhs);
        if o { None } else { Some(r) }
    }

    pub const fn checked_div(self, rhs: u16) -> Option<u16> {
        if rhs == 0 {
            None
        } else {
            Some(self / rhs)
        }
    }

    pub const fn checked_rem(self, rhs: u16) -> Option<u16> {
        if rhs == 0 {
            None
        } else {
            Some(self % rhs)
        }
    }

    pub const fn checked_div_euclid(self, rhs: u16) -> Option<u16> {
        if rhs == 0 {
            None
        } else {
            Some(self.div_euclid(rhs))
        }
    }

    pub const fn checked_rem_euclid(self, rhs: u16) -> Option<u16> {
        if rhs == 0 {
            None
        } else {
            Some(self.rem_euclid(rhs))
        }
    }

    pub const fn checked_neg(self) -> Option<u16> {
        let (r, o) = self.overflowing_neg();
        if o { None } else { Some(r) }
    }

    pub const fn checked_shl(self, rhs: u32) -> Option<u16> {
        if rhs < 16 { Some(self << rhs) } else { None }
    }

    pub const fn checked_shr(self, rhs: u32) -> Option<u16> {
        if rhs < 16 { Some(self >> rhs) } else { None }
    }

    pub const fn checked_pow(self, exp: u32) -> Option<u16> {
        if exp == 0 {
            return Some(1);
        }
        let mut base = self;
        let mut exp = exp;
        let mut acc: u16 = 1;
        loop {
            if exp & 1 == 1 {
                acc = match acc.checked_mul(base) {
                    Some(x) => x,
                    None => return None,
                };
                if exp == 1 {
                    return Some(acc);
                }
            }
            exp /= 2;
            base = match base.checked_mul(base) {
                Some(x) => x,
                None => return None,
            };
        }
    }

    pub const fn checked_ilog2(self) -> Option<u32> {
        if self <= 0 { None } else { Some(16 - 1 - self.leading_zeros()) }
    }

    pub const fn checked_ilog10(self) -> Option<u32> {
        self.checked_ilog(10)
    }

    pub const fn checked_ilog(self, base: u16) -> Option<u32> {
        if self <= 0 || base <= 1 {
            None
        } else {
            let mut n = 0;
            let mut r = self;
            while r >= base {
                r /= base;
                n += 1;
            }
            Some(n)
        }
    }

    #[track_caller]
    pub const fn ilog2(self) -> u32 {
        match self.checked_ilog2() {
            Some(n) => n,
            None => crate::panicking::panic("argument of integer logarithm must be positive"),
        }
    }

    #[track_caller]
    pub const fn ilog10(self) -> u32 {
        match self.checked_ilog10() {
            Some(n) => n,
            None => crate::panicking::panic("argument of integer logarithm must be positive"),
        }
    }

    #[track_caller]
    pub const fn ilog(self, base: u16) -> u32 {
        if base < 2 {
            crate::panicking::panic("base of integer logarithm must be at least 2");
        }
        match self.checked_ilog(base) {
            Some(n) => n,
            None => crate::panicking::panic("argument of integer logarithm must be positive"),
        }
    }

    pub const fn wrapping_div(self, rhs: u16) -> u16 {
        self.overflowing_div(rhs).0
    }

    pub const fn wrapping_rem(self, rhs: u16) -> u16 {
        self.overflowing_rem(rhs).0
    }

    pub const fn wrapping_div_euclid(self, rhs: u16) -> u16 {
        self.overflowing_div_euclid(rhs).0
    }

    pub const fn wrapping_rem_euclid(self, rhs: u16) -> u16 {
        self.overflowing_rem_euclid(rhs).0
    }

    pub const fn wrapping_shl(self, rhs: u32) -> u16 {
        self << (rhs & 15)
    }

    pub const fn wrapping_shr(self, rhs: u32) -> u16 {
        self >> (rhs & 15)
    }

    pub const fn wrapping_pow(self, exp: u32) -> u16 {
        if exp == 0 {
            return 1;
        }
        let mut base = self;
        let mut exp = exp;
        let mut acc: u16 = 1;
        loop {
            if exp & 1 == 1 {
                acc = acc.wrapping_mul(base);
                if exp == 1 {
                    return acc;
                }
            }
            exp /= 2;
            base = base.wrapping_mul(base);
        }
    }

    pub const fn overflowing_add(self, rhs: u16) -> (u16, bool) {
        let r = self.wrapping_add(rhs);
        (r, r < self)
    }

    pub const fn overflowing_sub(self, rhs: u16) -> (u16, bool) {
        let r = self.wrapping_sub(rhs);
        (r, r > self)
    }

    pub const fn overflowing_mul(self, rhs: u16) -> (u16, bool) {
        let r = self.wrapping_mul(rhs);

        (r, self != 0 && r / self != rhs)
    }

    pub const fn overflowing_div(self, rhs: u16) -> (u16, bool) {
        (self / rhs, false)
    }

    pub const fn overflowing_rem(self, rhs: u16) -> (u16, bool) {
        (self % rhs, false)
    }

    pub const fn overflowing_div_euclid(self, rhs: u16) -> (u16, bool) {
        (self / rhs, false)
    }

    pub const fn overflowing_rem_euclid(self, rhs: u16) -> (u16, bool) {
        (self % rhs, false)
    }

    pub const fn overflowing_neg(self) -> (u16, bool) {
        (self.wrapping_neg(), self != 0)
    }

    pub const fn overflowing_shl(self, rhs: u32) -> (u16, bool) {
        (self.wrapping_shl(rhs), rhs >= 16)
    }

    pub const fn overflowing_shr(self, rhs: u32) -> (u16, bool) {
        (self.wrapping_shr(rhs), rhs >= 16)
    }

    pub const fn overflowing_pow(self, exp: u32) -> (u16, bool) {
        if exp == 0 {
            return (1, false);
        }
        let mut base = self;
        let mut exp = exp;
        let mut acc: u16 = 1;
        let mut overflown = false;
        loop {
            if exp & 1 == 1 {
                let (r, o) = acc.overflowing_mul(base);
                if exp == 1 {
                    return (r, o || overflown);
                }
                acc = r;
                overflown = overflown || o;
            }
            exp /= 2;
            let (r, o) = base.overflowing_mul(base);
            base = r;
            overflown = overflown || o;
        }
    }

    pub const fn saturating_add(self, rhs: u16) -> u16 {
        match self.checked_add(rhs) {
            Some(x) => x,
            None => u16::MAX,
        }
    }

    pub const fn saturating_sub(self, rhs: u16) -> u16 {
        match self.checked_sub(rhs) {
            Some(x) => x,
            None => u16::MIN,
        }
    }

    pub const fn saturating_mul(self, rhs: u16) -> u16 {
        match self.checked_mul(rhs) {
            Some(x) => x,
            None => u16::MAX,
        }
    }

    pub const fn saturating_div(self, rhs: u16) -> u16 {
        self / rhs
    }

    pub const fn saturating_pow(self, exp: u32) -> u16 {
        match self.checked_pow(exp) {
            Some(x) => x,
            None => u16::MAX,
        }
    }

    pub const fn div_euclid(self, rhs: u16) -> u16 {
        self / rhs
    }

    pub const fn rem_euclid(self, rhs: u16) -> u16 {
        self % rhs
    }

    pub const fn div_ceil(self, rhs: u16) -> u16 {
        let d = self / rhs;
        let r = self % rhs;
        if r > 0 { d + 1 } else { d }
    }

    #[rustc_inherit_overflow_checks]
    pub const fn next_multiple_of(self, rhs: u16) -> u16 {
        match self % rhs {
            0 => self,
            r => self + (rhs - r),
        }
    }

    pub const fn checked_next_multiple_of(self, rhs: u16) -> Option<u16> {
        match self.checked_rem(rhs) {
            None => None,
            Some(0) => Some(self),
            Some(r) => self.checked_add(rhs - r),
        }
    }

    const fn one_less_than_next_power_of_two(self) -> u16 {
        if self <= 1 {
            return 0;
        }
        let p = self - 1;
        u16::MAX >> p.leading_zeros()
    }

    #[rustc_inherit_overflow_checks]
    pub const fn next_power_of_two(self) -> u16 {
        self.one_less_than_next_power_of_two() + 1
    }

    pub const fn checked_next_power_of_two(self) -> Option<u16> {
        self.one_less_than_next_power_of_two().checked_add(1)
    }

    pub const fn isqrt(self) -> u16 {
        if self < 2 {
            return self;
        }
        let mut op = self;
        let mut res: u16 = 0;
        let mut one: u16 = 1 << 14;
        while one > op {
            one >>= 2;
        }
        while one != 0 {
            if op >= res + one {
                op -= res + one;
                res = (res >> 1) + one;
            } else {
                res >>= 1;
            }
            one >>= 2;
        }
        res
    }

    pub const fn midpoint(self, rhs: u16) -> u16 {
        (self & rhs) + ((self ^ rhs) >> 1)
    }

    pub const fn cast_signed(self) -> i16 {
        self as i16
    }

    pub const fn checked_add_signed(self, rhs: i16) -> Option<u16> {
        let (r, o) = self.overflowing_add_signed(rhs);
        if o { None } else { Some(r) }
    }

    pub const fn wrapping_add_signed(self, rhs: i16) -> u16 {
        self.wrapping_add(rhs as u16)
    }

    pub const fn saturating_add_signed(self, rhs: i16) -> u16 {
        let (res, overflow) = self.overflowing_add(rhs as u16);
        if overflow == (rhs < 0) {
            res
        } else if overflow {
            u16::MAX
        } else {
            0
        }
    }

    pub const fn overflowing_add_signed(self, rhs: i16) -> (u16, bool) {
        let (res, overflowed) = self.overflowing_add(rhs as u16);
        (res, overflowed != (rhs < 0))
    }

    pub const fn unbounded_shl(self, rhs: u32) -> u16 {
        if rhs < 16 { self << rhs } else { 0 }
    }

    pub const fn unbounded_shr(self, rhs: u32) -> u16 {
        if rhs < 16 { self >> rhs } else { 0 }
    }

    pub const fn leading_ones(self) -> u32 {
        (!self).leading_zeros()
    }

    pub const fn trailing_ones(self) -> u32 {
        (!self).trailing_zeros()
    }

    pub const fn swap_bytes(self) -> u16 {
        let mut x = self as u16;
        let mut r: u16 = 0;
        let mut i = 0;
        while i < 2 {
            r = (r << 8) | (x & 0xff);
            x >>= 8;
            i += 1;
        }
        r as u16
    }

    pub const fn reverse_bits(self) -> u16 {
        let mut x = self as u16;
        let mut r: u16 = 0;
        let mut i = 0;
        while i < 16 {
            r = (r << 1) | (x & 1);
            x >>= 1;
            i += 1;
        }
        r as u16
    }

    pub const fn to_be(self) -> u16 {
        self.swap_bytes()
    }

    pub const fn to_le(self) -> u16 {
        self
    }

    pub const fn from_be(x: u16) -> u16 {
        x.swap_bytes()
    }

    pub const fn from_le(x: u16) -> u16 {
        x
    }

    pub const fn to_le_bytes(self) -> [u8; 2] {
        let x = self as u16;
        [x as u8, (x >> 8) as u8]
    }

    pub const fn to_be_bytes(self) -> [u8; 2] {
        self.swap_bytes().to_le_bytes()
    }

    pub const fn to_ne_bytes(self) -> [u8; 2] {
        self.to_le_bytes()
    }

    pub const fn from_le_bytes(bytes: [u8; 2]) -> u16 {
        (bytes[0] as u16 | (bytes[1] as u16) << 8) as u16
    }

    pub const fn from_be_bytes(bytes: [u8; 2]) -> u16 {
        u16::from_le_bytes(bytes).swap_bytes()
    }

    pub const fn from_ne_bytes(bytes: [u8; 2]) -> u16 {
        u16::from_le_bytes(bytes)
    }
}

impl u32 {
    pub const fn checked_add(self, rhs: u32) -> Option<u32> {
        let (r, o) = self.overflowing_add(rhs);
        if o { None } else { Some(r) }
    }

    pub const fn checked_sub(self, rhs: u32) -> Option<u32> {
        let (r, o) = self.overflowing_sub(rhs);
        if o { None } else { Some(r) }
    }

    pub const fn checked_mul(self, rhs: u32) -> Option<u32> {
        let (r, o) = self.overflowing_mul(rhs);
        if o { None } else { Some(r) }
    }

    pub const fn checked_div(self, rhs: u32) -> Option<u32> {
        if rhs == 0 {
            None
        } else {
            Some(self / rhs)
        }
    }

    pub const fn checked_rem(self, rhs: u32) -> Option<u32> {
        if rhs == 0 {
            None
        } else {
            Some(self % rhs)
        }
    }

    pub const fn checked_div_euclid(self, rhs: u32) -> Option<u32> {
        if rhs == 0 {
            None
        } else {
            Some(self.div_euclid(rhs))
        }
    }

    pub const fn checked_rem_euclid(self, rhs: u32) -> Option<u32> {
        if rhs == 0 {
            None
        } else {
            Some(self.rem_euclid(rhs))
        }
    }

    pub const fn checked_neg(self) -> Option<u32> {
        let (r, o) = self.overflowing_neg();
        if o { None } else { Some(r) }
    }

    pub const fn checked_shl(self, rhs: u32) -> Option<u32> {
        if rhs < 32 { Some(self << rhs) } else { None }
    }

    pub const fn checked_shr(self, rhs: u32) -> Option<u32> {
        if rhs < 32 { Some(self >> rhs) } else { None }
    }

    pub const fn checked_pow(self, exp: u32) -> Option<u32> {
        if exp == 0 {
            return Some(1);
        }
        let mut base = self;
        let mut exp = exp;
        let mut acc: u32 = 1;
        loop {
            if exp & 1 == 1 {
                acc = match acc.checked_mul(base) {
                    Some(x) => x,
                    None => return None,
                };
                if exp == 1 {
                    return Some(acc);
                }
            }
            exp /= 2;
            base = match base.checked_mul(base) {
                Some(x) => x,
                None => return None,
            };
        }
    }

    pub const fn checked_ilog2(self) -> Option<u32> {
        if self <= 0 { None } else { Some(32 - 1 - self.leading_zeros()) }
    }

    pub const fn checked_ilog10(self) -> Option<u32> {
        self.checked_ilog(10)
    }

    pub const fn checked_ilog(self, base: u32) -> Option<u32> {
        if self <= 0 || base <= 1 {
            None
        } else {
            let mut n = 0;
            let mut r = self;
            while r >= base {
                r /= base;
                n += 1;
            }
            Some(n)
        }
    }

    #[track_caller]
    pub const fn ilog2(self) -> u32 {
        match self.checked_ilog2() {
            Some(n) => n,
            None => crate::panicking::panic("argument of integer logarithm must be positive"),
        }
    }

    #[track_caller]
    pub const fn ilog10(self) -> u32 {
        match self.checked_ilog10() {
            Some(n) => n,
            None => crate::panicking::panic("argument of integer logarithm must be positive"),
        }
    }

    #[track_caller]
    pub const fn ilog(self, base: u32) -> u32 {
        if base < 2 {
            crate::panicking::panic("base of integer logarithm must be at least 2");
        }
        match self.checked_ilog(base) {
            Some(n) => n,
            None => crate::panicking::panic("argument of integer logarithm must be positive"),
        }
    }

    pub const fn wrapping_div(self, rhs: u32) -> u32 {
        self.overflowing_div(rhs).0
    }

    pub const fn wrapping_rem(self, rhs: u32) -> u32 {
        self.overflowing_rem(rhs).0
    }

    pub const fn wrapping_div_euclid(self, rhs: u32) -> u32 {
        self.overflowing_div_euclid(rhs).0
    }

    pub const fn wrapping_rem_euclid(self, rhs: u32) -> u32 {
        self.overflowing_rem_euclid(rhs).0
    }

    pub const fn wrapping_shl(self, rhs: u32) -> u32 {
        self << (rhs & 31)
    }

    pub const fn wrapping_shr(self, rhs: u32) -> u32 {
        self >> (rhs & 31)
    }

    pub const fn wrapping_pow(self, exp: u32) -> u32 {
        if exp == 0 {
            return 1;
        }
        let mut base = self;
        let mut exp = exp;
        let mut acc: u32 = 1;
        loop {
            if exp & 1 == 1 {
                acc = acc.wrapping_mul(base);
                if exp == 1 {
                    return acc;
                }
            }
            exp /= 2;
            base = base.wrapping_mul(base);
        }
    }

    pub const fn overflowing_add(self, rhs: u32) -> (u32, bool) {
        let r = self.wrapping_add(rhs);
        (r, r < self)
    }

    pub const fn overflowing_sub(self, rhs: u32) -> (u32, bool) {
        let r = self.wrapping_sub(rhs);
        (r, r > self)
    }

    pub const fn overflowing_mul(self, rhs: u32) -> (u32, bool) {
        let r = self.wrapping_mul(rhs);

        (r, self != 0 && r / self != rhs)
    }

    pub const fn overflowing_div(self, rhs: u32) -> (u32, bool) {
        (self / rhs, false)
    }

    pub const fn overflowing_rem(self, rhs: u32) -> (u32, bool) {
        (self % rhs, false)
    }

    pub const fn overflowing_div_euclid(self, rhs: u32) -> (u32, bool) {
        (self / rhs, false)
    }

    pub const fn overflowing_rem_euclid(self, rhs: u32) -> (u32, bool) {
        (self % rhs, false)
    }

    pub const fn overflowing_neg(self) -> (u32, bool) {
        (self.wrapping_neg(), self != 0)
    }

    pub const fn overflowing_shl(self, rhs: u32) -> (u32, bool) {
        (self.wrapping_shl(rhs), rhs >= 32)
    }

    pub const fn overflowing_shr(self, rhs: u32) -> (u32, bool) {
        (self.wrapping_shr(rhs), rhs >= 32)
    }

    pub const fn overflowing_pow(self, exp: u32) -> (u32, bool) {
        if exp == 0 {
            return (1, false);
        }
        let mut base = self;
        let mut exp = exp;
        let mut acc: u32 = 1;
        let mut overflown = false;
        loop {
            if exp & 1 == 1 {
                let (r, o) = acc.overflowing_mul(base);
                if exp == 1 {
                    return (r, o || overflown);
                }
                acc = r;
                overflown = overflown || o;
            }
            exp /= 2;
            let (r, o) = base.overflowing_mul(base);
            base = r;
            overflown = overflown || o;
        }
    }

    pub const fn saturating_add(self, rhs: u32) -> u32 {
        match self.checked_add(rhs) {
            Some(x) => x,
            None => u32::MAX,
        }
    }

    pub const fn saturating_sub(self, rhs: u32) -> u32 {
        match self.checked_sub(rhs) {
            Some(x) => x,
            None => u32::MIN,
        }
    }

    pub const fn saturating_mul(self, rhs: u32) -> u32 {
        match self.checked_mul(rhs) {
            Some(x) => x,
            None => u32::MAX,
        }
    }

    pub const fn saturating_div(self, rhs: u32) -> u32 {
        self / rhs
    }

    pub const fn saturating_pow(self, exp: u32) -> u32 {
        match self.checked_pow(exp) {
            Some(x) => x,
            None => u32::MAX,
        }
    }

    pub const fn div_euclid(self, rhs: u32) -> u32 {
        self / rhs
    }

    pub const fn rem_euclid(self, rhs: u32) -> u32 {
        self % rhs
    }

    pub const fn div_ceil(self, rhs: u32) -> u32 {
        let d = self / rhs;
        let r = self % rhs;
        if r > 0 { d + 1 } else { d }
    }

    #[rustc_inherit_overflow_checks]
    pub const fn next_multiple_of(self, rhs: u32) -> u32 {
        match self % rhs {
            0 => self,
            r => self + (rhs - r),
        }
    }

    pub const fn checked_next_multiple_of(self, rhs: u32) -> Option<u32> {
        match self.checked_rem(rhs) {
            None => None,
            Some(0) => Some(self),
            Some(r) => self.checked_add(rhs - r),
        }
    }

    const fn one_less_than_next_power_of_two(self) -> u32 {
        if self <= 1 {
            return 0;
        }
        let p = self - 1;
        u32::MAX >> p.leading_zeros()
    }

    #[rustc_inherit_overflow_checks]
    pub const fn next_power_of_two(self) -> u32 {
        self.one_less_than_next_power_of_two() + 1
    }

    pub const fn checked_next_power_of_two(self) -> Option<u32> {
        self.one_less_than_next_power_of_two().checked_add(1)
    }

    pub const fn isqrt(self) -> u32 {
        if self < 2 {
            return self;
        }
        let mut op = self;
        let mut res: u32 = 0;
        let mut one: u32 = 1 << 30;
        while one > op {
            one >>= 2;
        }
        while one != 0 {
            if op >= res + one {
                op -= res + one;
                res = (res >> 1) + one;
            } else {
                res >>= 1;
            }
            one >>= 2;
        }
        res
    }

    pub const fn midpoint(self, rhs: u32) -> u32 {
        (self & rhs) + ((self ^ rhs) >> 1)
    }

    pub const fn cast_signed(self) -> i32 {
        self as i32
    }

    pub const fn checked_add_signed(self, rhs: i32) -> Option<u32> {
        let (r, o) = self.overflowing_add_signed(rhs);
        if o { None } else { Some(r) }
    }

    pub const fn wrapping_add_signed(self, rhs: i32) -> u32 {
        self.wrapping_add(rhs as u32)
    }

    pub const fn saturating_add_signed(self, rhs: i32) -> u32 {
        let (res, overflow) = self.overflowing_add(rhs as u32);
        if overflow == (rhs < 0) {
            res
        } else if overflow {
            u32::MAX
        } else {
            0
        }
    }

    pub const fn overflowing_add_signed(self, rhs: i32) -> (u32, bool) {
        let (res, overflowed) = self.overflowing_add(rhs as u32);
        (res, overflowed != (rhs < 0))
    }

    pub const fn unbounded_shl(self, rhs: u32) -> u32 {
        if rhs < 32 { self << rhs } else { 0 }
    }

    pub const fn unbounded_shr(self, rhs: u32) -> u32 {
        if rhs < 32 { self >> rhs } else { 0 }
    }

    pub const fn leading_ones(self) -> u32 {
        (!self).leading_zeros()
    }

    pub const fn trailing_ones(self) -> u32 {
        (!self).trailing_zeros()
    }

    pub const fn swap_bytes(self) -> u32 {
        let mut x = self as u32;
        let mut r: u32 = 0;
        let mut i = 0;
        while i < 4 {
            r = (r << 8) | (x & 0xff);
            x >>= 8;
            i += 1;
        }
        r as u32
    }

    pub const fn reverse_bits(self) -> u32 {
        let mut x = self as u32;
        let mut r: u32 = 0;
        let mut i = 0;
        while i < 32 {
            r = (r << 1) | (x & 1);
            x >>= 1;
            i += 1;
        }
        r as u32
    }

    pub const fn to_be(self) -> u32 {
        self.swap_bytes()
    }

    pub const fn to_le(self) -> u32 {
        self
    }

    pub const fn from_be(x: u32) -> u32 {
        x.swap_bytes()
    }

    pub const fn from_le(x: u32) -> u32 {
        x
    }

    pub const fn to_le_bytes(self) -> [u8; 4] {
        let x = self as u32;
        [x as u8, (x >> 8) as u8, (x >> 16) as u8, (x >> 24) as u8]
    }

    pub const fn to_be_bytes(self) -> [u8; 4] {
        self.swap_bytes().to_le_bytes()
    }

    pub const fn to_ne_bytes(self) -> [u8; 4] {
        self.to_le_bytes()
    }

    pub const fn from_le_bytes(bytes: [u8; 4]) -> u32 {
        (bytes[0] as u32 | (bytes[1] as u32) << 8 | (bytes[2] as u32) << 16 | (bytes[3] as u32) << 24) as u32
    }

    pub const fn from_be_bytes(bytes: [u8; 4]) -> u32 {
        u32::from_le_bytes(bytes).swap_bytes()
    }

    pub const fn from_ne_bytes(bytes: [u8; 4]) -> u32 {
        u32::from_le_bytes(bytes)
    }
}

impl u64 {
    pub const fn checked_add(self, rhs: u64) -> Option<u64> {
        let (r, o) = self.overflowing_add(rhs);
        if o { None } else { Some(r) }
    }

    pub const fn checked_sub(self, rhs: u64) -> Option<u64> {
        let (r, o) = self.overflowing_sub(rhs);
        if o { None } else { Some(r) }
    }

    pub const fn checked_mul(self, rhs: u64) -> Option<u64> {
        let (r, o) = self.overflowing_mul(rhs);
        if o { None } else { Some(r) }
    }

    pub const fn checked_div(self, rhs: u64) -> Option<u64> {
        if rhs == 0 {
            None
        } else {
            Some(self / rhs)
        }
    }

    pub const fn checked_rem(self, rhs: u64) -> Option<u64> {
        if rhs == 0 {
            None
        } else {
            Some(self % rhs)
        }
    }

    pub const fn checked_div_euclid(self, rhs: u64) -> Option<u64> {
        if rhs == 0 {
            None
        } else {
            Some(self.div_euclid(rhs))
        }
    }

    pub const fn checked_rem_euclid(self, rhs: u64) -> Option<u64> {
        if rhs == 0 {
            None
        } else {
            Some(self.rem_euclid(rhs))
        }
    }

    pub const fn checked_neg(self) -> Option<u64> {
        let (r, o) = self.overflowing_neg();
        if o { None } else { Some(r) }
    }

    pub const fn checked_shl(self, rhs: u32) -> Option<u64> {
        if rhs < 64 { Some(self << rhs) } else { None }
    }

    pub const fn checked_shr(self, rhs: u32) -> Option<u64> {
        if rhs < 64 { Some(self >> rhs) } else { None }
    }

    pub const fn checked_pow(self, exp: u32) -> Option<u64> {
        if exp == 0 {
            return Some(1);
        }
        let mut base = self;
        let mut exp = exp;
        let mut acc: u64 = 1;
        loop {
            if exp & 1 == 1 {
                acc = match acc.checked_mul(base) {
                    Some(x) => x,
                    None => return None,
                };
                if exp == 1 {
                    return Some(acc);
                }
            }
            exp /= 2;
            base = match base.checked_mul(base) {
                Some(x) => x,
                None => return None,
            };
        }
    }

    pub const fn checked_ilog2(self) -> Option<u32> {
        if self <= 0 { None } else { Some(64 - 1 - self.leading_zeros()) }
    }

    pub const fn checked_ilog10(self) -> Option<u32> {
        self.checked_ilog(10)
    }

    pub const fn checked_ilog(self, base: u64) -> Option<u32> {
        if self <= 0 || base <= 1 {
            None
        } else {
            let mut n = 0;
            let mut r = self;
            while r >= base {
                r /= base;
                n += 1;
            }
            Some(n)
        }
    }

    #[track_caller]
    pub const fn ilog2(self) -> u32 {
        match self.checked_ilog2() {
            Some(n) => n,
            None => crate::panicking::panic("argument of integer logarithm must be positive"),
        }
    }

    #[track_caller]
    pub const fn ilog10(self) -> u32 {
        match self.checked_ilog10() {
            Some(n) => n,
            None => crate::panicking::panic("argument of integer logarithm must be positive"),
        }
    }

    #[track_caller]
    pub const fn ilog(self, base: u64) -> u32 {
        if base < 2 {
            crate::panicking::panic("base of integer logarithm must be at least 2");
        }
        match self.checked_ilog(base) {
            Some(n) => n,
            None => crate::panicking::panic("argument of integer logarithm must be positive"),
        }
    }

    pub const fn wrapping_div(self, rhs: u64) -> u64 {
        self.overflowing_div(rhs).0
    }

    pub const fn wrapping_rem(self, rhs: u64) -> u64 {
        self.overflowing_rem(rhs).0
    }

    pub const fn wrapping_div_euclid(self, rhs: u64) -> u64 {
        self.overflowing_div_euclid(rhs).0
    }

    pub const fn wrapping_rem_euclid(self, rhs: u64) -> u64 {
        self.overflowing_rem_euclid(rhs).0
    }

    pub const fn wrapping_shl(self, rhs: u32) -> u64 {
        self << (rhs & 63)
    }

    pub const fn wrapping_shr(self, rhs: u32) -> u64 {
        self >> (rhs & 63)
    }

    pub const fn wrapping_pow(self, exp: u32) -> u64 {
        if exp == 0 {
            return 1;
        }
        let mut base = self;
        let mut exp = exp;
        let mut acc: u64 = 1;
        loop {
            if exp & 1 == 1 {
                acc = acc.wrapping_mul(base);
                if exp == 1 {
                    return acc;
                }
            }
            exp /= 2;
            base = base.wrapping_mul(base);
        }
    }

    pub const fn overflowing_add(self, rhs: u64) -> (u64, bool) {
        let r = self.wrapping_add(rhs);
        (r, r < self)
    }

    pub const fn overflowing_sub(self, rhs: u64) -> (u64, bool) {
        let r = self.wrapping_sub(rhs);
        (r, r > self)
    }

    pub const fn overflowing_mul(self, rhs: u64) -> (u64, bool) {
        let r = self.wrapping_mul(rhs);

        (r, self != 0 && r / self != rhs)
    }

    pub const fn overflowing_div(self, rhs: u64) -> (u64, bool) {
        (self / rhs, false)
    }

    pub const fn overflowing_rem(self, rhs: u64) -> (u64, bool) {
        (self % rhs, false)
    }

    pub const fn overflowing_div_euclid(self, rhs: u64) -> (u64, bool) {
        (self / rhs, false)
    }

    pub const fn overflowing_rem_euclid(self, rhs: u64) -> (u64, bool) {
        (self % rhs, false)
    }

    pub const fn overflowing_neg(self) -> (u64, bool) {
        (self.wrapping_neg(), self != 0)
    }

    pub const fn overflowing_shl(self, rhs: u32) -> (u64, bool) {
        (self.wrapping_shl(rhs), rhs >= 64)
    }

    pub const fn overflowing_shr(self, rhs: u32) -> (u64, bool) {
        (self.wrapping_shr(rhs), rhs >= 64)
    }

    pub const fn overflowing_pow(self, exp: u32) -> (u64, bool) {
        if exp == 0 {
            return (1, false);
        }
        let mut base = self;
        let mut exp = exp;
        let mut acc: u64 = 1;
        let mut overflown = false;
        loop {
            if exp & 1 == 1 {
                let (r, o) = acc.overflowing_mul(base);
                if exp == 1 {
                    return (r, o || overflown);
                }
                acc = r;
                overflown = overflown || o;
            }
            exp /= 2;
            let (r, o) = base.overflowing_mul(base);
            base = r;
            overflown = overflown || o;
        }
    }

    pub const fn saturating_add(self, rhs: u64) -> u64 {
        match self.checked_add(rhs) {
            Some(x) => x,
            None => u64::MAX,
        }
    }

    pub const fn saturating_sub(self, rhs: u64) -> u64 {
        match self.checked_sub(rhs) {
            Some(x) => x,
            None => u64::MIN,
        }
    }

    pub const fn saturating_mul(self, rhs: u64) -> u64 {
        match self.checked_mul(rhs) {
            Some(x) => x,
            None => u64::MAX,
        }
    }

    pub const fn saturating_div(self, rhs: u64) -> u64 {
        self / rhs
    }

    pub const fn saturating_pow(self, exp: u32) -> u64 {
        match self.checked_pow(exp) {
            Some(x) => x,
            None => u64::MAX,
        }
    }

    pub const fn div_euclid(self, rhs: u64) -> u64 {
        self / rhs
    }

    pub const fn rem_euclid(self, rhs: u64) -> u64 {
        self % rhs
    }

    pub const fn div_ceil(self, rhs: u64) -> u64 {
        let d = self / rhs;
        let r = self % rhs;
        if r > 0 { d + 1 } else { d }
    }

    #[rustc_inherit_overflow_checks]
    pub const fn next_multiple_of(self, rhs: u64) -> u64 {
        match self % rhs {
            0 => self,
            r => self + (rhs - r),
        }
    }

    pub const fn checked_next_multiple_of(self, rhs: u64) -> Option<u64> {
        match self.checked_rem(rhs) {
            None => None,
            Some(0) => Some(self),
            Some(r) => self.checked_add(rhs - r),
        }
    }

    const fn one_less_than_next_power_of_two(self) -> u64 {
        if self <= 1 {
            return 0;
        }
        let p = self - 1;
        u64::MAX >> p.leading_zeros()
    }

    #[rustc_inherit_overflow_checks]
    pub const fn next_power_of_two(self) -> u64 {
        self.one_less_than_next_power_of_two() + 1
    }

    pub const fn checked_next_power_of_two(self) -> Option<u64> {
        self.one_less_than_next_power_of_two().checked_add(1)
    }

    pub const fn isqrt(self) -> u64 {
        if self < 2 {
            return self;
        }
        let mut op = self;
        let mut res: u64 = 0;
        let mut one: u64 = 1 << 62;
        while one > op {
            one >>= 2;
        }
        while one != 0 {
            if op >= res + one {
                op -= res + one;
                res = (res >> 1) + one;
            } else {
                res >>= 1;
            }
            one >>= 2;
        }
        res
    }

    pub const fn midpoint(self, rhs: u64) -> u64 {
        (self & rhs) + ((self ^ rhs) >> 1)
    }

    pub const fn cast_signed(self) -> i64 {
        self as i64
    }

    pub const fn checked_add_signed(self, rhs: i64) -> Option<u64> {
        let (r, o) = self.overflowing_add_signed(rhs);
        if o { None } else { Some(r) }
    }

    pub const fn wrapping_add_signed(self, rhs: i64) -> u64 {
        self.wrapping_add(rhs as u64)
    }

    pub const fn saturating_add_signed(self, rhs: i64) -> u64 {
        let (res, overflow) = self.overflowing_add(rhs as u64);
        if overflow == (rhs < 0) {
            res
        } else if overflow {
            u64::MAX
        } else {
            0
        }
    }

    pub const fn overflowing_add_signed(self, rhs: i64) -> (u64, bool) {
        let (res, overflowed) = self.overflowing_add(rhs as u64);
        (res, overflowed != (rhs < 0))
    }

    pub const fn unbounded_shl(self, rhs: u32) -> u64 {
        if rhs < 64 { self << rhs } else { 0 }
    }

    pub const fn unbounded_shr(self, rhs: u32) -> u64 {
        if rhs < 64 { self >> rhs } else { 0 }
    }

    pub const fn leading_ones(self) -> u32 {
        (!self).leading_zeros()
    }

    pub const fn trailing_ones(self) -> u32 {
        (!self).trailing_zeros()
    }

    pub const fn swap_bytes(self) -> u64 {
        let mut x = self as u64;
        let mut r: u64 = 0;
        let mut i = 0;
        while i < 8 {
            r = (r << 8) | (x & 0xff);
            x >>= 8;
            i += 1;
        }
        r as u64
    }

    pub const fn reverse_bits(self) -> u64 {
        let mut x = self as u64;
        let mut r: u64 = 0;
        let mut i = 0;
        while i < 64 {
            r = (r << 1) | (x & 1);
            x >>= 1;
            i += 1;
        }
        r as u64
    }

    pub const fn to_be(self) -> u64 {
        self.swap_bytes()
    }

    pub const fn to_le(self) -> u64 {
        self
    }

    pub const fn from_be(x: u64) -> u64 {
        x.swap_bytes()
    }

    pub const fn from_le(x: u64) -> u64 {
        x
    }

    pub const fn to_le_bytes(self) -> [u8; 8] {
        let x = self as u64;
        [x as u8, (x >> 8) as u8, (x >> 16) as u8, (x >> 24) as u8, (x >> 32) as u8, (x >> 40) as u8, (x >> 48) as u8, (x >> 56) as u8]
    }

    pub const fn to_be_bytes(self) -> [u8; 8] {
        self.swap_bytes().to_le_bytes()
    }

    pub const fn to_ne_bytes(self) -> [u8; 8] {
        self.to_le_bytes()
    }

    pub const fn from_le_bytes(bytes: [u8; 8]) -> u64 {
        (bytes[0] as u64 | (bytes[1] as u64) << 8 | (bytes[2] as u64) << 16 | (bytes[3] as u64) << 24 | (bytes[4] as u64) << 32 | (bytes[5] as u64) << 40 | (bytes[6] as u64) << 48 | (bytes[7] as u64) << 56) as u64
    }

    pub const fn from_be_bytes(bytes: [u8; 8]) -> u64 {
        u64::from_le_bytes(bytes).swap_bytes()
    }

    pub const fn from_ne_bytes(bytes: [u8; 8]) -> u64 {
        u64::from_le_bytes(bytes)
    }
}

impl u128 {
    pub const fn checked_add(self, rhs: u128) -> Option<u128> {
        let (r, o) = self.overflowing_add(rhs);
        if o { None } else { Some(r) }
    }

    pub const fn checked_sub(self, rhs: u128) -> Option<u128> {
        let (r, o) = self.overflowing_sub(rhs);
        if o { None } else { Some(r) }
    }

    pub const fn checked_mul(self, rhs: u128) -> Option<u128> {
        let (r, o) = self.overflowing_mul(rhs);
        if o { None } else { Some(r) }
    }

    pub const fn checked_div(self, rhs: u128) -> Option<u128> {
        if rhs == 0 {
            None
        } else {
            Some(self / rhs)
        }
    }

    pub const fn checked_rem(self, rhs: u128) -> Option<u128> {
        if rhs == 0 {
            None
        } else {
            Some(self % rhs)
        }
    }

    pub const fn checked_div_euclid(self, rhs: u128) -> Option<u128> {
        if rhs == 0 {
            None
        } else {
            Some(self.div_euclid(rhs))
        }
    }

    pub const fn checked_rem_euclid(self, rhs: u128) -> Option<u128> {
        if rhs == 0 {
            None
        } else {
            Some(self.rem_euclid(rhs))
        }
    }

    pub const fn checked_neg(self) -> Option<u128> {
        let (r, o) = self.overflowing_neg();
        if o { None } else { Some(r) }
    }

    pub const fn checked_shl(self, rhs: u32) -> Option<u128> {
        if rhs < 128 { Some(self << rhs) } else { None }
    }

    pub const fn checked_shr(self, rhs: u32) -> Option<u128> {
        if rhs < 128 { Some(self >> rhs) } else { None }
    }

    pub const fn checked_pow(self, exp: u32) -> Option<u128> {
        if exp == 0 {
            return Some(1);
        }
        let mut base = self;
        let mut exp = exp;
        let mut acc: u128 = 1;
        loop {
            if exp & 1 == 1 {
                acc = match acc.checked_mul(base) {
                    Some(x) => x,
                    None => return None,
                };
                if exp == 1 {
                    return Some(acc);
                }
            }
            exp /= 2;
            base = match base.checked_mul(base) {
                Some(x) => x,
                None => return None,
            };
        }
    }

    pub const fn checked_ilog2(self) -> Option<u32> {
        if self <= 0 { None } else { Some(128 - 1 - self.leading_zeros()) }
    }

    pub const fn checked_ilog10(self) -> Option<u32> {
        self.checked_ilog(10)
    }

    pub const fn checked_ilog(self, base: u128) -> Option<u32> {
        if self <= 0 || base <= 1 {
            None
        } else {
            let mut n = 0;
            let mut r = self;
            while r >= base {
                r /= base;
                n += 1;
            }
            Some(n)
        }
    }

    #[track_caller]
    pub const fn ilog2(self) -> u32 {
        match self.checked_ilog2() {
            Some(n) => n,
            None => crate::panicking::panic("argument of integer logarithm must be positive"),
        }
    }

    #[track_caller]
    pub const fn ilog10(self) -> u32 {
        match self.checked_ilog10() {
            Some(n) => n,
            None => crate::panicking::panic("argument of integer logarithm must be positive"),
        }
    }

    #[track_caller]
    pub const fn ilog(self, base: u128) -> u32 {
        if base < 2 {
            crate::panicking::panic("base of integer logarithm must be at least 2");
        }
        match self.checked_ilog(base) {
            Some(n) => n,
            None => crate::panicking::panic("argument of integer logarithm must be positive"),
        }
    }

    pub const fn wrapping_div(self, rhs: u128) -> u128 {
        self.overflowing_div(rhs).0
    }

    pub const fn wrapping_rem(self, rhs: u128) -> u128 {
        self.overflowing_rem(rhs).0
    }

    pub const fn wrapping_div_euclid(self, rhs: u128) -> u128 {
        self.overflowing_div_euclid(rhs).0
    }

    pub const fn wrapping_rem_euclid(self, rhs: u128) -> u128 {
        self.overflowing_rem_euclid(rhs).0
    }

    pub const fn wrapping_shl(self, rhs: u32) -> u128 {
        self << (rhs & 127)
    }

    pub const fn wrapping_shr(self, rhs: u32) -> u128 {
        self >> (rhs & 127)
    }

    pub const fn wrapping_pow(self, exp: u32) -> u128 {
        if exp == 0 {
            return 1;
        }
        let mut base = self;
        let mut exp = exp;
        let mut acc: u128 = 1;
        loop {
            if exp & 1 == 1 {
                acc = acc.wrapping_mul(base);
                if exp == 1 {
                    return acc;
                }
            }
            exp /= 2;
            base = base.wrapping_mul(base);
        }
    }

    pub const fn overflowing_add(self, rhs: u128) -> (u128, bool) {
        let r = self.wrapping_add(rhs);
        (r, r < self)
    }

    pub const fn overflowing_sub(self, rhs: u128) -> (u128, bool) {
        let r = self.wrapping_sub(rhs);
        (r, r > self)
    }

    pub const fn overflowing_mul(self, rhs: u128) -> (u128, bool) {
        let r = self.wrapping_mul(rhs);

        (r, self != 0 && r / self != rhs)
    }

    pub const fn overflowing_div(self, rhs: u128) -> (u128, bool) {
        (self / rhs, false)
    }

    pub const fn overflowing_rem(self, rhs: u128) -> (u128, bool) {
        (self % rhs, false)
    }

    pub const fn overflowing_div_euclid(self, rhs: u128) -> (u128, bool) {
        (self / rhs, false)
    }

    pub const fn overflowing_rem_euclid(self, rhs: u128) -> (u128, bool) {
        (self % rhs, false)
    }

    pub const fn overflowing_neg(self) -> (u128, bool) {
        (self.wrapping_neg(), self != 0)
    }

    pub const fn overflowing_shl(self, rhs: u32) -> (u128, bool) {
        (self.wrapping_shl(rhs), rhs >= 128)
    }

    pub const fn overflowing_shr(self, rhs: u32) -> (u128, bool) {
        (self.wrapping_shr(rhs), rhs >= 128)
    }

    pub const fn overflowing_pow(self, exp: u32) -> (u128, bool) {
        if exp == 0 {
            return (1, false);
        }
        let mut base = self;
        let mut exp = exp;
        let mut acc: u128 = 1;
        let mut overflown = false;
        loop {
            if exp & 1 == 1 {
                let (r, o) = acc.overflowing_mul(base);
                if exp == 1 {
                    return (r, o || overflown);
                }
                acc = r;
                overflown = overflown || o;
            }
            exp /= 2;
            let (r, o) = base.overflowing_mul(base);
            base = r;
            overflown = overflown || o;
        }
    }

    pub const fn saturating_add(self, rhs: u128) -> u128 {
        match self.checked_add(rhs) {
            Some(x) => x,
            None => u128::MAX,
        }
    }

    pub const fn saturating_sub(self, rhs: u128) -> u128 {
        match self.checked_sub(rhs) {
            Some(x) => x,
            None => u128::MIN,
        }
    }

    pub const fn saturating_mul(self, rhs: u128) -> u128 {
        match self.checked_mul(rhs) {
            Some(x) => x,
            None => u128::MAX,
        }
    }

    pub const fn saturating_div(self, rhs: u128) -> u128 {
        self / rhs
    }

    pub const fn saturating_pow(self, exp: u32) -> u128 {
        match self.checked_pow(exp) {
            Some(x) => x,
            None => u128::MAX,
        }
    }

    pub const fn div_euclid(self, rhs: u128) -> u128 {
        self / rhs
    }

    pub const fn rem_euclid(self, rhs: u128) -> u128 {
        self % rhs
    }

    pub const fn div_ceil(self, rhs: u128) -> u128 {
        let d = self / rhs;
        let r = self % rhs;
        if r > 0 { d + 1 } else { d }
    }

    #[rustc_inherit_overflow_checks]
    pub const fn next_multiple_of(self, rhs: u128) -> u128 {
        match self % rhs {
            0 => self,
            r => self + (rhs - r),
        }
    }

    pub const fn checked_next_multiple_of(self, rhs: u128) -> Option<u128> {
        match self.checked_rem(rhs) {
            None => None,
            Some(0) => Some(self),
            Some(r) => self.checked_add(rhs - r),
        }
    }

    const fn one_less_than_next_power_of_two(self) -> u128 {
        if self <= 1 {
            return 0;
        }
        let p = self - 1;
        u128::MAX >> p.leading_zeros()
    }

    #[rustc_inherit_overflow_checks]
    pub const fn next_power_of_two(self) -> u128 {
        self.one_less_than_next_power_of_two() + 1
    }

    pub const fn checked_next_power_of_two(self) -> Option<u128> {
        self.one_less_than_next_power_of_two().checked_add(1)
    }

    pub const fn isqrt(self) -> u128 {
        if self < 2 {
            return self;
        }
        let mut op = self;
        let mut res: u128 = 0;
        let mut one: u128 = 1 << 126;
        while one > op {
            one >>= 2;
        }
        while one != 0 {
            if op >= res + one {
                op -= res + one;
                res = (res >> 1) + one;
            } else {
                res >>= 1;
            }
            one >>= 2;
        }
        res
    }

    pub const fn midpoint(self, rhs: u128) -> u128 {
        (self & rhs) + ((self ^ rhs) >> 1)
    }

    pub const fn cast_signed(self) -> i128 {
        self as i128
    }

    pub const fn checked_add_signed(self, rhs: i128) -> Option<u128> {
        let (r, o) = self.overflowing_add_signed(rhs);
        if o { None } else { Some(r) }
    }

    pub const fn wrapping_add_signed(self, rhs: i128) -> u128 {
        self.wrapping_add(rhs as u128)
    }

    pub const fn saturating_add_signed(self, rhs: i128) -> u128 {
        let (res, overflow) = self.overflowing_add(rhs as u128);
        if overflow == (rhs < 0) {
            res
        } else if overflow {
            u128::MAX
        } else {
            0
        }
    }

    pub const fn overflowing_add_signed(self, rhs: i128) -> (u128, bool) {
        let (res, overflowed) = self.overflowing_add(rhs as u128);
        (res, overflowed != (rhs < 0))
    }

    pub const fn unbounded_shl(self, rhs: u32) -> u128 {
        if rhs < 128 { self << rhs } else { 0 }
    }

    pub const fn unbounded_shr(self, rhs: u32) -> u128 {
        if rhs < 128 { self >> rhs } else { 0 }
    }

    pub const fn leading_ones(self) -> u32 {
        (!self).leading_zeros()
    }

    pub const fn trailing_ones(self) -> u32 {
        (!self).trailing_zeros()
    }

    pub const fn swap_bytes(self) -> u128 {
        let mut x = self as u128;
        let mut r: u128 = 0;
        let mut i = 0;
        while i < 16 {
            r = (r << 8) | (x & 0xff);
            x >>= 8;
            i += 1;
        }
        r as u128
    }

    pub const fn reverse_bits(self) -> u128 {
        let mut x = self as u128;
        let mut r: u128 = 0;
        let mut i = 0;
        while i < 128 {
            r = (r << 1) | (x & 1);
            x >>= 1;
            i += 1;
        }
        r as u128
    }

    pub const fn to_be(self) -> u128 {
        self.swap_bytes()
    }

    pub const fn to_le(self) -> u128 {
        self
    }

    pub const fn from_be(x: u128) -> u128 {
        x.swap_bytes()
    }

    pub const fn from_le(x: u128) -> u128 {
        x
    }

    pub const fn to_le_bytes(self) -> [u8; 16] {
        let x = self as u128;
        [x as u8, (x >> 8) as u8, (x >> 16) as u8, (x >> 24) as u8, (x >> 32) as u8, (x >> 40) as u8, (x >> 48) as u8, (x >> 56) as u8, (x >> 64) as u8, (x >> 72) as u8, (x >> 80) as u8, (x >> 88) as u8, (x >> 96) as u8, (x >> 104) as u8, (x >> 112) as u8, (x >> 120) as u8]
    }

    pub const fn to_be_bytes(self) -> [u8; 16] {
        self.swap_bytes().to_le_bytes()
    }

    pub const fn to_ne_bytes(self) -> [u8; 16] {
        self.to_le_bytes()
    }

    pub const fn from_le_bytes(bytes: [u8; 16]) -> u128 {
        (bytes[0] as u128 | (bytes[1] as u128) << 8 | (bytes[2] as u128) << 16 | (bytes[3] as u128) << 24 | (bytes[4] as u128) << 32 | (bytes[5] as u128) << 40 | (bytes[6] as u128) << 48 | (bytes[7] as u128) << 56 | (bytes[8] as u128) << 64 | (bytes[9] as u128) << 72 | (bytes[10] as u128) << 80 | (bytes[11] as u128) << 88 | (bytes[12] as u128) << 96 | (bytes[13] as u128) << 104 | (bytes[14] as u128) << 112 | (bytes[15] as u128) << 120) as u128
    }

    pub const fn from_be_bytes(bytes: [u8; 16]) -> u128 {
        u128::from_le_bytes(bytes).swap_bytes()
    }

    pub const fn from_ne_bytes(bytes: [u8; 16]) -> u128 {
        u128::from_le_bytes(bytes)
    }
}

impl usize {
    pub const fn checked_add(self, rhs: usize) -> Option<usize> {
        let (r, o) = self.overflowing_add(rhs);
        if o { None } else { Some(r) }
    }

    pub const fn checked_sub(self, rhs: usize) -> Option<usize> {
        let (r, o) = self.overflowing_sub(rhs);
        if o { None } else { Some(r) }
    }

    pub const fn checked_mul(self, rhs: usize) -> Option<usize> {
        let (r, o) = self.overflowing_mul(rhs);
        if o { None } else { Some(r) }
    }

    pub const fn checked_div(self, rhs: usize) -> Option<usize> {
        if rhs == 0 {
            None
        } else {
            Some(self / rhs)
        }
    }

    pub const fn checked_rem(self, rhs: usize) -> Option<usize> {
        if rhs == 0 {
            None
        } else {
            Some(self % rhs)
        }
    }

    pub const fn checked_div_euclid(self, rhs: usize) -> Option<usize> {
        if rhs == 0 {
            None
        } else {
            Some(self.div_euclid(rhs))
        }
    }

    pub const fn checked_rem_euclid(self, rhs: usize) -> Option<usize> {
        if rhs == 0 {
            None
        } else {
            Some(self.rem_euclid(rhs))
        }
    }

    pub const fn checked_neg(self) -> Option<usize> {
        let (r, o) = self.overflowing_neg();
        if o { None } else { Some(r) }
    }

    pub const fn checked_shl(self, rhs: u32) -> Option<usize> {
        if rhs < 64 { Some(self << rhs) } else { None }
    }

    pub const fn checked_shr(self, rhs: u32) -> Option<usize> {
        if rhs < 64 { Some(self >> rhs) } else { None }
    }

    pub const fn checked_pow(self, exp: u32) -> Option<usize> {
        if exp == 0 {
            return Some(1);
        }
        let mut base = self;
        let mut exp = exp;
        let mut acc: usize = 1;
        loop {
            if exp & 1 == 1 {
                acc = match acc.checked_mul(base) {
                    Some(x) => x,
                    None => return None,
                };
                if exp == 1 {
                    return Some(acc);
                }
            }
            exp /= 2;
            base = match base.checked_mul(base) {
                Some(x) => x,
                None => return None,
            };
        }
    }

    pub const fn checked_ilog2(self) -> Option<u32> {
        if self <= 0 { None } else { Some(64 - 1 - self.leading_zeros()) }
    }

    pub const fn checked_ilog10(self) -> Option<u32> {
        self.checked_ilog(10)
    }

    pub const fn checked_ilog(self, base: usize) -> Option<u32> {
        if self <= 0 || base <= 1 {
            None
        } else {
            let mut n = 0;
            let mut r = self;
            while r >= base {
                r /= base;
                n += 1;
            }
            Some(n)
        }
    }

    #[track_caller]
    pub const fn ilog2(self) -> u32 {
        match self.checked_ilog2() {
            Some(n) => n,
            None => crate::panicking::panic("argument of integer logarithm must be positive"),
        }
    }

    #[track_caller]
    pub const fn ilog10(self) -> u32 {
        match self.checked_ilog10() {
            Some(n) => n,
            None => crate::panicking::panic("argument of integer logarithm must be positive"),
        }
    }

    #[track_caller]
    pub const fn ilog(self, base: usize) -> u32 {
        if base < 2 {
            crate::panicking::panic("base of integer logarithm must be at least 2");
        }
        match self.checked_ilog(base) {
            Some(n) => n,
            None => crate::panicking::panic("argument of integer logarithm must be positive"),
        }
    }

    pub const fn wrapping_div(self, rhs: usize) -> usize {
        self.overflowing_div(rhs).0
    }

    pub const fn wrapping_rem(self, rhs: usize) -> usize {
        self.overflowing_rem(rhs).0
    }

    pub const fn wrapping_div_euclid(self, rhs: usize) -> usize {
        self.overflowing_div_euclid(rhs).0
    }

    pub const fn wrapping_rem_euclid(self, rhs: usize) -> usize {
        self.overflowing_rem_euclid(rhs).0
    }

    pub const fn wrapping_shl(self, rhs: u32) -> usize {
        self << (rhs & 63)
    }

    pub const fn wrapping_shr(self, rhs: u32) -> usize {
        self >> (rhs & 63)
    }

    pub const fn wrapping_pow(self, exp: u32) -> usize {
        if exp == 0 {
            return 1;
        }
        let mut base = self;
        let mut exp = exp;
        let mut acc: usize = 1;
        loop {
            if exp & 1 == 1 {
                acc = acc.wrapping_mul(base);
                if exp == 1 {
                    return acc;
                }
            }
            exp /= 2;
            base = base.wrapping_mul(base);
        }
    }

    pub const fn overflowing_add(self, rhs: usize) -> (usize, bool) {
        let r = self.wrapping_add(rhs);
        (r, r < self)
    }

    pub const fn overflowing_sub(self, rhs: usize) -> (usize, bool) {
        let r = self.wrapping_sub(rhs);
        (r, r > self)
    }

    pub const fn overflowing_mul(self, rhs: usize) -> (usize, bool) {
        let r = self.wrapping_mul(rhs);

        (r, self != 0 && r / self != rhs)
    }

    pub const fn overflowing_div(self, rhs: usize) -> (usize, bool) {
        (self / rhs, false)
    }

    pub const fn overflowing_rem(self, rhs: usize) -> (usize, bool) {
        (self % rhs, false)
    }

    pub const fn overflowing_div_euclid(self, rhs: usize) -> (usize, bool) {
        (self / rhs, false)
    }

    pub const fn overflowing_rem_euclid(self, rhs: usize) -> (usize, bool) {
        (self % rhs, false)
    }

    pub const fn overflowing_neg(self) -> (usize, bool) {
        (self.wrapping_neg(), self != 0)
    }

    pub const fn overflowing_shl(self, rhs: u32) -> (usize, bool) {
        (self.wrapping_shl(rhs), rhs >= 64)
    }

    pub const fn overflowing_shr(self, rhs: u32) -> (usize, bool) {
        (self.wrapping_shr(rhs), rhs >= 64)
    }

    pub const fn overflowing_pow(self, exp: u32) -> (usize, bool) {
        if exp == 0 {
            return (1, false);
        }
        let mut base = self;
        let mut exp = exp;
        let mut acc: usize = 1;
        let mut overflown = false;
        loop {
            if exp & 1 == 1 {
                let (r, o) = acc.overflowing_mul(base);
                if exp == 1 {
                    return (r, o || overflown);
                }
                acc = r;
                overflown = overflown || o;
            }
            exp /= 2;
            let (r, o) = base.overflowing_mul(base);
            base = r;
            overflown = overflown || o;
        }
    }

    pub const fn saturating_add(self, rhs: usize) -> usize {
        match self.checked_add(rhs) {
            Some(x) => x,
            None => usize::MAX,
        }
    }

    pub const fn saturating_sub(self, rhs: usize) -> usize {
        match self.checked_sub(rhs) {
            Some(x) => x,
            None => usize::MIN,
        }
    }

    pub const fn saturating_mul(self, rhs: usize) -> usize {
        match self.checked_mul(rhs) {
            Some(x) => x,
            None => usize::MAX,
        }
    }

    pub const fn saturating_div(self, rhs: usize) -> usize {
        self / rhs
    }

    pub const fn saturating_pow(self, exp: u32) -> usize {
        match self.checked_pow(exp) {
            Some(x) => x,
            None => usize::MAX,
        }
    }

    pub const fn div_euclid(self, rhs: usize) -> usize {
        self / rhs
    }

    pub const fn rem_euclid(self, rhs: usize) -> usize {
        self % rhs
    }

    pub const fn div_ceil(self, rhs: usize) -> usize {
        let d = self / rhs;
        let r = self % rhs;
        if r > 0 { d + 1 } else { d }
    }

    #[rustc_inherit_overflow_checks]
    pub const fn next_multiple_of(self, rhs: usize) -> usize {
        match self % rhs {
            0 => self,
            r => self + (rhs - r),
        }
    }

    pub const fn checked_next_multiple_of(self, rhs: usize) -> Option<usize> {
        match self.checked_rem(rhs) {
            None => None,
            Some(0) => Some(self),
            Some(r) => self.checked_add(rhs - r),
        }
    }

    const fn one_less_than_next_power_of_two(self) -> usize {
        if self <= 1 {
            return 0;
        }
        let p = self - 1;
        usize::MAX >> p.leading_zeros()
    }

    #[rustc_inherit_overflow_checks]
    pub const fn next_power_of_two(self) -> usize {
        self.one_less_than_next_power_of_two() + 1
    }

    pub const fn checked_next_power_of_two(self) -> Option<usize> {
        self.one_less_than_next_power_of_two().checked_add(1)
    }

    pub const fn isqrt(self) -> usize {
        if self < 2 {
            return self;
        }
        let mut op = self;
        let mut res: usize = 0;
        let mut one: usize = 1 << 62;
        while one > op {
            one >>= 2;
        }
        while one != 0 {
            if op >= res + one {
                op -= res + one;
                res = (res >> 1) + one;
            } else {
                res >>= 1;
            }
            one >>= 2;
        }
        res
    }

    pub const fn midpoint(self, rhs: usize) -> usize {
        (self & rhs) + ((self ^ rhs) >> 1)
    }

    pub const fn cast_signed(self) -> isize {
        self as isize
    }

    pub const fn checked_add_signed(self, rhs: isize) -> Option<usize> {
        let (r, o) = self.overflowing_add_signed(rhs);
        if o { None } else { Some(r) }
    }

    pub const fn wrapping_add_signed(self, rhs: isize) -> usize {
        self.wrapping_add(rhs as usize)
    }

    pub const fn saturating_add_signed(self, rhs: isize) -> usize {
        let (res, overflow) = self.overflowing_add(rhs as usize);
        if overflow == (rhs < 0) {
            res
        } else if overflow {
            usize::MAX
        } else {
            0
        }
    }

    pub const fn overflowing_add_signed(self, rhs: isize) -> (usize, bool) {
        let (res, overflowed) = self.overflowing_add(rhs as usize);
        (res, overflowed != (rhs < 0))
    }

    pub const fn unbounded_shl(self, rhs: u32) -> usize {
        if rhs < 64 { self << rhs } else { 0 }
    }

    pub const fn unbounded_shr(self, rhs: u32) -> usize {
        if rhs < 64 { self >> rhs } else { 0 }
    }

    pub const fn leading_ones(self) -> u32 {
        (!self).leading_zeros()
    }

    pub const fn trailing_ones(self) -> u32 {
        (!self).trailing_zeros()
    }

    pub const fn swap_bytes(self) -> usize {
        let mut x = self as usize;
        let mut r: usize = 0;
        let mut i = 0;
        while i < 8 {
            r = (r << 8) | (x & 0xff);
            x >>= 8;
            i += 1;
        }
        r as usize
    }

    pub const fn reverse_bits(self) -> usize {
        let mut x = self as usize;
        let mut r: usize = 0;
        let mut i = 0;
        while i < 64 {
            r = (r << 1) | (x & 1);
            x >>= 1;
            i += 1;
        }
        r as usize
    }

    pub const fn to_be(self) -> usize {
        self.swap_bytes()
    }

    pub const fn to_le(self) -> usize {
        self
    }

    pub const fn from_be(x: usize) -> usize {
        x.swap_bytes()
    }

    pub const fn from_le(x: usize) -> usize {
        x
    }

    pub const fn to_le_bytes(self) -> [u8; 8] {
        let x = self as usize;
        [x as u8, (x >> 8) as u8, (x >> 16) as u8, (x >> 24) as u8, (x >> 32) as u8, (x >> 40) as u8, (x >> 48) as u8, (x >> 56) as u8]
    }

    pub const fn to_be_bytes(self) -> [u8; 8] {
        self.swap_bytes().to_le_bytes()
    }

    pub const fn to_ne_bytes(self) -> [u8; 8] {
        self.to_le_bytes()
    }

    pub const fn from_le_bytes(bytes: [u8; 8]) -> usize {
        (bytes[0] as usize | (bytes[1] as usize) << 8 | (bytes[2] as usize) << 16 | (bytes[3] as usize) << 24 | (bytes[4] as usize) << 32 | (bytes[5] as usize) << 40 | (bytes[6] as usize) << 48 | (bytes[7] as usize) << 56) as usize
    }

    pub const fn from_be_bytes(bytes: [u8; 8]) -> usize {
        usize::from_le_bytes(bytes).swap_bytes()
    }

    pub const fn from_ne_bytes(bytes: [u8; 8]) -> usize {
        usize::from_le_bytes(bytes)
    }
}

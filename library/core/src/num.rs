//! The integer types' constants and methods.
//!
//! Written in flux, which compiles with overflow checks off, so
//! arithmetic wraps, during constant evaluation too, except in methods
//! marked `#[rustc_inherit_overflow_checks]`, which check where their
//! caller does (in constants), as Rust's `core` does. Methods that are
//! trait methods in Rust (`min`, `max`) are inherent here, until flux has
//! traits; they are not `const`, as in Rust.

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

    pub fn min(self, other: i8) -> i8 {
        if other < self { other } else { self }
    }

    pub fn max(self, other: i8) -> i8 {
        if other < self { self } else { other }
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

    pub fn min(self, other: i16) -> i16 {
        if other < self { other } else { self }
    }

    pub fn max(self, other: i16) -> i16 {
        if other < self { self } else { other }
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

    pub fn min(self, other: i32) -> i32 {
        if other < self { other } else { self }
    }

    pub fn max(self, other: i32) -> i32 {
        if other < self { self } else { other }
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

    pub fn min(self, other: i64) -> i64 {
        if other < self { other } else { self }
    }

    pub fn max(self, other: i64) -> i64 {
        if other < self { self } else { other }
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

    pub fn min(self, other: i128) -> i128 {
        if other < self { other } else { self }
    }

    pub fn max(self, other: i128) -> i128 {
        if other < self { self } else { other }
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

    pub fn min(self, other: isize) -> isize {
        if other < self { other } else { self }
    }

    pub fn max(self, other: isize) -> isize {
        if other < self { self } else { other }
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

    pub fn min(self, other: u8) -> u8 {
        if other < self { other } else { self }
    }

    pub fn max(self, other: u8) -> u8 {
        if other < self { self } else { other }
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

    pub fn min(self, other: u16) -> u16 {
        if other < self { other } else { self }
    }

    pub fn max(self, other: u16) -> u16 {
        if other < self { self } else { other }
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

    pub fn min(self, other: u32) -> u32 {
        if other < self { other } else { self }
    }

    pub fn max(self, other: u32) -> u32 {
        if other < self { self } else { other }
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

    pub fn min(self, other: u64) -> u64 {
        if other < self { other } else { self }
    }

    pub fn max(self, other: u64) -> u64 {
        if other < self { self } else { other }
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

    pub fn min(self, other: u128) -> u128 {
        if other < self { other } else { self }
    }

    pub fn max(self, other: u128) -> u128 {
        if other < self { self } else { other }
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

    pub fn min(self, other: usize) -> usize {
        if other < self { other } else { self }
    }

    pub fn max(self, other: usize) -> usize {
        if other < self { self } else { other }
    }

    pub const fn is_power_of_two(self) -> bool {
        self != 0 && self & (self - 1) == 0
    }
}

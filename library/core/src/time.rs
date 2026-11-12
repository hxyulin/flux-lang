//! Nonnegative time spans available without an allocator or operating system.
use crate::ops::{Add, AddAssign, Sub, SubAssign};

const NANOS_PER_SEC: u32 = 1_000_000_000;

/// A nonnegative span of time with nanosecond precision.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Duration {
    secs: u64,
    nanos: u32,
}

impl Duration {
    pub const ZERO: Self = Self::new(0, 0);
    pub const MAX: Self = Self::new(u64::MAX, NANOS_PER_SEC - 1);

    /// Creates a duration, carrying whole seconds from `nanos`.
    ///
    /// # Panics
    /// Panics if the normalized seconds exceed `u64::MAX`.
    pub const fn new(secs: u64, nanos: u32) -> Self {
        let carry = (nanos / NANOS_PER_SEC) as u64;
        let secs = match secs.checked_add(carry) {
            Some(secs) => secs,
            None => panic!("overflow in Duration::new"),
        };
        Self {
            secs,
            nanos: nanos % NANOS_PER_SEC,
        }
    }

    pub const fn from_secs(secs: u64) -> Self {
        Self { secs, nanos: 0 }
    }
    pub const fn from_millis(millis: u64) -> Self {
        Self::new(millis / 1_000, ((millis % 1_000) * 1_000_000) as u32)
    }
    pub const fn from_micros(micros: u64) -> Self {
        Self::new(micros / 1_000_000, ((micros % 1_000_000) * 1_000) as u32)
    }
    pub const fn from_nanos(nanos: u64) -> Self {
        Self::new(
            nanos / NANOS_PER_SEC as u64,
            (nanos % NANOS_PER_SEC as u64) as u32,
        )
    }
    pub const fn as_secs(&self) -> u64 {
        self.secs
    }
    pub const fn subsec_millis(&self) -> u32 {
        self.nanos / 1_000_000
    }
    pub const fn subsec_micros(&self) -> u32 {
        self.nanos / 1_000
    }
    pub const fn subsec_nanos(&self) -> u32 {
        self.nanos
    }
    pub const fn as_nanos(&self) -> u128 {
        self.secs as u128 * NANOS_PER_SEC as u128 + self.nanos as u128
    }
    pub const fn as_micros(&self) -> u128 {
        self.as_nanos() / 1_000
    }
    pub const fn as_millis(&self) -> u128 {
        self.as_nanos() / 1_000_000
    }
    pub fn as_secs_f64(&self) -> f64 {
        self.secs as f64 + self.nanos as f64 / NANOS_PER_SEC as f64
    }
    pub const fn is_zero(&self) -> bool {
        self.secs == 0 && self.nanos == 0
    }

    pub const fn checked_add(self, rhs: Self) -> Option<Self> {
        let nanos = self.nanos + rhs.nanos;
        let carry = (nanos / NANOS_PER_SEC) as u64;
        let secs = match self.secs.checked_add(rhs.secs) {
            Some(secs) => secs,
            None => return None,
        };
        match secs.checked_add(carry) {
            Some(secs) => Some(Self {
                secs,
                nanos: nanos % NANOS_PER_SEC,
            }),
            None => None,
        }
    }

    pub const fn checked_sub(self, rhs: Self) -> Option<Self> {
        let secs = match self.secs.checked_sub(rhs.secs) {
            Some(secs) => secs,
            None => return None,
        };
        if self.nanos >= rhs.nanos {
            Some(Self {
                secs,
                nanos: self.nanos - rhs.nanos,
            })
        } else {
            match secs.checked_sub(1) {
                Some(secs) => Some(Self {
                    secs,
                    nanos: self.nanos + NANOS_PER_SEC - rhs.nanos,
                }),
                None => None,
            }
        }
    }
}

impl Duration {
    /// `self + rhs`, or [`Duration::MAX`] on overflow.
    pub const fn saturating_add(self, rhs: Self) -> Self {
        match self.checked_add(rhs) {
            Some(sum) => sum,
            None => Self::MAX,
        }
    }

    /// `self - rhs`, or [`Duration::ZERO`] when `rhs` is larger.
    pub const fn saturating_sub(self, rhs: Self) -> Self {
        match self.checked_sub(rhs) {
            Some(difference) => difference,
            None => Self::ZERO,
        }
    }
}

impl Default for Duration {
    fn default() -> Self {
        Self::ZERO
    }
}
impl Add for Duration {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        self.checked_add(rhs)
            .expect("overflow when adding durations")
    }
}
impl Sub for Duration {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        self.checked_sub(rhs)
            .expect("overflow when subtracting durations")
    }
}

impl AddAssign for Duration {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}
impl SubAssign for Duration {
    fn sub_assign(&mut self, rhs: Self) {
        *self = *self - rhs;
    }
}

impl crate::fmt::Debug for Duration {
    fn fmt(&self, f: &mut crate::fmt::Formatter<'_>) -> crate::fmt::Result {
        let (unit, digits, suffix) = if self.secs != 0 {
            (1_000_000_000u128, 9usize, "s")
        } else if self.nanos >= 1_000_000 {
            (1_000_000, 6, "ms")
        } else if self.nanos >= 1_000 {
            (1_000, 3, "µs")
        } else {
            (1, 0, "ns")
        };
        let precision = f.precision();
        let retained = precision.unwrap_or(digits).min(digits);
        let rounding = 10u128.pow((digits - retained) as u32);
        let amount = self.as_nanos();
        let mut rounded = amount / rounding;
        let remainder = amount % rounding;
        if remainder > rounding / 2
            || (rounding > 1 && remainder == rounding / 2 && !rounded.is_multiple_of(2))
        {
            rounded += 1;
        }
        let decimal_unit = unit / rounding;
        let whole = rounded / decimal_unit;
        let mut fraction = rounded % decimal_unit;
        let mut decimal_digits = retained;
        if precision.is_none() {
            while decimal_digits > 0 && fraction.is_multiple_of(10) {
                fraction /= 10;
                decimal_digits -= 1;
            }
        }
        let output_digits = precision.unwrap_or(decimal_digits);
        let mut whole_digits = 1usize;
        let mut remaining = whole;
        while remaining >= 10 {
            whole_digits += 1;
            remaining /= 10;
        }
        let character_count = whole_digits
            + if f.sign_plus() { 1 } else { 0 }
            + if output_digits > 0 {
                output_digits + 1
            } else {
                0
            }
            + suffix.chars().count();
        let padding = f.width().unwrap_or(0).saturating_sub(character_count);
        let left = match f.align().unwrap_or(crate::fmt::Alignment::Left) {
            crate::fmt::Alignment::Left => 0,
            crate::fmt::Alignment::Right => padding,
            crate::fmt::Alignment::Center => padding / 2,
        };
        let mut fill = [0u8; 4];
        let fill = f.fill().encode_utf8(&mut fill);
        for _ in 0..left {
            f.write_str(fill)?;
        }
        if f.sign_plus() {
            f.write_str("+")?;
        }
        write!(f, "{}", whole)?;
        if output_digits > 0 {
            f.write_str(".")?;
            let significant = if precision.is_some() {
                retained
            } else {
                decimal_digits
            };
            let mut fractional = [b'0'; 9];
            let mut divisor = 10u128.pow(significant.saturating_sub(1) as u32);
            for digit in &mut fractional[..significant] {
                *digit = b'0' + (fraction / divisor) as u8;
                fraction %= divisor;
                divisor /= 10;
            }
            let fractional = crate::str::from_utf8(&fractional[..significant])
                .expect("duration fractional digits are ASCII");
            f.write_str(fractional)?;
            for _ in significant..output_digits {
                f.write_str("0")?;
            }
        }
        f.write_str(suffix)?;
        for _ in left..padding {
            f.write_str(fill)?;
        }
        Ok(())
    }
}

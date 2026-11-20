//! Nonnegative time spans available without an allocator or operating system.
use crate::ops::{Add, AddAssign, Div, Sub, SubAssign};

const NANOS_PER_SEC: u32 = 1_000_000_000;

/// A nonnegative span of time with nanosecond precision.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Duration {
    secs: u64,
    nanos: u32,
}

/// The error returned when floating point seconds cannot form a duration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TryFromFloatSecsError {
    kind: TryFromFloatSecsErrorKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TryFromFloatSecsErrorKind {
    Negative,
    OverflowOrNan,
}

impl crate::fmt::Display for TryFromFloatSecsError {
    fn fmt(&self, f: &mut crate::fmt::Formatter<'_>) -> crate::fmt::Result {
        f.write_str(match self.kind {
            TryFromFloatSecsErrorKind::Negative => {
                "cannot convert float seconds to Duration: value is negative"
            }
            TryFromFloatSecsErrorKind::OverflowOrNan => {
                "cannot convert float seconds to Duration: value is either too big or NaN"
            }
        })
    }
}

impl crate::error::Error for TryFromFloatSecsError {}

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

    /// Converts finite, nonnegative seconds, rounding to the nearest
    /// nanosecond with ties rounded to even.
    pub fn try_from_secs_f64(secs: f64) -> Result<Self, TryFromFloatSecsError> {
        if secs < 0.0 {
            return Err(TryFromFloatSecsError {
                kind: TryFromFloatSecsErrorKind::Negative,
            });
        }

        const IMPLICIT_BIT: u64 = 1 << 52;
        let bits = secs.to_bits();
        let exponent = ((bits >> 52) & 0x7ff) as i32 - 1023;
        if exponent >= 64 {
            return Err(TryFromFloatSecsError {
                kind: TryFromFloatSecsErrorKind::OverflowOrNan,
            });
        }
        // This also covers zero and subnormals. The largest value with an
        // exponent below -31 is less than half a nanosecond.
        if exponent < -31 {
            return Ok(Self::ZERO);
        }
        let mantissa = (bits & (IMPLICIT_BIT - 1)) | IMPLICIT_BIT;
        if exponent >= 52 {
            return Ok(Self::from_secs(mantissa << (exponent - 52) as u32));
        }

        // The fractional part is an exact integer divided by a power of
        // two. Scale the numerator before rounding to avoid double rounding.
        let shift = (52 - exponent) as u32;
        let mask = (1u128 << shift) - 1;
        let mantissa = mantissa as u128;
        let seconds = (mantissa >> shift) as u64;
        let scaled = (mantissa & mask) * NANOS_PER_SEC as u128;
        let nanos = (scaled >> shift) as u32;
        let remainder = scaled & mask;
        let half = 1u128 << (shift - 1);
        let round_up = remainder > half || (remainder == half && nanos & 1 != 0);
        Ok(Self::new(seconds, nanos + round_up as u32))
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
    /// Returns the elapsed seconds and fractional seconds as `f32`.
    pub const fn as_secs_f32(&self) -> f32 {
        self.secs as f32 + self.nanos as f32 / NANOS_PER_SEC as f32
    }
    pub const fn is_zero(&self) -> bool {
        self.secs == 0 && self.nanos == 0
    }

    /// Returns the ratio of two durations in floating point.
    pub const fn div_duration_f64(self, rhs: Self) -> f64 {
        let scale = NANOS_PER_SEC as f64;
        let lhs_nanos = self.secs as f64 * scale + self.nanos as f64;
        let rhs_nanos = rhs.secs as f64 * scale + rhs.nanos as f64;
        lhs_nanos / rhs_nanos
    }

    /// Divides by a scalar, truncating subnanosecond fractions.
    /// Returns `None` if the divisor is zero.
    pub const fn checked_div(self, rhs: u32) -> Option<Self> {
        if rhs == 0 {
            return None;
        }
        let rhs = rhs as u64;
        let remaining_nanos = (self.secs % rhs) * NANOS_PER_SEC as u64 + self.nanos as u64;
        Some(Self {
            secs: self.secs / rhs,
            nanos: (remaining_nanos / rhs) as u32,
        })
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

    /// `self * rhs`, or [`Duration::MAX`] on overflow.
    pub const fn saturating_mul(self, rhs: u32) -> Self {
        let nanos = self.nanos as u64 * rhs as u64;
        let Some(secs) = self.secs.checked_mul(rhs as u64) else {
            return Self::MAX;
        };
        let scale = NANOS_PER_SEC as u64;
        match secs.checked_add(nanos / scale) {
            Some(secs) => Self {
                secs,
                nanos: (nanos % scale) as u32,
            },
            None => Self::MAX,
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

impl Div<u32> for Duration {
    type Output = Self;

    #[track_caller]
    fn div(self, rhs: u32) -> Self {
        self.checked_div(rhs)
            .expect("divide by zero error when dividing duration by scalar")
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

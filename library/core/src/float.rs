//! IEEE binary floating-point primitives.
use crate::cmp::Ordering;

impl f32 {
    pub const RADIX: u32 = 2;
    pub const MANTISSA_DIGITS: u32 = 24;
    pub const DIGITS: u32 = 6;
    pub const MIN_EXP: i32 = -125;
    pub const MAX_EXP: i32 = 128;
    pub const MIN_10_EXP: i32 = -37;
    pub const MAX_10_EXP: i32 = 38;
}

impl f64 {
    pub const RADIX: u32 = 2;
    pub const MANTISSA_DIGITS: u32 = 53;
    pub const DIGITS: u32 = 15;
    pub const MIN_EXP: i32 = -1021;
    pub const MAX_EXP: i32 = 1024;
    pub const MIN_10_EXP: i32 = -307;
    pub const MAX_10_EXP: i32 = 308;
}

macro_rules! float_impl {
    ($t:ty, $u:ty, $sign:expr, $exp:expr, $frac:expr, $max:expr, $minpos:expr, $epsilon:expr) => {
        impl $t {
            /// Converts a finite float to an integer by truncating toward zero.
            ///
            /// # Safety
            /// The truncated value must be representable by `Int`.
            pub unsafe fn to_int_unchecked<Int>(self) -> Int
            where Self: crate::convert::FloatToInt<Int> {
                <Self as crate::convert::FloatToInt<Int>>::to_int(self)
            }
            pub const fn classify(self)->crate::num::FpCategory {
                let bits=self.to_bits();
                let exponent=bits & $exp;
                let fraction=bits & $frac;
                if exponent==$exp {
                    if fraction==0 {crate::num::FpCategory::Infinite}else{crate::num::FpCategory::Nan}
                }else if exponent==0 {
                    if fraction==0 {crate::num::FpCategory::Zero}else{crate::num::FpCategory::Subnormal}
                }else {crate::num::FpCategory::Normal}
            }
            pub const NAN: Self = Self::from_bits($exp | (($frac + 1) >> 1));
            pub const INFINITY: Self = Self::from_bits($exp);
            pub const NEG_INFINITY: Self = Self::from_bits($sign | $exp);
            pub const MAX: Self = Self::from_bits($max);
            pub const MIN: Self = Self::from_bits($sign | $max);
            pub const MIN_POSITIVE: Self = Self::from_bits($minpos);
            pub const EPSILON: Self = Self::from_bits($epsilon);
            // SAFETY: the integer and float have equal size; every float bit pattern is valid.
            pub const fn from_bits(bits: $u) -> Self { unsafe { crate::mem::transmute(bits) } }
            // SAFETY: the integer and float have equal size; every integer bit pattern is valid.
            pub const fn to_bits(self) -> $u { unsafe { crate::mem::transmute(self) } }
            pub const fn is_nan(self) -> bool { (self.to_bits() & !$sign) > $exp }
            pub const fn is_infinite(self) -> bool { (self.to_bits() & !$sign) == $exp }
            pub const fn is_finite(self) -> bool { (self.to_bits() & $exp) != $exp }
            pub const fn is_sign_negative(self) -> bool { (self.to_bits() & $sign) != 0 }
            pub const fn is_sign_positive(self) -> bool { !self.is_sign_negative() }
            pub const fn is_normal(self) -> bool { let e = self.to_bits() & $exp; e != 0 && e != $exp }
            pub const fn is_subnormal(self) -> bool { (self.to_bits() & $exp) == 0 && (self.to_bits() & $frac) != 0 }
            /// Returns the next representable value toward positive infinity.
            pub const fn next_up(self) -> Self {
                let bits = self.to_bits();
                let magnitude = bits & !$sign;
                if magnitude > $exp || bits == $exp {
                    self
                } else if magnitude == 0 {
                    Self::from_bits(1)
                } else if bits & $sign != 0 {
                    Self::from_bits(bits - 1)
                } else {
                    Self::from_bits(bits + 1)
                }
            }
            /// Returns the next representable value toward negative infinity.
            pub const fn next_down(self) -> Self {
                let bits = self.to_bits();
                let magnitude = bits & !$sign;
                if magnitude > $exp || bits == ($sign | $exp) {
                    self
                } else if magnitude == 0 {
                    Self::from_bits($sign | 1)
                } else if bits & $sign != 0 {
                    Self::from_bits(bits + 1)
                } else {
                    Self::from_bits(bits - 1)
                }
            }
            /// Copies the sign bit, including for zeros and NaNs.
            pub const fn copysign(self, sign: Self) -> Self {
                Self::from_bits((self.to_bits() & !$sign) | (sign.to_bits() & $sign))
            }
            pub const fn recip(self) -> Self { 1.0 / self }
            pub const fn abs(self) -> Self { Self::from_bits(self.to_bits() & !$sign) }
            /// Ignore one NaN; for equal values (including signed zeros),
            /// return an input as Rust's minimumNumber contract allows.
            pub const fn min(self, other: Self) -> Self {
                if self.is_nan() { if other.is_nan() {self+other} else {other} }
                else if other.is_nan() || self<other {self} else {other}
            }
            pub const fn max(self, other: Self) -> Self {
                if self.is_nan() { if other.is_nan() {self+other} else {other} }
                else if other.is_nan() || self>other {self} else {other}
            }

        }
        impl crate::marker::Copy for $t {}
        impl crate::clone::Clone for $t { fn clone(&self) -> Self { *self } }
        impl crate::default::Default for $t { fn default() -> Self { 0.0 } }
        impl crate::cmp::PartialEq for $t {
            fn eq(&self, other: &Self) -> bool { *self == *other }
            fn ne(&self, other: &Self) -> bool { *self != *other }
        }
        impl crate::cmp::PartialOrd for $t {
            fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
                if *self < *other { Some(Ordering::Less) } else if *self > *other { Some(Ordering::Greater) }
                else if *self == *other { Some(Ordering::Equal) } else { None }
            }
            fn lt(&self, other: &Self) -> bool { *self < *other }
            fn le(&self, other: &Self) -> bool { *self <= *other }
            fn gt(&self, other: &Self) -> bool { *self > *other }
            fn ge(&self, other: &Self) -> bool { *self >= *other }
        }
        impl crate::ops::Neg for $t { type Output = Self; fn neg(self) -> Self { -self } }
        impl crate::ops::Neg for &$t { type Output = $t; fn neg(self) -> $t { -*self } }
        impl crate::ops::Add<$t> for &$t { type Output = $t; fn add(self, rhs: $t) -> $t { *self + rhs } }
        impl crate::ops::Add<&$t> for $t { type Output = $t; fn add(self, rhs: &$t) -> $t { self + *rhs } }
        impl crate::ops::Add<&$t> for &$t { type Output = $t; fn add(self, rhs: &$t) -> $t { *self + *rhs } }
        impl crate::ops::Add for $t { type Output = Self; fn add(self, rhs: Self) -> Self { self + rhs } }
        impl crate::ops::AddAssign<&$t> for $t { fn add_assign(&mut self, rhs: &$t) { *self = *self + *rhs; } }
        impl crate::ops::AddAssign for $t { fn add_assign(&mut self, rhs: Self) { *self = *self + rhs; } }
        impl crate::ops::Sub<$t> for &$t { type Output = $t; fn sub(self, rhs: $t) -> $t { *self - rhs } }
        impl crate::ops::Sub<&$t> for $t { type Output = $t; fn sub(self, rhs: &$t) -> $t { self - *rhs } }
        impl crate::ops::Sub<&$t> for &$t { type Output = $t; fn sub(self, rhs: &$t) -> $t { *self - *rhs } }
        impl crate::ops::Sub for $t { type Output = Self; fn sub(self, rhs: Self) -> Self { self - rhs } }
        impl crate::ops::SubAssign<&$t> for $t { fn sub_assign(&mut self, rhs: &$t) { *self = *self - *rhs; } }
        impl crate::ops::SubAssign for $t { fn sub_assign(&mut self, rhs: Self) { *self = *self - rhs; } }
        impl crate::ops::Mul<$t> for &$t { type Output = $t; fn mul(self, rhs: $t) -> $t { *self * rhs } }
        impl crate::ops::Mul<&$t> for $t { type Output = $t; fn mul(self, rhs: &$t) -> $t { self * *rhs } }
        impl crate::ops::Mul<&$t> for &$t { type Output = $t; fn mul(self, rhs: &$t) -> $t { *self * *rhs } }
        impl crate::ops::Mul for $t { type Output = Self; fn mul(self, rhs: Self) -> Self { self * rhs } }
        impl crate::ops::MulAssign<&$t> for $t { fn mul_assign(&mut self, rhs: &$t) { *self = *self * *rhs; } }
        impl crate::ops::MulAssign for $t { fn mul_assign(&mut self, rhs: Self) { *self = *self * rhs; } }
        impl crate::ops::Div<$t> for &$t { type Output = $t; fn div(self, rhs: $t) -> $t { *self / rhs } }
        impl crate::ops::Div<&$t> for $t { type Output = $t; fn div(self, rhs: &$t) -> $t { self / *rhs } }
        impl crate::ops::Div<&$t> for &$t { type Output = $t; fn div(self, rhs: &$t) -> $t { *self / *rhs } }
        impl crate::ops::Div for $t { type Output = Self; fn div(self, rhs: Self) -> Self { self / rhs } }
        impl crate::ops::DivAssign<&$t> for $t { fn div_assign(&mut self, rhs: &$t) { *self = *self / *rhs; } }
        impl crate::ops::DivAssign for $t { fn div_assign(&mut self, rhs: Self) { *self = *self / rhs; } }
        impl crate::ops::Rem<$t> for &$t { type Output = $t; fn rem(self, rhs: $t) -> $t { *self % rhs } }
        impl crate::ops::Rem<&$t> for $t { type Output = $t; fn rem(self, rhs: &$t) -> $t { self % *rhs } }
        impl crate::ops::Rem<&$t> for &$t { type Output = $t; fn rem(self, rhs: &$t) -> $t { *self % *rhs } }
        impl crate::ops::Rem for $t { type Output = Self; fn rem(self, rhs: Self) -> Self { self % rhs } }
        impl crate::ops::RemAssign<&$t> for $t { fn rem_assign(&mut self, rhs: &$t) { *self = *self % *rhs; } }
        impl crate::ops::RemAssign for $t { fn rem_assign(&mut self, rhs: Self) { *self = *self % rhs; } }
    };
}
float_impl!(f32,u32,0x80000000u32,0x7f800000u32,0x007fffffu32,0x7f7fffffu32,0x00800000u32,0x34000000u32);
float_impl!(f64,u64,0x8000000000000000u64,0x7ff0000000000000u64,0x000fffffffffffffu64,0x7fefffffffffffffu64,0x0010000000000000u64,0x3cb0000000000000u64);

mod decimal;
use self::decimal::DecimalSeq;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FloatErrorKind { Empty, Invalid }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParseFloatError { kind: FloatErrorKind }
impl crate::fmt::Display for ParseFloatError {
    fn fmt(&self, f: &mut crate::fmt::Formatter<'_>) -> crate::fmt::Result {
        f.write_str(match self.kind { FloatErrorKind::Empty => "cannot parse float from empty string", FloatErrorKind::Invalid => "invalid float literal" })
    }
}
impl crate::error::Error for ParseFloatError {}
fn special(bytes: &[u8], text: &[u8]) -> bool {
    if bytes.len() != text.len() { return false; }
    let mut i=0;
    while i<bytes.len() { if bytes[i].to_ascii_lowercase() != text[i] { return false; } i+=1; }
    true
}
fn parse_bits(text: &str, single: bool) -> Result<u64, ParseFloatError> {
    let invalid = ParseFloatError { kind: FloatErrorKind::Invalid };
    let bytes=text.as_bytes();
    if bytes.is_empty() { return Err(ParseFloatError { kind: FloatErrorKind::Empty }); }
    let negative=bytes[0]==b'-';
    let mut i=if negative || bytes[0]==b'+' { 1 } else { 0 };
    let rest=&bytes[i..];
    let sign=if negative { if single { 1u64<<31 } else { 1u64<<63 } } else { 0 };
    let inf=if single { 0x7f800000u64 } else { 0x7ff0000000000000u64 };
    if special(rest,b"inf") || special(rest,b"infinity") { return Ok(sign|inf); }
    if special(rest,b"nan") { return Ok(sign|inf|if single { 1u64<<22 } else { 1u64<<51 }); }
    let mut d=DecimalSeq::default();
    let mut digits=0usize;
    let mut before=0usize;
    let mut leading=0usize;
    let mut dot=false;
    while i<bytes.len() {
        let c=bytes[i];
        if c==b'.' && !dot { dot=true; i+=1; continue; }
        if c<b'0' || c>b'9' { break; }
        digits+=1;
        if !dot { before+=1; }
        if c==b'0' && d.num_digits==0 { leading+=1; }
        else if d.num_digits<DecimalSeq::MAX_DIGITS { d.digits[d.num_digits]=c-b'0'; d.num_digits+=1; }
        else if c!=b'0' { d.truncated=true; }
        i+=1;
    }
    if digits==0 { return Err(invalid); }
    let mut exponent=0i64;
    if i<bytes.len() && (bytes[i]==b'e' || bytes[i]==b'E') {
        i+=1;
        let exp_negative=i<bytes.len() && bytes[i]==b'-';
        if i<bytes.len() && (bytes[i]==b'-' || bytes[i]==b'+') { i+=1; }
        let start=i;
        while i<bytes.len() && bytes[i]>=b'0' && bytes[i]<=b'9' {
            exponent=(exponent*10+(bytes[i]-b'0') as i64).min(1i64<<55); i+=1;
        }
        if start==i { return Err(invalid); }
        if exp_negative { exponent=-exponent; }
    }
    if i!=bytes.len() { return Err(invalid); }
    let point=before as i128-leading as i128+exponent as i128;
    d.decimal_point=point.max(-10000).min(10000) as i32;
    d.trim();
    Ok(sign|decimal::to_bits(d,single))
}
impl crate::str::FromStr for f32 {
    type Err=ParseFloatError;
    fn from_str(text:&str)->Result<Self,ParseFloatError>{ Ok(Self::from_bits(parse_bits(text,true)? as u32)) }
}
impl crate::str::FromStr for f64 {
    type Err=ParseFloatError;
    fn from_str(text:&str)->Result<Self,ParseFloatError>{ Ok(Self::from_bits(parse_bits(text,false)?)) }
}

#[derive(Clone, Copy)]
enum FloatFormat {
    Display,
    Debug,
    LowerExp,
    UpperExp,
}

fn format_float(
    bits: u64,
    single: bool,
    f: &mut crate::fmt::Formatter<'_>,
    style: FloatFormat,
) -> crate::fmt::Result {
    let sign = if single { 1u64 << 31 } else { 1u64 << 63 };
    let positive = bits & sign == 0;
    let magnitude = bits & !sign;
    let infinity = if single {
        0x7f800000u64
    } else {
        0x7ff0000000000000u64
    };
    if magnitude > infinity {
        return f.pad_float_nan();
    }
    if magnitude == infinity {
        return f.pad_integral(positive, "", "inf");
    }
    let debug = matches!(style, FloatFormat::Debug);
    let marker = match style {
        FloatFormat::LowerExp => Some(b'e'),
        FloatFormat::UpperExp => Some(b'E'),
        _ => None,
    };
    if let Some(precision) = f.precision() {
        if let Some(marker) = marker {
            let mut decimal = exact_decimal(magnitude, single);
            round_decimal(&mut decimal, precision.saturating_add(1));
            let exponent = if magnitude == 0 {
                0
            } else {
                decimal.decimal_point - 1
            };
            return scientific_float(
                positive,
                &decimal.digits[..decimal.num_digits],
                exponent,
                precision,
                marker,
                f,
            );
        }
        return fixed_float(bits, single, precision, f);
    }
    if magnitude == 0 {
        if let Some(marker) = marker {
            return scientific_float(positive, &[0], 0, 0, marker, f);
        }
        return f.pad_integral(positive, "", if debug { "0.0" } else { "0" });
    }
    let (mut coefficient, exponent) = decimal::shortest(magnitude, single);
    let mut digits = [0u8; 20];
    let mut start = digits.len();
    while coefficient != 0 {
        start -= 1;
        digits[start] = (coefficient % 10) as u8;
        coefficient /= 10;
    }
    let digits = &digits[start..];
    let scientific = digits.len() as i32 + exponent - 1;
    if let Some(marker) = marker {
        return scientific_float(positive, digits, scientific, digits.len() - 1, marker, f);
    }
    if debug && (scientific < -4 || scientific >= 16) {
        return scientific_float(positive, digits, scientific, digits.len() - 1, b'e', f);
    }
    let point = digits.len() as i32 + exponent;
    let mut buffer = [0u8; 768];
    let mut n = 0;
    if point <= 0 {
        buffer[0] = b'0';
        buffer[1] = b'.';
        n = 2;
        let mut zeros = -point;
        while zeros > 0 {
            buffer[n] = b'0';
            n += 1;
            zeros -= 1;
        }
    }
    for (i, &digit) in digits.iter().enumerate() {
        if point > 0 && i as i32 == point {
            buffer[n] = b'.';
            n += 1;
        }
        buffer[n] = b'0' + digit;
        n += 1;
    }
    if point >= digits.len() as i32 {
        let mut zeros = point - digits.len() as i32;
        while zeros > 0 {
            buffer[n] = b'0';
            n += 1;
            zeros -= 1;
        }
        if debug {
            buffer[n] = b'.';
            n += 1;
            buffer[n] = b'0';
            n += 1;
        }
    }
    // SAFETY: the buffer contains only ASCII digits and punctuation.
    f.pad_integral(positive, "", unsafe {
        crate::str::from_utf8_unchecked(&buffer[..n])
    })
}

fn scientific_float(
    positive: bool,
    digits: &[u8],
    exponent: i32,
    precision: usize,
    marker: u8,
    f: &mut crate::fmt::Formatter<'_>,
) -> crate::fmt::Result {
    // Space for all 768 digits of DecimalSeq and the decimal point.
    let mut buffer = [0u8; 769];
    buffer[0] = b'0' + digits.first().copied().unwrap_or(0);
    let mut len = 1;
    let fractional = precision.min(digits.len().saturating_sub(1));
    if precision != 0 {
        buffer[len] = b'.';
        len += 1;
        for i in 0..fractional {
            buffer[len] = b'0' + digits[i + 1];
            len += 1;
        }
    }

    let mut suffix = [0u8; 5];
    let mut start = suffix.len();
    let mut magnitude = exponent.abs();
    loop {
        start -= 1;
        suffix[start] = b'0' + (magnitude % 10) as u8;
        magnitude /= 10;
        if magnitude == 0 {
            break;
        }
    }
    if exponent < 0 {
        start -= 1;
        suffix[start] = b'-';
    }
    start -= 1;
    suffix[start] = marker;
    // SAFETY: the mantissa and exponent only contain ASCII digits and punctuation.
    let (text, suffix) = unsafe {
        (
            crate::str::from_utf8_unchecked(&buffer[..len]),
            crate::str::from_utf8_unchecked(&suffix[start..]),
        )
    };
    pad_float(positive, text, precision - fractional, suffix, f)
}

macro_rules! float_format_impl {
    ($t:ty, $single:literal, $trait:ident, $style:ident) => {
        impl crate::fmt::$trait for $t {
            fn fmt(&self, f: &mut crate::fmt::Formatter<'_>) -> crate::fmt::Result {
                format_float(self.to_bits() as u64, $single, f, FloatFormat::$style)
            }
        }
    };
}
float_format_impl!(f32, true, Debug, Debug);
float_format_impl!(f64, false, Debug, Debug);
float_format_impl!(f32, true, Display, Display);
float_format_impl!(f64, false, Display, Display);
float_format_impl!(f32, true, LowerExp, LowerExp);
float_format_impl!(f64, false, LowerExp, LowerExp);
float_format_impl!(f32, true, UpperExp, UpperExp);
float_format_impl!(f64, false, UpperExp, UpperExp);

fn exact_decimal(magnitude: u64, single: bool) -> DecimalSeq {
    let fraction_bits = if single { 23u32 } else { 52 };
    let power = (magnitude >> fraction_bits) & if single { 255 } else { 2047 };
    let fraction = magnitude & ((1u64 << fraction_bits) - 1);
    let mantissa = if power == 0 {
        fraction
    } else {
        fraction | (1u64 << fraction_bits)
    };
    let exponent = (if power == 0 { 1 } else { power as i32 })
        - if single { 127 } else { 1023 }
        - fraction_bits as i32;
    decimal::binary(mantissa, exponent)
}

fn round_decimal(decimal: &mut DecimalSeq, cutoff: usize) {
    if cutoff >= decimal.num_digits {
        return;
    }
    let mut later = decimal.truncated;
    let mut i = cutoff + 1;
    while i < decimal.num_digits {
        later = later || decimal.digits[i] != 0;
        i += 1;
    }
    let up = decimal.digits[cutoff] > 5
        || decimal.digits[cutoff] == 5
            && (later || cutoff != 0 && decimal.digits[cutoff - 1] % 2 != 0);
    decimal.num_digits = cutoff;
    if up {
        let mut i = cutoff;
        while i != 0 && decimal.digits[i - 1] == 9 {
            i -= 1;
            decimal.digits[i] = 0;
        }
        if i == 0 {
            decimal.digits[0] = 1;
            decimal.num_digits = 1;
            decimal.decimal_point += 1;
        } else {
            decimal.digits[i - 1] += 1;
        }
    }
}

fn fixed_float(
    bits: u64,
    single: bool,
    precision: usize,
    f: &mut crate::fmt::Formatter<'_>,
) -> crate::fmt::Result {
    let sign = if single { 1u64 << 31 } else { 1u64 << 63 };
    let mut d = exact_decimal(bits & !sign, single);
    let cutoff = d.decimal_point as i128 + precision as i128;
    if cutoff < 0 {
        d.num_digits = 0;
    } else if cutoff < d.num_digits as i128 {
        round_decimal(&mut d, cutoff as usize);
    }
    if d.num_digits == 0 {
        d.decimal_point = 1;
        d.digits[0] = 0;
    }
    let integers = d.decimal_point.max(1) as usize;
    let fractional = precision.min(1074);
    let mut buffer = [0u8; 1400];
    let mut n = 0usize;
    while n < integers {
        buffer[n] = b'0'
            + if d.decimal_point > 0 && n < d.num_digits {
                d.digits[n]
            } else {
                0
            };
        n += 1;
    }
    if precision != 0 {
        buffer[n] = b'.';
        n += 1;
    }
    let mut i = 0usize;
    while i < fractional {
        let at = d.decimal_point + i as i32;
        buffer[n] = b'0'
            + if at >= 0 && (at as usize) < d.num_digits {
                d.digits[at as usize]
            } else {
                0
            };
        n += 1;
        i += 1;
    }
    // SAFETY: only ASCII decimal digits and '.' were written.
    let text = unsafe { crate::str::from_utf8_unchecked(&buffer[..n]) };
    pad_float(bits & sign == 0, text, precision - fractional, "", f)
}
fn pad_float(
    positive: bool,
    text: &str,
    tail: usize,
    suffix: &str,
    f: &mut crate::fmt::Formatter<'_>,
) -> crate::fmt::Result {
    use crate::fmt::{Alignment, Write};
    if tail == 0 && suffix.is_empty() {
        return f.pad_integral(positive, "", text);
    }
    let has_sign = !positive || f.sign_plus();
    let length = text
        .len()
        .saturating_add(tail)
        .saturating_add(suffix.len())
        .saturating_add(has_sign as usize);
    let padding = f.width().unwrap_or(0).saturating_sub(length);
    let align = f.align().unwrap_or(Alignment::Right);
    let zero_pad = f.sign_aware_zero_pad();
    let left = if zero_pad {
        0
    } else {
        match align {
            Alignment::Left => 0,
            Alignment::Right => padding,
            Alignment::Center => padding / 2,
        }
    };
    let fill = f.fill();
    let mut i = 0;
    while i < left {
        f.write_char(fill)?;
        i += 1;
    }
    if !positive {
        f.write_str("-")?;
    } else if f.sign_plus() {
        f.write_str("+")?;
    }
    if zero_pad {
        i = 0;
        while i < padding {
            f.write_str("0")?;
            i += 1;
        }
    }
    f.write_str(text)?;
    i = 0;
    while i < tail {
        f.write_str("0")?;
        i += 1;
    }
    if !suffix.is_empty() {
        f.write_str(suffix)?;
    }
    if !zero_pad {
        i = left;
        while i < padding {
            f.write_char(fill)?;
            i += 1;
        }
    }
    Ok(())
}

impl crate::convert::From<f32> for f64 { fn from(value:f32)->f64 {value as f64} }
impl crate::convert::From<u8> for f32 { fn from(value:u8)->f32 {value as f32} }
impl crate::convert::From<i8> for f32 { fn from(value:i8)->f32 {value as f32} }
impl crate::convert::From<u16> for f32 { fn from(value:u16)->f32 {value as f32} }
impl crate::convert::From<i16> for f32 { fn from(value:i16)->f32 {value as f32} }
impl crate::convert::From<u8> for f64 { fn from(value:u8)->f64 {value as f64} }
impl crate::convert::From<i8> for f64 { fn from(value:i8)->f64 {value as f64} }
impl crate::convert::From<u16> for f64 { fn from(value:u16)->f64 {value as f64} }
impl crate::convert::From<i16> for f64 { fn from(value:i16)->f64 {value as f64} }
impl crate::convert::From<u32> for f64 { fn from(value:u32)->f64 {value as f64} }
impl crate::convert::From<i32> for f64 { fn from(value:i32)->f64 {value as f64} }


macro_rules! float_bits_impl {
    ($t:ident, $u:ty, $s:ty, $n:expr, $shift:expr, $degrees:expr) => {
        impl $t {
            /// Converts radians to degrees.
            pub const fn to_degrees(self) -> Self { self * $degrees }
            /// Converts degrees to radians.
            pub const fn to_radians(self) -> Self {
                const RADS_PER_DEG: $t = crate::$t::consts::PI / 180.0;
                self * RADS_PER_DEG
            }
            /// 1.0 with this value's sign, or NaN for NaN.
            pub const fn signum(self) -> Self {
                if self.is_nan() { Self::NAN } else { (1.0 as $t).copysign(self) }
            }
            /// Restricts the value to `min..=max`; NaN stays NaN.
            ///
            /// # Panics
            /// Panics if `min > max`, or either is NaN.
            pub const fn clamp(mut self, min: Self, max: Self) -> Self {
                if !(min <= max) { crate::panicking::panic_float_clamp(min, max); }
                if self < min { self = min; }
                if self > max { self = max; }
                self
            }
            /// The IEEE 754 total order: negative NaN, -inf, ..., -0, +0, ..., inf, NaN.
            pub const fn total_cmp(&self, other: &Self) -> Ordering {
                let mut left = self.to_bits() as $s;
                let mut right = other.to_bits() as $s;
                left ^= (((left >> $shift) as $u) >> 1) as $s;
                right ^= (((right >> $shift) as $u) >> 1) as $s;
                if left < right { Ordering::Less } else if left > right { Ordering::Greater } else { Ordering::Equal }
            }
            /// The average of two values, without intermediate overflow.
            pub const fn midpoint(self, other: Self) -> Self {
                const HI: $t = <$t>::MAX * 0.5;
                let (a, b) = (self, other);
                if a.abs() <= HI && b.abs() <= HI { (a + b) * 0.5 } else { (a * 0.5) + (b * 0.5) }
            }
            pub const fn to_be_bytes(self) -> [u8; $n] { self.to_bits().to_be_bytes() }
            pub const fn to_le_bytes(self) -> [u8; $n] { self.to_bits().to_le_bytes() }
            pub const fn to_ne_bytes(self) -> [u8; $n] { self.to_bits().to_ne_bytes() }
            pub const fn from_be_bytes(bytes: [u8; $n]) -> Self { Self::from_bits(<$u>::from_be_bytes(bytes)) }
            pub const fn from_le_bytes(bytes: [u8; $n]) -> Self { Self::from_bits(<$u>::from_le_bytes(bytes)) }
            pub const fn from_ne_bytes(bytes: [u8; $n]) -> Self { Self::from_bits(<$u>::from_ne_bytes(bytes)) }
        }
    };
}
float_bits_impl!(f32, u32, i32, 4, 31, 57.2957795130823208767981548141051703_f32);
float_bits_impl!(f64, u64, i64, 8, 63, 180.0 / crate::f64::consts::PI);

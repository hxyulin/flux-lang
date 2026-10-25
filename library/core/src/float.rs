//! IEEE binary floating-point primitives.
use crate::cmp::Ordering;
macro_rules! float_impl {
    ($t:ty, $u:ty, $sign:expr, $exp:expr, $frac:expr, $max:expr, $minpos:expr, $epsilon:expr) => {
        impl $t {
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
    use decimal::DecimalSeq;
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

fn debug_float(bits:u64,single:bool,f:&mut crate::fmt::Formatter<'_>)->crate::fmt::Result {
    let sign=if single {1u64<<31} else {1u64<<63};
    let magnitude=bits&!sign;
    let infinity=if single {0x7f800000u64} else {0x7ff0000000000000u64};
    if magnitude>infinity { return f.pad_integral(true,"","NaN"); }
    if magnitude==infinity { return f.pad_integral(bits&sign==0,"","inf"); }
    if let Some(precision)=f.precision() {return fixed_float(bits,single,precision,f);}
    if magnitude==0 { return f.pad_integral(bits&sign==0,"","0.0"); }
    let (mut coefficient,exponent)=decimal::shortest(magnitude,single);
    let mut reversed=[0u8;20]; let mut digits=0usize;
    while coefficient!=0 { reversed[digits]=b'0'+(coefficient%10) as u8; coefficient/=10; digits+=1; }
    let scientific=digits as i32+exponent-1;
    let mut buffer=[0u8;64]; let mut n=0usize;
    if scientific < -4 || scientific >= 16 {
        buffer[n]=reversed[digits-1]; n+=1;
        if digits>1 {
            buffer[n]=b'.'; n+=1;
            let mut i=1; while i<digits { buffer[n]=reversed[digits-1-i]; n+=1; i+=1; }
        }
        buffer[n]=b'e'; n+=1;
        let mut exp=scientific;
        if exp<0 {buffer[n]=b'-'; n+=1; exp=-exp;}
        let mut r=[0u8;4]; let mut count=0usize;
        while exp!=0 {r[count]=b'0'+(exp%10) as u8; exp/=10; count+=1;}
        if count==0 {r[0]=b'0'; count=1;}
        while count!=0 {count-=1; buffer[n]=r[count]; n+=1;}
    } else {
        let point=digits as i32+exponent;
        if point<=0 {
            buffer[n]=b'0'; n+=1; buffer[n]=b'.'; n+=1;
            let mut zeros=-point; while zeros>0 {buffer[n]=b'0'; n+=1; zeros-=1;}
        }
        let mut i=0usize;
        while i<digits {
            if point>0 && i as i32==point {buffer[n]=b'.'; n+=1;}
            buffer[n]=reversed[digits-1-i]; n+=1; i+=1;
        }
        if point>=digits as i32 {
            let mut zeros=point-digits as i32;
            while zeros>0 {buffer[n]=b'0'; n+=1; zeros-=1;}
            buffer[n]=b'.'; n+=1; buffer[n]=b'0'; n+=1;
        }
    }
    // SAFETY: the buffer contains only ASCII digits and punctuation.
    f.pad_integral(bits&sign==0,"",unsafe { crate::str::from_utf8_unchecked(&buffer[..n]) })
}
impl crate::fmt::Debug for f32 { fn fmt(&self,f:&mut crate::fmt::Formatter<'_>)->crate::fmt::Result {debug_float(self.to_bits() as u64,true,f)} }
impl crate::fmt::Debug for f64 { fn fmt(&self,f:&mut crate::fmt::Formatter<'_>)->crate::fmt::Result {debug_float(self.to_bits(),false,f)} }

fn fixed_float(bits:u64,single:bool,precision:usize,f:&mut crate::fmt::Formatter<'_>)->crate::fmt::Result {
    let fraction_bits=if single {23u32} else {52};
    let sign=if single {1u64<<31} else {1u64<<63};
    let magnitude=bits&!sign;
    let power=(magnitude>>fraction_bits)&if single {255} else {2047};
    let fraction=magnitude&((1u64<<fraction_bits)-1);
    let mantissa=if power==0 {fraction} else {fraction|(1u64<<fraction_bits)};
    let exponent=(if power==0 {1} else {power as i32})-if single {127} else {1023}-fraction_bits as i32;
    let mut d=decimal::binary(mantissa,exponent);
    let cutoff=d.decimal_point as i128+precision as i128;
    if cutoff<0 {d.num_digits=0;}
    else if cutoff<d.num_digits as i128 {
        let end=cutoff as usize;
        let mut later=d.truncated;
        let mut i=end+1;
        while i<d.num_digits {later=later || d.digits[i]!=0;i+=1;}
        let up=d.digits[end]>5 || d.digits[end]==5 && (later || end!=0 && d.digits[end-1]%2!=0);
        d.num_digits=end;
        if up {
            let mut i=end;
            while i!=0 && d.digits[i-1]==9 {i-=1;d.digits[i]=0;}
            if i==0 {d.digits[0]=1;d.num_digits=1;d.decimal_point+=1;}
            else {d.digits[i-1]+=1;}
        }
    }
    if d.num_digits==0 {d.decimal_point=1;d.digits[0]=0;}
    let integers=d.decimal_point.max(1) as usize;
    let fractional=precision.min(1074);
    let mut buffer=[0u8;1400]; let mut n=0usize;
    while n<integers {
        buffer[n]=b'0'+if d.decimal_point>0 && n<d.num_digits {d.digits[n]} else {0};n+=1;
    }
    if precision!=0 {buffer[n]=b'.';n+=1;}
    let mut i=0usize;
    while i<fractional {
        let at=d.decimal_point+i as i32;
        buffer[n]=b'0'+if at>=0 && (at as usize)<d.num_digits {d.digits[at as usize]} else {0};
        n+=1;i+=1;
    }
    // SAFETY: only ASCII decimal digits and '.' were written.
    let text=unsafe {crate::str::from_utf8_unchecked(&buffer[..n])};
    pad_float(bits&sign==0,text,precision-fractional,f)
}
fn pad_float(positive:bool,text:&str,tail:usize,f:&mut crate::fmt::Formatter<'_>)->crate::fmt::Result {
    use crate::fmt::{Write,Alignment};
    if tail==0 {return f.pad_integral(positive,"",text);}
    let has_sign=!positive || f.sign_plus();
    let length=text.len().saturating_add(tail).saturating_add(has_sign as usize);
    let padding=f.width().unwrap_or(0).saturating_sub(length);
    let align=f.align().unwrap_or(Alignment::Right);
    let zero_pad=f.sign_aware_zero_pad();
    let left=if zero_pad {0} else {match align {Alignment::Left=>0,Alignment::Right=>padding,Alignment::Center=>padding/2}};
    let fill=f.fill();let mut i=0;
    while i<left {f.write_char(fill)?;i+=1;}
    if !positive {f.write_str("-")?;} else if f.sign_plus() {f.write_str("+")?;}
    if zero_pad {i=0;while i<padding {f.write_str("0")?;i+=1;}}
    f.write_str(text)?;
    i=0;while i<tail {f.write_str("0")?;i+=1;}
    if !zero_pad {i=left;while i<padding {f.write_char(fill)?;i+=1;}}
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

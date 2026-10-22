//! Integers' `Display`, `Debug`, `LowerHex`, `UpperHex`, `Octal` and
//! `Binary`.

use super::*;

/// Writes `n`'s decimal digits, after a `-` unless `is_nonnegative`.
fn fmt_u64(n: u64, is_nonnegative: bool, f: &mut Formatter<'_>) -> Result {
    let mut buf = [0u8; 20];
    let mut curr = buf.len();
    let mut n = n;
    loop {
        curr -= 1;
        buf[curr] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    let digits = unsafe { crate::str::from_utf8_unchecked(crate::intrinsics::subslice(&buf, curr, 20)) };
    f.pad_integral(is_nonnegative, "", digits)
}

fn fmt_u128(n: u128, is_nonnegative: bool, f: &mut Formatter<'_>) -> Result {
    if n <= u64::MAX as u128 {
        return fmt_u64(n as u64, is_nonnegative, f);
    }
    let mut buf = [0u8; 39];
    let mut curr = buf.len();
    let mut n = n;
    loop {
        curr -= 1;
        buf[curr] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    let digits = unsafe { crate::str::from_utf8_unchecked(crate::intrinsics::subslice(&buf, curr, 39)) };
    f.pad_integral(is_nonnegative, "", digits)
}

/// Writes `n`'s digits in base `1 << shift` (hexadecimal, octal or
/// binary), upper-case if `upper`, after `prefix` for `{:#}`.
fn fmt_radix(n: u128, shift: u32, upper: bool, prefix: &str, f: &mut Formatter<'_>) -> Result {
    let mut buf = [0u8; 128];
    let mut curr = buf.len();
    let mut n = n;
    let mask = (1u128 << shift) - 1;
    loop {
        curr -= 1;
        let d = (n & mask) as u8;
        buf[curr] = if d < 10 {
            b'0' + d
        } else if upper {
            b'A' + d - 10
        } else {
            b'a' + d - 10
        };
        n >>= shift;
        if n == 0 {
            break;
        }
    }
    let digits = unsafe { crate::str::from_utf8_unchecked(crate::intrinsics::subslice(&buf, curr, 128)) };
    f.pad_integral(true, prefix, digits)
}

macro_rules! radix_impls {
    ($($t:ty => $u:ty),*) => {
        $(
            impl LowerHex for $t {
                fn fmt(&self, f: &mut Formatter<'_>) -> Result {
                    fmt_radix(*self as $u as u128, 4, false, "0x", f)
                }
            }

            impl UpperHex for $t {
                fn fmt(&self, f: &mut Formatter<'_>) -> Result {
                    fmt_radix(*self as $u as u128, 4, true, "0x", f)
                }
            }

            impl Octal for $t {
                fn fmt(&self, f: &mut Formatter<'_>) -> Result {
                    fmt_radix(*self as $u as u128, 3, false, "0o", f)
                }
            }

            impl Binary for $t {
                fn fmt(&self, f: &mut Formatter<'_>) -> Result {
                    fmt_radix(*self as $u as u128, 1, false, "0b", f)
                }
            }

            impl Debug for $t {
                fn fmt(&self, f: &mut Formatter<'_>) -> Result {
                    if f.debug_lower_hex() {
                        LowerHex::fmt(self, f)
                    } else if f.debug_upper_hex() {
                        UpperHex::fmt(self, f)
                    } else {
                        Display::fmt(self, f)
                    }
                }
            }
        )*
    };
}

radix_impls!(
    i8 => u8, i16 => u16, i32 => u32, i64 => u64, i128 => u128, isize => usize,
    u8 => u8, u16 => u16, u32 => u32, u64 => u64, u128 => u128, usize => usize
);

macro_rules! unsigned_display {
    ($($t:ty),*) => {
        $(
            impl Display for $t {
                fn fmt(&self, f: &mut Formatter<'_>) -> Result {
                    fmt_u64(*self as u64, true, f)
                }

                // rustc's `to_string` makes room for the most digits.
                #[doc(hidden)]
                fn __to_string_hint(&self) -> super::ToStringHint {
                    super::ToStringHint::Capacity(<$t>::MAX.ilog10() as usize + 1)
                }
            }
        )*
    };
}

macro_rules! signed_display {
    ($($t:ty),*) => {
        $(
            impl Display for $t {
                fn fmt(&self, f: &mut Formatter<'_>) -> Result {
                    let n = *self as i64;
                    let abs = if n < 0 { 0u64.wrapping_sub(n as u64) } else { n as u64 };
                    fmt_u64(abs, n >= 0, f)
                }

                // The most digits, and the sign.
                #[doc(hidden)]
                fn __to_string_hint(&self) -> super::ToStringHint {
                    let digits = <$t>::MAX.ilog10() as usize + 1;
                    super::ToStringHint::Capacity(digits + (*self < 0) as usize)
                }
            }
        )*
    };
}

unsigned_display!(u8, u16, u32, u64, usize);
signed_display!(i8, i16, i32, i64, isize);

impl Display for u128 {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        fmt_u128(*self, true, f)
    }

    #[doc(hidden)]
    fn __to_string_hint(&self) -> super::ToStringHint {
        super::ToStringHint::Capacity(39)
    }
}

impl Display for i128 {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        let n = *self;
        let abs = if n < 0 { 0u128.wrapping_sub(n as u128) } else { n as u128 };
        fmt_u128(abs, n >= 0, f)
    }

    #[doc(hidden)]
    fn __to_string_hint(&self) -> super::ToStringHint {
        super::ToStringHint::Capacity(39 + (*self < 0) as usize)
    }
}

/// `{:e}` and `{:E}` of an integer, as rustc writes them: a coefficient
/// in `[1, 10)` with its trailing zeros dropped (or as many decimals as
/// the precision says, rounding half to even), then `e` and the power
/// of ten.
fn fmt_exp(n: u128, is_nonnegative: bool, letter: u8, f: &mut Formatter<'_>) -> Result {
    let mut exp = 0usize;
    let mut m = n;
    while m >= 10 {
        m /= 10;
        exp += 1;
    }
    let mut coef_prec = exp;
    let mut coef = n;
    let more_prec = match f.precision() {
        None => {
            while coef_prec != 0 && coef % 10 == 0 {
                coef /= 10;
                coef_prec -= 1;
            }
            0
        }
        Some(p) if p >= coef_prec => p - coef_prec,
        Some(p) => {
            let mut scale = 1u128;
            let mut i = 0;
            while i < coef_prec - p {
                scale *= 10;
                i += 1;
            }
            let floor = coef / scale;
            let over = coef % scale;
            let half = scale / 2;
            let round_up = if over < half {
                0
            } else if over > half {
                1
            } else {
                floor & 1
            };
            coef = floor + round_up;
            coef_prec = p;
            // Rounding up may have made ten of the coefficient.
            let mut digits = 0;
            let mut c = coef;
            while c >= 10 {
                c /= 10;
                digits += 1;
            }
            if round_up != 0 && digits > coef_prec {
                coef /= 10;
                exp += 1;
            }
            0
        }
    };
    // The coefficient's digits, a `.` after the first if it has more.
    let mut buf = [0u8; 48];
    let mut coef_len = 1;
    if coef_prec == 0 && more_prec == 0 {
        buf[0] = b'0' + coef as u8;
    } else {
        let mut i = coef_prec + 1;
        let mut c = coef;
        while i >= 2 {
            buf[i] = b'0' + (c % 10) as u8;
            c /= 10;
            i -= 1;
        }
        buf[1] = b'.';
        buf[0] = b'0' + c as u8;
        coef_len = coef_prec + 2;
    }
    let mut exp_buf = [0u8; 3];
    exp_buf[0] = letter;
    let exp_len = if exp < 10 {
        exp_buf[1] = b'0' + exp as u8;
        2
    } else {
        exp_buf[1] = b'0' + (exp / 10) as u8;
        exp_buf[2] = b'0' + (exp % 10) as u8;
        3
    };
    let coef_text = unsafe { crate::str::from_utf8_unchecked(crate::intrinsics::subslice(&buf, 0, coef_len)) };
    let exp_text = unsafe { crate::str::from_utf8_unchecked(crate::intrinsics::subslice(&exp_buf, 0, exp_len)) };
    if more_prec == 0 {
        let mut text = [0u8; 51];
        let mut i = 0;
        while i < coef_len {
            text[i] = buf[i];
            i += 1;
        }
        let mut j = 0;
        while j < exp_len {
            text[i + j] = exp_buf[j];
            j += 1;
        }
        let s = unsafe { crate::str::from_utf8_unchecked(crate::intrinsics::subslice(&text, 0, i + j)) };
        return f.pad_integral(is_nonnegative, "", s);
    }
    // rustc's `pad_formatted_parts`, with `more_prec` zeros between.
    let sign = if !is_nonnegative {
        "-"
    } else if f.sign_plus() {
        "+"
    } else {
        ""
    };
    let len = sign.len() + coef_len + more_prec + exp_len;
    let mut width = f.options.width as usize;
    let write_parts = |f: &mut Formatter<'_>, sign: &str| -> Result {
        f.buf.write_str(sign)?;
        f.buf.write_str(coef_text)?;
        let mut i = 0;
        while i < more_prec {
            f.buf.write_str("0")?;
            i += 1;
        }
        f.buf.write_str(exp_text)
    };
    if width <= len {
        return write_parts(f, sign);
    }
    let old = f.options;
    let mut sign = sign;
    let mut len = len;
    if f.sign_aware_zero_pad() {
        f.buf.write_str(sign)?;
        len -= sign.len();
        width -= sign.len();
        sign = "";
        f.set_fill('0');
        f.set_align(flags::ALIGN_RIGHT);
    }
    let post = f.padding(width - len, Alignment::Right)?;
    write_parts(f, sign)?;
    post.write(f)?;
    f.options = old;
    Ok(())
}

macro_rules! exp_impls {
    ($($t:ty => $signed:expr),*) => {
        $(
            impl LowerExp for $t {
                fn fmt(&self, f: &mut Formatter<'_>) -> Result {
                    let n = *self;
                    let negative = $signed && n < (0 as $t);
                    let abs = if negative { 0u128.wrapping_sub(n as u128) } else { n as u128 };
                    fmt_exp(abs, !negative, b'e', f)
                }
            }

            impl UpperExp for $t {
                fn fmt(&self, f: &mut Formatter<'_>) -> Result {
                    let n = *self;
                    let negative = $signed && n < (0 as $t);
                    let abs = if negative { 0u128.wrapping_sub(n as u128) } else { n as u128 };
                    fmt_exp(abs, !negative, b'E', f)
                }
            }
        )*
    };
}

exp_impls!(
    i8 => true, i16 => true, i32 => true, i64 => true, i128 => true, isize => true,
    u8 => false, u16 => false, u32 => false, u64 => false, u128 => false, usize => false
);

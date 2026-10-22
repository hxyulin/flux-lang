//! Fixed-width unsigned big integers `BigUint<N>` stored as `N` little-endian
//! `u32` limbs, with carries and borrows done through `u64`: addition,
//! subtraction, schoolbook multiplication, division by a small number,
//! shifts, comparison through `Ord`, and decimal and hex formatting into a
//! byte buffer. Computes 50!, 2^200, Fibonacci(400), a binomial
//! coefficient, and checks a few identities. The operators come from
//! `core::ops`, and overflow past N limbs is reported, not wrapped.

use core::cmp::Ordering;
use core::ops::{Add, AddAssign, Mul, Shl, Sub};

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BigUint<const N: usize> {
    limbs: [u32; N],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BigError {
    Overflow,
    DivideByZero,
}

impl<const N: usize> BigUint<N> {
    const ZERO: Self = BigUint { limbs: [0; N] };

    fn from_u64(x: u64) -> Self {
        let mut r = Self::ZERO;
        r.limbs[0] = x as u32;
        if N > 1 {
            r.limbs[1] = (x >> 32) as u32;
        }
        r
    }

    fn is_zero(&self) -> bool {
        self.limbs.iter().all(|&l| l == 0)
    }

    /// Number of significant limbs.
    fn used(&self) -> usize {
        self.limbs.iter().rposition(|&l| l != 0).map_or(0, |i| i + 1)
    }

    fn bits(&self) -> u32 {
        match self.used() {
            0 => 0,
            n => (n as u32 - 1) * 32 + (32 - self.limbs[n - 1].leading_zeros()),
        }
    }

    fn checked_add(&self, other: &Self) -> Result<Self, BigError> {
        let mut r = Self::ZERO;
        let mut carry = 0u64;
        for i in 0..N {
            let s = self.limbs[i] as u64 + other.limbs[i] as u64 + carry;
            r.limbs[i] = s as u32;
            carry = s >> 32;
        }
        if carry != 0 { Err(BigError::Overflow) } else { Ok(r) }
    }

    fn checked_sub(&self, other: &Self) -> Option<Self> {
        let mut r = Self::ZERO;
        let mut borrow = 0i64;
        for (i, (&a, &b)) in self.limbs.iter().zip(other.limbs.iter()).enumerate() {
            let mut d = a as i64 - b as i64 - borrow;
            borrow = 0;
            if d < 0 {
                d += 1 << 32;
                borrow = 1;
            }
            r.limbs[i] = d as u32;
        }
        (borrow == 0).then_some(r)
    }

    fn checked_mul(&self, other: &Self) -> Result<Self, BigError> {
        let mut r = Self::ZERO;
        for i in 0..self.used() {
            let mut carry = 0u64;
            for j in 0..other.used() {
                let t = self.limbs[i] as u64 * other.limbs[j] as u64 + r.limbs.get(i + j).copied().unwrap_or(0) as u64 + carry;
                if i + j >= N {
                    if t != 0 {
                        return Err(BigError::Overflow);
                    }
                    continue;
                }
                r.limbs[i + j] = t as u32;
                carry = t >> 32;
            }
            let k = i + other.used();
            if carry != 0 {
                if k >= N {
                    return Err(BigError::Overflow);
                }
                r.limbs[k] = carry as u32;
            }
        }
        Ok(r)
    }

    fn mul_small(&self, m: u32) -> Result<Self, BigError> {
        let mut r = *self;
        let mut carry = 0u64;
        for limb in r.limbs.iter_mut() {
            let t = *limb as u64 * m as u64 + carry;
            *limb = t as u32;
            carry = t >> 32;
        }
        if carry == 0 { Ok(r) } else { Err(BigError::Overflow) }
    }

    /// Divides in place, returning the remainder.
    fn div_rem_small(&mut self, d: u32) -> Result<u32, BigError> {
        if d == 0 {
            return Err(BigError::DivideByZero);
        }
        let mut rem = 0u64;
        for limb in self.limbs.iter_mut().rev() {
            let cur = (rem << 32) | *limb as u64;
            *limb = (cur / d as u64) as u32;
            rem = cur % d as u64;
        }
        Ok(rem as u32)
    }

    /// Writes the decimal digits into `buf`, returning the used tail.
    fn to_decimal<'b>(&self, buf: &'b mut [u8]) -> &'b str {
        let mut n = *self;
        let mut pos = buf.len();
        loop {
            // Peel off nine digits at a time.
            let mut chunk = n.div_rem_small(1_000_000_000).unwrap();
            let last = n.is_zero();
            for _ in 0..9 {
                pos -= 1;
                buf[pos] = b'0' + (chunk % 10) as u8;
                chunk /= 10;
                if last && chunk == 0 {
                    break;
                }
            }
            if last {
                break;
            }
        }
        core::str::from_utf8(&buf[pos..]).unwrap()
    }

    fn digit_sum(&self) -> u32 {
        let mut buf = [0u8; 400];
        self.to_decimal(&mut buf).bytes().map(|b| (b - b'0') as u32).sum()
    }
}

impl<const N: usize> Ord for BigUint<N> {
    fn cmp(&self, other: &Self) -> Ordering {
        for i in (0..N).rev() {
            match self.limbs[i].cmp(&other.limbs[i]) {
                Ordering::Equal => continue,
                ord => return ord,
            }
        }
        Ordering::Equal
    }
}

impl<const N: usize> PartialOrd for BigUint<N> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<const N: usize> Add for BigUint<N> {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        self.checked_add(&rhs).expect("BigUint addition overflowed")
    }
}

impl<const N: usize> AddAssign for BigUint<N> {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

impl<const N: usize> Sub for BigUint<N> {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        self.checked_sub(&rhs).expect("BigUint subtraction underflowed")
    }
}

impl<const N: usize> Mul for BigUint<N> {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        self.checked_mul(&rhs).expect("BigUint multiplication overflowed")
    }
}

impl<const N: usize> Shl<u32> for BigUint<N> {
    type Output = Self;
    fn shl(self, s: u32) -> Self {
        let (words, bits) = ((s / 32) as usize, s % 32);
        let mut r = Self::ZERO;
        for i in (words..N).rev() {
            let src = i - words;
            let mut v = self.limbs[src] << bits;
            if bits > 0 && src > 0 {
                v |= self.limbs[src - 1] >> (32 - bits);
            }
            r.limbs[i] = v;
        }
        r
    }
}

impl<const N: usize> From<u32> for BigUint<N> {
    fn from(x: u32) -> Self {
        Self::from_u64(x as u64)
    }
}

type U1024 = BigUint<32>;

fn factorial(n: u32) -> Result<U1024, BigError> {
    (2..=n).try_fold(U1024::from(1), |acc, k| acc.mul_small(k))
}

fn fibonacci(n: u32) -> U1024 {
    let (mut a, mut b) = (U1024::ZERO, U1024::from(1));
    for _ in 0..n {
        let next = a + b;
        a = b;
        b = next;
    }
    a
}

fn binomial(n: u32, k: u32) -> U1024 {
    // C(n, k) = prod (n - k + i) / i, exact at each step.
    let mut r = U1024::from(1);
    for i in 1..=k {
        r = r.mul_small(n - k + i).unwrap();
        let rem = r.div_rem_small(i).unwrap();
        assert_eq!(rem, 0);
    }
    r
}

fn show(label: &str, x: &U1024) {
    let mut buf = [0u8; 400];
    let s = x.to_decimal(&mut buf);
    unsafe {
        printf(
            c"%-10.*s = %.*s (%zu digits, %u bits)\n".as_ptr(),
            label.len() as i32,
            label.as_ptr(),
            s.len() as i32,
            s.as_ptr(),
            s.len(),
            x.bits(),
        );
    }
}

fn main() {
    let f50 = factorial(50).unwrap();
    show("50!", &f50);
    let f25 = factorial(25).unwrap();
    show("25!", &f25);

    let two_200 = U1024::from(1) << 200;
    show("2^200", &two_200);

    let fib = fibonacci(400);
    show("fib(400)", &fib);

    let c = binomial(100, 50);
    show("C(100,50)", &c);

    // Identities.
    let f51 = factorial(51).unwrap();
    assert!(f51 == f50.mul_small(51).unwrap());
    assert!(f51 > f50 && f50 >= f50 && fib < two_200 * two_200);
    let diff = f51 - f50;
    assert_eq!(diff, f50 * U1024::from(50));
    // fib(n)^2 - fib(n-1) * fib(n+1) = (-1)^(n-1); for even n: fib(n-1)*fib(n+1) - fib(n)^2 = 1.
    let (f399, f401) = (fibonacci(399), fibonacci(401));
    assert!(f399 * f401 - fib * fib == U1024::from(1));

    let mut acc = U1024::ZERO;
    for k in 0..=100 {
        acc += binomial(100, k);
    }
    assert!(acc == U1024::from(1) << 100);
    show("sum C(100)", &acc);

    // Overflow and division errors are values, not panics.
    let huge = factorial(200);
    let mut zero_div = U1024::from(7);
    let errors = [huge.err(), zero_div.div_rem_small(0).err(), two_200.checked_mul(&(two_200 << 700)).err()];
    let mut overflow_count = 0;
    for e in errors.iter().flatten() {
        match e {
            BigError::Overflow => overflow_count += 1,
            BigError::DivideByZero => unsafe { printf(c"division by zero caught\n".as_ptr()); },
        }
    }
    unsafe { printf(c"%d overflows caught\n".as_ptr(), overflow_count) };

    let mut factorials_hex = f50;
    let mut hex = [0u8; 80];
    let mut n = 0;
    while !factorials_hex.is_zero() {
        let d = factorials_hex.div_rem_small(16).unwrap();
        hex[n] = b"0123456789abcdef"[d as usize];
        n += 1;
    }
    hex[..n].reverse();
    unsafe { printf(c"50! in hex = %.*s\n".as_ptr(), n as i32, hex.as_ptr()) };

    let sum = f50.digit_sum();
    unsafe { printf(c"digit sum of 50! = %u, of 2^200 = %u\n".as_ptr(), sum, two_200.digit_sum()) };
}

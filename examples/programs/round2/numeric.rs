//! Integer-only numeric code: an `Rng` trait with two generators (PCG32
//! and xorshift64*) and provided methods (`below` without modulo bias,
//! `shuffle`, `fill`); a Q16.16 fixed-point type with the arithmetic
//! operator traits, Newton square root and a Taylor-series sine; and a
//! generic `Matrix<T, R, C>` with const generic dimensions whose `Mul`
//! produces `Matrix<T, R, K>`, over a `Ring` trait with associated
//! constants, used for Fibonacci by fast matrix power (mod a prime) and
//! fixed-point rotations. Ends with a Monte Carlo estimate of pi and a
//! timing-sized matrix power loop.

use core::ops::{Add, Div, Mul, Neg, Sub};

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

trait Rng {
    fn next_u32(&mut self) -> u32;

    fn next_u64(&mut self) -> u64 {
        (self.next_u32() as u64) << 32 | self.next_u32() as u64
    }

    /// Uniform in `0..n`, by rejection (Lemire's method).
    fn below(&mut self, n: u32) -> u32 {
        assert!(n > 0);
        let threshold = n.wrapping_neg() % n;
        loop {
            let m = self.next_u32() as u64 * n as u64;
            if (m as u32) >= threshold {
                return (m >> 32) as u32;
            }
        }
    }

    fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = self.below(i as u32 + 1) as usize;
            items.swap(i, j);
        }
    }

    fn fill(&mut self, out: &mut [u32]) {
        for x in out {
            *x = self.next_u32();
        }
    }
}

struct Pcg32 {
    state: u64,
    inc: u64,
}

impl Pcg32 {
    fn new(seed: u64, stream: u64) -> Self {
        let mut rng = Pcg32 { state: 0, inc: (stream << 1) | 1 };
        rng.next_u32();
        rng.state = rng.state.wrapping_add(seed);
        rng.next_u32();
        rng
    }
}

impl Rng for Pcg32 {
    fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.state = old.wrapping_mul(6364136223846793005).wrapping_add(self.inc);
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        let rot = (old >> 59) as u32;
        xorshifted.rotate_right(rot)
    }
}

struct XorShiftStar(u64);

impl Rng for XorShiftStar {
    fn next_u32(&mut self) -> u32 {
        (self.next_u64() >> 32) as u32
    }
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
}

/// Q16.16 fixed point.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Debug)]
struct Fixed(i32);

impl Fixed {
    const FRAC: u32 = 16;
    const ONE: Fixed = Fixed(1 << Self::FRAC);
    const PI: Fixed = Fixed(205_887);

    const fn from_int(n: i32) -> Fixed {
        Fixed(n << Self::FRAC)
    }

    fn from_ratio(num: i32, den: i32) -> Fixed {
        Fixed((((num as i64) << Self::FRAC) / den as i64) as i32)
    }

    fn sqrt(self) -> Fixed {
        assert!(self.0 >= 0, "sqrt of a negative number");
        if self.0 == 0 {
            return self;
        }
        let mut x = if self > Fixed::ONE { self } else { Fixed::ONE };
        for _ in 0..20 {
            let next = (x + self / x) / Fixed::from_int(2);
            if next == x {
                break;
            }
            x = next;
        }
        x
    }

    /// sin(x) for |x| <= pi, by its Taylor series.
    fn sin(self) -> Fixed {
        let x2 = self * self;
        let mut term = self;
        let mut sum = self;
        for k in 1..8 {
            term = -(term * x2) / Fixed::from_int((2 * k) * (2 * k + 1));
            sum = sum + term;
        }
        sum
    }

    /// Prints with four decimals.
    fn show(self) {
        let neg = self.0 < 0;
        let v = (self.0 as i64).abs();
        let int = v >> Self::FRAC;
        let frac = ((v & 0xffff) * 10_000 + (1 << 15)) >> Self::FRAC;
        unsafe { printf(c"%s%lld.%04lld".as_ptr(), if neg { c"-".as_ptr() } else { c"".as_ptr() }, int, frac) };
    }
}

impl Add for Fixed {
    type Output = Fixed;
    fn add(self, o: Fixed) -> Fixed {
        Fixed(self.0.wrapping_add(o.0))
    }
}
impl Sub for Fixed {
    type Output = Fixed;
    fn sub(self, o: Fixed) -> Fixed {
        Fixed(self.0.wrapping_sub(o.0))
    }
}
impl Mul for Fixed {
    type Output = Fixed;
    fn mul(self, o: Fixed) -> Fixed {
        Fixed(((self.0 as i64 * o.0 as i64) >> Self::FRAC) as i32)
    }
}
impl Div for Fixed {
    type Output = Fixed;
    fn div(self, o: Fixed) -> Fixed {
        Fixed((((self.0 as i64) << Self::FRAC) / o.0 as i64) as i32)
    }
}
impl Neg for Fixed {
    type Output = Fixed;
    fn neg(self) -> Fixed {
        Fixed(-self.0)
    }
}

trait Ring: Copy + Add<Output = Self> + Mul<Output = Self> {
    const ZERO: Self;
    const ONE: Self;
}

impl Ring for Fixed {
    const ZERO: Fixed = Fixed(0);
    const ONE: Fixed = Fixed::ONE;
}

impl Ring for i64 {
    const ZERO: i64 = 0;
    const ONE: i64 = 1;
}

/// Integers modulo a prime given as a const parameter.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct ModP<const P: u64>(u64);

impl<const P: u64> Add for ModP<P> {
    type Output = Self;
    fn add(self, o: Self) -> Self {
        ModP((self.0 + o.0) % P)
    }
}
impl<const P: u64> Mul for ModP<P> {
    type Output = Self;
    fn mul(self, o: Self) -> Self {
        ModP(((self.0 as u128 * o.0 as u128) % P as u128) as u64)
    }
}
impl<const P: u64> Ring for ModP<P> {
    const ZERO: Self = ModP(0);
    const ONE: Self = ModP(1 % P);
}

#[derive(Clone, Copy, PartialEq, Debug)]
struct Matrix<T, const R: usize, const C: usize> {
    m: [[T; C]; R],
}

impl<T: Ring, const R: usize, const C: usize> Matrix<T, R, C> {
    fn zero() -> Self {
        Matrix { m: [[T::ZERO; C]; R] }
    }

    fn transpose(&self) -> Matrix<T, C, R> {
        let mut t = Matrix::<T, C, R>::zero();
        for r in 0..R {
            for c in 0..C {
                t.m[c][r] = self.m[r][c];
            }
        }
        t
    }
}

impl<T: Ring, const N: usize> Matrix<T, N, N> {
    fn identity() -> Self {
        let mut id = Self::zero();
        for i in 0..N {
            id.m[i][i] = T::ONE;
        }
        id
    }

    fn pow(self, mut e: u64) -> Self {
        let mut base = self;
        let mut acc = Self::identity();
        while e > 0 {
            if e & 1 == 1 {
                acc = acc * base;
            }
            base = base * base;
            e >>= 1;
        }
        acc
    }
}

impl<T: Ring, const R: usize, const K: usize, const C: usize> Mul<Matrix<T, K, C>> for Matrix<T, R, K> {
    type Output = Matrix<T, R, C>;
    fn mul(self, rhs: Matrix<T, K, C>) -> Matrix<T, R, C> {
        let mut out = Matrix::<T, R, C>::zero();
        for r in 0..R {
            for c in 0..C {
                out.m[r][c] = (0..K).fold(T::ZERO, |acc, k| acc + self.m[r][k] * rhs.m[k][c]);
            }
        }
        out
    }
}

const P: u64 = 1_000_000_007;

fn fib_mod(n: u64) -> u64 {
    let q = Matrix::<ModP<P>, 2, 2> { m: [[ModP(1), ModP(1)], [ModP(1), ModP(0)]] };
    q.pow(n).m[0][1].0
}

fn main() {
    // Generators.
    let mut pcg = Pcg32::new(42, 54);
    let mut first = [0u32; 6];
    pcg.fill(&mut first);
    unsafe { printf(c"pcg32:".as_ptr()) };
    for x in first {
        unsafe { printf(c" %08x".as_ptr(), x) };
    }
    let mut xs = XorShiftStar(0x1234_5678_9abc_def1);
    unsafe { printf(c"\nxorshift*: %016llx %016llx\n".as_ptr(), xs.next_u64(), xs.next_u64()) };

    // Distribution of `below` and a shuffle.
    let mut buckets = [0u32; 6];
    for _ in 0..60_000 {
        buckets[pcg.below(6) as usize] += 1;
    }
    let (lo, hi) = (buckets.iter().min().unwrap(), buckets.iter().max().unwrap());
    unsafe { printf(c"die: min %u max %u spread ok %d\n".as_ptr(), *lo, *hi, (hi - lo < 600) as i32) };
    let mut deck: [u8; 10] = core::array::from_fn(|i| i as u8);
    xs.shuffle(&mut deck);
    let mut sorted = deck;
    sorted.sort_unstable();
    unsafe { printf(c"shuffled:".as_ptr()) };
    deck.iter().for_each(|d| unsafe { printf(c" %d".as_ptr(), *d as i32); });
    unsafe { printf(c" (permutation %d)\n".as_ptr(), (sorted == core::array::from_fn::<u8, 10, _>(|i| i as u8)) as i32) };

    // Fixed point.
    for n in [2, 3, 10, 144] {
        unsafe { printf(c"sqrt(%d) = ".as_ptr(), n) };
        Fixed::from_int(n).sqrt().show();
        unsafe { printf(c"\n".as_ptr()) };
    }
    for (num, den) in [(0, 1), (1, 6), (1, 2), (-1, 3), (1, 1)] {
        let x = Fixed::PI * Fixed::from_ratio(num, den);
        unsafe { printf(c"sin(%d/%d pi) = ".as_ptr(), num, den) };
        x.sin().show();
        unsafe { printf(c"\n".as_ptr()) };
    }

    // Matrices: a rectangular product, a transpose, a rotation.
    let a = Matrix::<i64, 2, 3> { m: [[1, 2, 3], [4, 5, 6]] };
    let b = a.transpose();
    let ab: Matrix<i64, 2, 2> = a * b;
    let ba: Matrix<i64, 3, 3> = b * a;
    unsafe { printf(c"a*aT = [%lld %lld; %lld %lld], trace(aT*a) = %lld\n".as_ptr(),
        ab.m[0][0], ab.m[0][1], ab.m[1][0], ab.m[1][1], ba.m[0][0] + ba.m[1][1] + ba.m[2][2]) };
    let (s, c) = ((Fixed::PI / Fixed::from_int(6)).sin(), (Fixed::PI / Fixed::from_int(3)).sin());
    let rot = Matrix::<Fixed, 2, 2> { m: [[c, -s], [s, c]] };
    let p = Matrix::<Fixed, 2, 1> { m: [[Fixed::ONE], [Fixed::ZERO]] };
    let turned = rot.pow(3) * p;
    unsafe { printf(c"(1,0) rotated 3 x 30 deg = (".as_ptr()) };
    turned.m[0][0].show();
    unsafe { printf(c", ".as_ptr()) };
    turned.m[1][0].show();
    unsafe { printf(c")\n".as_ptr()) };

    for n in [10, 90, 1_000, 1_000_000_000_000] {
        unsafe { printf(c"fib(%llu) mod p = %llu\n".as_ptr(), n, fib_mod(n)) };
    }

    // Monte Carlo pi with integer coordinates.
    let mut rng = Pcg32::new(7, 7);
    let trials = 2_000_000u32;
    let r = 1u64 << 16;
    let inside = (0..trials)
        .filter(|_| {
            let (x, y) = (rng.below(r as u32) as u64, rng.below(r as u32) as u64);
            x * x + y * y < r * r
        })
        .count() as i32;
    unsafe { printf(c"pi ~ ".as_ptr()) };
    Fixed::from_ratio(inside, trials as i32 / 4).show();
    unsafe { printf(c"\n".as_ptr()) };

    // A heavier loop: many 8x8 matrix powers mod p.
    let mut m = Matrix::<ModP<P>, 8, 8>::zero();
    for i in 0..8 {
        for j in 0..8 {
            m.m[i][j] = ModP(rng.below(1000) as u64);
        }
    }
    let mut checksum = 0u64;
    for e in 0..2000u64 {
        let pm = m.pow(e * 7919 + 1);
        checksum = checksum.wrapping_mul(31).wrapping_add(pm.m[e as usize % 8][(e as usize / 8) % 8].0);
    }
    unsafe { printf(c"matrix power checksum %llu\n".as_ptr(), checksum) };
}

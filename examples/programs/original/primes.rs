//! Number theory: a sieve of Eratosthenes, prime factorization by trial
//! division over the sieved primes, gcd/lcm, Euler's totient, perfect
//! numbers, Goldbach decompositions and twin primes, mostly through
//! iterator chains.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

const LIMIT: usize = 1000;

struct Sieve {
    composite: [bool; LIMIT + 1],
}

impl Sieve {
    fn new() -> Self {
        let mut composite = [false; LIMIT + 1];
        composite[0] = true;
        composite[1] = true;
        let mut i = 2;
        while i * i <= LIMIT {
            if !composite[i] {
                for j in (i * i..=LIMIT).step_by(i) {
                    composite[j] = true;
                }
            }
            i += 1;
        }
        Sieve { composite }
    }

    fn is_prime(&self, n: usize) -> bool {
        n <= LIMIT && !self.composite[n]
    }

    fn primes(&self) -> impl Iterator<Item = usize> + '_ {
        (2..=LIMIT).filter(move |&n| self.is_prime(n))
    }
}

/// Prime factors with multiplicity, as (prime, exponent) pairs.
struct Factors {
    pairs: [(u64, u32); 16],
    len: usize,
}

impl Factors {
    fn iter(&self) -> impl Iterator<Item = &(u64, u32)> {
        self.pairs[..self.len].iter()
    }
}

fn factorize(mut n: u64, sieve: &Sieve) -> Factors {
    let mut f = Factors { pairs: [(0, 0); 16], len: 0 };
    for p in sieve.primes().map(|p| p as u64) {
        if p * p > n {
            break;
        }
        let mut e = 0;
        while n % p == 0 {
            n /= p;
            e += 1;
        }
        if e > 0 {
            f.pairs[f.len] = (p, e);
            f.len += 1;
        }
    }
    if n > 1 {
        f.pairs[f.len] = (n, 1);
        f.len += 1;
    }
    f
}

fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 { a } else { gcd(b, a % b) }
}

fn lcm(a: u64, b: u64) -> u64 {
    a / gcd(a, b) * b
}

fn totient(n: u64, sieve: &Sieve) -> u64 {
    factorize(n, sieve).iter().fold(n, |acc, &(p, _)| acc / p * (p - 1))
}

fn divisor_sum(n: u64) -> u64 {
    (1..n).filter(|d| n % d == 0).sum()
}

fn goldbach(n: usize, sieve: &Sieve) -> Option<(usize, usize)> {
    sieve.primes().take_while(|&p| p <= n / 2).find(|&p| sieve.is_prime(n - p)).map(|p| (p, n - p))
}

fn print_factors(n: u64, sieve: &Sieve) {
    unsafe { printf(c"%llu =".as_ptr(), n) };
    for (i, &(p, e)) in factorize(n, sieve).iter().enumerate() {
        let sep = if i == 0 { c" " } else { c" * " };
        unsafe {
            if e == 1 {
                printf(c"%s%llu".as_ptr(), sep.as_ptr(), p);
            } else {
                printf(c"%s%llu^%u".as_ptr(), sep.as_ptr(), p, e);
            }
        }
    }
    unsafe { printf(c"\n".as_ptr()) };
}

fn main() {
    let sieve = Sieve::new();
    let count = sieve.primes().count();
    let sum: usize = sieve.primes().sum();
    let largest = sieve.primes().last().unwrap_or(0);
    unsafe {
        printf(c"%zu primes below %zu, sum %zu, largest %zu\n".as_ptr(), count, LIMIT, sum, largest);
    }

    for n in [360u64, 997, 1001, 65536, 999_999, 600851475143] {
        print_factors(n, &sieve);
    }

    let lcm_1_to_20 = (1..=20u64).fold(1, lcm);
    unsafe {
        printf(c"gcd(1071, 462) = %llu\n".as_ptr(), gcd(1071, 462));
        printf(c"lcm(1..=20) = %llu\n".as_ptr(), lcm_1_to_20);
    }

    let totients: u64 = (1..=100).map(|n| totient(n, &sieve)).sum();
    unsafe { printf(c"sum of phi(1..=100) = %llu\n".as_ptr(), totients) };

    let perfect = (2..10_000u64).filter(|&n| divisor_sum(n) == n);
    unsafe { printf(c"perfect:".as_ptr()) };
    perfect.for_each(|n| unsafe {
        printf(c" %llu".as_ptr(), n);
    });
    unsafe { printf(c"\n".as_ptr()) };

    for n in (4..=40).step_by(6) {
        if let Some((p, q)) = goldbach(n, &sieve) {
            unsafe { printf(c"%zu = %zu + %zu\n".as_ptr(), n, p, q) };
        }
    }

    let twins = sieve.primes().zip(sieve.primes().skip(1)).filter(|&(p, q)| q - p == 2).count();
    let (gap_start, gap) = sieve
        .primes()
        .zip(sieve.primes().skip(1))
        .map(|(p, q)| (p, q - p))
        .max_by_key(|&(_, g)| g)
        .unwrap();
    unsafe {
        printf(c"%zu twin prime pairs, largest gap %zu after %zu\n".as_ptr(), twins, gap, gap_start);
    }
}

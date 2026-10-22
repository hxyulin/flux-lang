//! flux variant of ../hashing.rs, minimally rewritten to compile with flux:
//! Own AsBytes trait instead of AsRef<[u8]>; the fnv! macro takes the
//! output length as a literal instead of size_of::<$ty>() (or (<$ty>::BITS
//! / 8) as usize, which hits bugs/macro_item_array_len_expr.rs); the slice
//! pattern becomes a tuple; chunks_exact_mut becomes chunks_mut.
//!
//! Checksums and hashes over byte slices, all with wrapping arithmetic and
//! bit rotation: CRC-32 with its table built by a `const fn` at compile
//! time, Adler-32, FNV-1a (32 and 64 bit) and SHA-256. They share a
//! streaming `Digest` trait with an associated output type and constant,
//! and provided methods (`chain`, `digest`) built on the required ones.
//! Checks known test vectors, that streaming in odd-sized chunks matches
//! one-shot hashing, then hashes 8 MiB of xorshift output as a benchmark.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

trait AsBytes {
    fn as_bytes(&self) -> &[u8];
}

impl<const N: usize> AsBytes for [u8; N] {
    fn as_bytes(&self) -> &[u8] {
        self
    }
}

trait Digest: Default {
    type Output: AsBytes + Copy;
    const NAME: &'static str;
    const OUTPUT_LEN: usize;

    fn update(&mut self, data: &[u8]);
    fn finalize(self) -> Self::Output;

    fn chain(mut self, data: &[u8]) -> Self
    where
        Self: Sized,
    {
        self.update(data);
        self
    }

    fn digest(data: &[u8]) -> Self::Output
    where
        Self: Sized,
    {
        Self::default().chain(data).finalize()
    }
}

const fn make_crc_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            k += 1;
        }
        table[i] = c;
        i += 1;
    }
    table
}

static CRC_TABLE: [u32; 256] = make_crc_table();

#[derive(Clone, Copy)]
struct Crc32(u32);

impl Default for Crc32 {
    fn default() -> Self {
        Crc32(!0)
    }
}

impl Digest for Crc32 {
    type Output = [u8; 4];
    const NAME: &'static str = "crc32";
    const OUTPUT_LEN: usize = 4;

    fn update(&mut self, data: &[u8]) {
        for &b in data {
            self.0 = CRC_TABLE[((self.0 ^ b as u32) & 0xff) as usize] ^ (self.0 >> 8);
        }
    }
    fn finalize(self) -> [u8; 4] {
        (!self.0).to_be_bytes()
    }
}

#[derive(Clone, Copy)]
struct Adler32 {
    a: u32,
    b: u32,
}

impl Default for Adler32 {
    fn default() -> Self {
        Adler32 { a: 1, b: 0 }
    }
}

impl Digest for Adler32 {
    type Output = [u8; 4];
    const NAME: &'static str = "adler32";
    const OUTPUT_LEN: usize = 4;

    fn update(&mut self, data: &[u8]) {
        const MOD: u32 = 65521;
        // Defer the modulo for blocks that cannot overflow.
        for block in data.chunks(5552) {
            for &x in block {
                self.a += x as u32;
                self.b += self.a;
            }
            self.a %= MOD;
            self.b %= MOD;
        }
    }
    fn finalize(self) -> [u8; 4] {
        ((self.b << 16) | self.a).to_be_bytes()
    }
}

macro_rules! fnv {
    ($name:ident, $ty:ty, $len:literal, $basis:expr, $prime:expr, $label:literal) => {
        #[derive(Clone, Copy)]
        struct $name($ty);

        impl Default for $name {
            fn default() -> Self {
                $name($basis)
            }
        }

        impl Digest for $name {
            type Output = [u8; $len];
            const NAME: &'static str = $label;
            const OUTPUT_LEN: usize = (<$ty>::BITS / 8) as usize;

            fn update(&mut self, data: &[u8]) {
                for &b in data {
                    self.0 = (self.0 ^ b as $ty).wrapping_mul($prime);
                }
            }
            fn finalize(self) -> Self::Output {
                self.0.to_be_bytes()
            }
        }
    };
}

fnv!(Fnv32, u32, 4, 0x811c_9dc5, 0x0100_0193, "fnv1a-32");
fnv!(Fnv64, u64, 8, 0xcbf2_9ce4_8422_2325, 0x0000_0100_0000_01b3, "fnv1a-64");

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

#[derive(Clone, Copy)]
struct Sha256 {
    state: [u32; 8],
    block: [u8; 64],
    filled: usize,
    total: u64,
}

impl Default for Sha256 {
    fn default() -> Self {
        Sha256 {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
            ],
            block: [0; 64],
            filled: 0,
            total: 0,
        }
    }
}

impl Sha256 {
    fn compress(&mut self) {
        let mut w = [0u32; 64];
        for (i, word) in self.block.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let s = self.state;
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h) = (s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7]);
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = h.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (s, v) in self.state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
            *s = s.wrapping_add(v);
        }
    }
}

impl Digest for Sha256 {
    type Output = [u8; 32];
    const NAME: &'static str = "sha256";
    const OUTPUT_LEN: usize = 32;

    fn update(&mut self, mut data: &[u8]) {
        self.total += data.len() as u64;
        while !data.is_empty() {
            let take = (64 - self.filled).min(data.len());
            self.block[self.filled..self.filled + take].copy_from_slice(&data[..take]);
            self.filled += take;
            data = &data[take..];
            if self.filled == 64 {
                self.compress();
                self.filled = 0;
            }
        }
    }

    fn finalize(mut self) -> [u8; 32] {
        let bit_len = self.total.wrapping_mul(8);
        self.update(&[0x80]);
        while self.filled != 56 {
            self.update(&[0]);
        }
        self.update(&bit_len.to_be_bytes());
        let mut out = [0u8; 32];
        for (chunk, word) in out.chunks_mut(4).zip(self.state.iter()) {
            chunk.copy_from_slice(&word.to_be_bytes());
        }
        out
    }
}

fn print_hex(bytes: &[u8]) {
    for b in bytes {
        unsafe { printf(c"%02x".as_ptr(), *b as u32) };
    }
}

fn check<D: Digest>(input: &[u8], expected_prefix: &str) -> bool {
    let out = D::digest(input);
    let bytes = out.as_bytes();
    assert_eq!(bytes.len(), D::OUTPUT_LEN);
    let mut hex = [0u8; 64];
    for (i, b) in bytes.iter().enumerate() {
        hex[2 * i] = b"0123456789abcdef"[(b >> 4) as usize];
        hex[2 * i + 1] = b"0123456789abcdef"[(b & 15) as usize];
    }
    let ok = hex.starts_with(expected_prefix.as_bytes());
    unsafe {
        printf(c"%-9.*s(%-14.*s) = ".as_ptr(), D::NAME.len() as i32, D::NAME.as_ptr(),
            input.len().min(14) as i32, input.as_ptr());
    }
    print_hex(bytes);
    unsafe { printf(c" %s\n".as_ptr(), if ok { c"ok".as_ptr() } else { c"MISMATCH".as_ptr() }) };
    ok
}

/// Streaming in uneven chunks must equal one-shot hashing.
fn streaming_matches<D: Digest>(data: &[u8]) -> bool
where
    D::Output: PartialEq,
{
    let mut d = D::default();
    let mut rest = data;
    let mut step = 1;
    while !rest.is_empty() {
        let n = step.min(rest.len());
        let (head, tail) = rest.split_at(n);
        d.update(head);
        rest = tail;
        step = step * 3 % 97 + 1;
    }
    d.finalize() == D::digest(data)
}

struct XorShift64(u64);

impl Iterator for XorShift64 {
    type Item = u64;
    fn next(&mut self) -> Option<u64> {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        Some(x)
    }
}

fn main() {
    let vectors: [&[u8]; 3] = [b"", b"abc", b"The quick brown fox jumps over the lazy dog"];
    let mut ok = 0;
    ok += check::<Crc32>(vectors[2], "414fa339") as u32;
    ok += check::<Crc32>(b"123456789", "cbf43926") as u32;
    ok += check::<Adler32>(b"Wikipedia", "11e60398") as u32;
    ok += check::<Fnv32>(vectors[0], "811c9dc5") as u32;
    ok += check::<Fnv32>(b"a", "e40c292c") as u32;
    ok += check::<Fnv64>(b"a", "af63dc4c8601ec8c") as u32;
    ok += check::<Sha256>(vectors[0], "e3b0c44298fc1c149afbf4c8996fb924") as u32;
    ok += check::<Sha256>(vectors[1], "ba7816bf8f01cfea414140de5dae2223") as u32;
    ok += check::<Sha256>(vectors[2], "d7a8fbb307d7809469ca9abcb0082e4f") as u32;
    unsafe { printf(c"%u/9 test vectors ok\n".as_ptr(), ok) };

    // 8 MiB of pseudo-random bytes, generated a block at a time.
    let mut rng = XorShift64(0x9E37_79B9_7F4A_7C15);
    let mut block = [0u8; 4096];
    let (mut crc, mut adler, mut fnv, mut sha) =
        (Crc32::default(), Adler32::default(), Fnv64::default(), Sha256::default());
    for round in 0..2048 {
        for (chunk, word) in block.chunks_mut(8).zip(rng.by_ref()) {
            chunk.copy_from_slice(&word.to_le_bytes());
        }
        if round == 0 {
            let all = streaming_matches::<Crc32>(&block)
                && streaming_matches::<Adler32>(&block)
                && streaming_matches::<Fnv32>(&block)
                && streaming_matches::<Sha256>(&block);
            unsafe { printf(c"streaming matches one-shot: %d\n".as_ptr(), all as i32) };
        }
        crc.update(&block);
        adler.update(&block);
        fnv.update(&block);
        sha.update(&block);
    }
    unsafe { printf(c"8 MiB: crc32 ".as_ptr()) };
    print_hex(&crc.finalize());
    unsafe { printf(c" adler32 ".as_ptr()) };
    print_hex(&adler.finalize());
    unsafe { printf(c" fnv64 ".as_ptr()) };
    print_hex(&fnv.finalize());
    unsafe { printf(c"\n       sha256 ".as_ptr()) };
    print_hex(&sha.finalize());
    unsafe { printf(c"\n".as_ptr()) };
}

//! LZ77-style compression with a fixed-size window and hash chains, no
//! heap: the encoder finds the longest earlier match through a hash of
//! the next three bytes, emits `Token`s (literals and back-references),
//! and packs them into a byte stream with a flag byte per eight tokens and
//! LEB128 varints. The decoder checks every reference and reports corrupt
//! input as an error. Roundtrips several inputs (repetitive text, a
//! generated "log file", random bytes, edge cases) and benchmarks on a
//! 256 KiB generated input.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

const WINDOW: usize = 1 << 15;
const MIN_MATCH: usize = 3;
const MAX_MATCH: usize = 258;
const HASH_BITS: u32 = 14;
const MAX_CHAIN: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Token {
    Literal(u8),
    Match { dist: u16, len: u16 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DecodeError {
    Truncated,
    BadDistance { at: usize, dist: usize },
    OutputFull,
}

struct Writer<'a> {
    buf: &'a mut [u8],
    len: usize,
}

impl<'a> Writer<'a> {
    fn byte(&mut self, b: u8) {
        self.buf[self.len] = b;
        self.len += 1;
    }

    fn varint(&mut self, mut v: u32) {
        loop {
            let low = (v & 0x7f) as u8;
            v >>= 7;
            if v == 0 {
                self.byte(low);
                return;
            }
            self.byte(low | 0x80);
        }
    }
}

struct Encoder {
    head: [u32; 1 << HASH_BITS],
    prev: [u32; WINDOW],
}

fn hash3(b: &[u8]) -> usize {
    let v = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
    (v.wrapping_mul(2654435761) >> (32 - HASH_BITS)) as usize
}

impl Encoder {
    fn new() -> Self {
        Encoder { head: [u32::MAX; 1 << HASH_BITS], prev: [u32::MAX; WINDOW] }
    }

    fn insert(&mut self, data: &[u8], pos: usize) {
        if pos + MIN_MATCH <= data.len() {
            let h = hash3(&data[pos..]);
            self.prev[pos % WINDOW] = self.head[h];
            self.head[h] = pos as u32;
        }
    }

    fn longest_match(&self, data: &[u8], pos: usize) -> Option<(usize, usize)> {
        if pos + MIN_MATCH > data.len() {
            return None;
        }
        let max_len = MAX_MATCH.min(data.len() - pos);
        let mut candidate = self.head[hash3(&data[pos..])];
        let mut best: Option<(usize, usize)> = None;
        for _ in 0..MAX_CHAIN {
            if candidate == u32::MAX {
                break;
            }
            let c = candidate as usize;
            if pos - c > WINDOW - 1 {
                break;
            }
            let len = data[c..].iter().zip(&data[pos..pos + max_len]).take_while(|(a, b)| a == b).count();
            if len >= MIN_MATCH && best.map_or(true, |(_, l)| len > l) {
                best = Some((pos - c, len));
                if len == max_len {
                    break;
                }
            }
            candidate = self.prev[c % WINDOW];
        }
        best
    }

    /// Tokenizes `data`, calling `emit` for each token.
    fn tokenize(&mut self, data: &[u8], mut emit: impl FnMut(Token)) {
        self.head.fill(u32::MAX);
        let mut pos = 0;
        while pos < data.len() {
            match self.longest_match(data, pos) {
                Some((dist, len)) => {
                    emit(Token::Match { dist: dist as u16, len: len as u16 });
                    for p in pos..pos + len {
                        self.insert(data, p);
                    }
                    pos += len;
                }
                None => {
                    emit(Token::Literal(data[pos]));
                    self.insert(data, pos);
                    pos += 1;
                }
            }
        }
    }

    /// Compresses into `out`, returning the compressed length.
    fn compress(&mut self, data: &[u8], out: &mut [u8]) -> usize {
        let mut w = Writer { buf: out, len: 0 };
        w.varint(data.len() as u32);
        let mut group = [Token::Literal(0); 8];
        let mut n = 0;
        let flush = |w: &mut Writer, group: &[Token]| {
            let flags = group.iter().enumerate().fold(0u8, |f, (i, t)| match t {
                Token::Match { .. } => f | 1 << i,
                Token::Literal(_) => f,
            });
            w.byte(flags);
            for t in group {
                match *t {
                    Token::Literal(b) => w.byte(b),
                    Token::Match { dist, len } => {
                        w.varint(dist as u32);
                        w.byte((len as usize - MIN_MATCH) as u8);
                    }
                }
            }
        };
        self.tokenize(data, |t| {
            group[n] = t;
            n += 1;
            if n == 8 {
                flush(&mut w, &group);
                n = 0;
            }
        });
        if n > 0 {
            flush(&mut w, &group[..n]);
        }
        w.len
    }
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn byte(&mut self) -> Result<u8, DecodeError> {
        let b = *self.data.get(self.pos).ok_or(DecodeError::Truncated)?;
        self.pos += 1;
        Ok(b)
    }

    fn varint(&mut self) -> Result<u32, DecodeError> {
        let mut v = 0u32;
        for shift in (0..35).step_by(7) {
            let b = self.byte()?;
            v |= ((b & 0x7f) as u32) << shift;
            if b & 0x80 == 0 {
                return Ok(v);
            }
        }
        Err(DecodeError::Truncated)
    }
}

fn decompress(input: &[u8], out: &mut [u8]) -> Result<usize, DecodeError> {
    let mut r = Reader { data: input, pos: 0 };
    let total = r.varint()? as usize;
    if total > out.len() {
        return Err(DecodeError::OutputFull);
    }
    let mut len = 0;
    while len < total {
        let flags = r.byte()?;
        for bit in 0..8 {
            if len == total {
                break;
            }
            if flags >> bit & 1 == 0 {
                out[len] = r.byte()?;
                len += 1;
            } else {
                let dist = r.varint()? as usize;
                let n = r.byte()? as usize + MIN_MATCH;
                if dist == 0 || dist > len {
                    return Err(DecodeError::BadDistance { at: len, dist });
                }
                if len + n > total {
                    return Err(DecodeError::OutputFull);
                }
                // Byte by byte: references may overlap their own output.
                for i in 0..n {
                    out[len + i] = out[len + i - dist];
                }
                len += n;
            }
        }
    }
    Ok(len)
}

struct Lcg(u32);

impl Lcg {
    fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(1_103_515_245).wrapping_add(12345);
        self.0 >> 16
    }
}

fn make_log(buf: &mut [u8], seed: u32) -> usize {
    const LEVELS: [&str; 3] = ["INFO", "WARN", "DEBUG"];
    const WORDS: [&str; 8] = ["request", "served", "cache", "miss", "user", "login", "timeout", "retry"];
    let mut rng = Lcg(seed);
    let cap = buf.len();
    let mut len = 0;
    let mut push = |s: &[u8], len: &mut usize| {
        let n = s.len().min(buf.len() - *len);
        buf[*len..*len + n].copy_from_slice(&s[..n]);
        *len += n;
    };
    let mut line = 0u32;
    while len < cap {
        let mut num = [0u8; 10];
        let mut k = num.len();
        let mut v = line;
        loop {
            k -= 1;
            num[k] = b'0' + (v % 10) as u8;
            v /= 10;
            if v == 0 {
                break;
            }
        }
        push(&num[k..], &mut len);
        push(b" [", &mut len);
        push(LEVELS[rng.next() as usize % 3].as_bytes(), &mut len);
        push(b"] ", &mut len);
        for _ in 0..(2 + rng.next() % 5) {
            push(WORDS[rng.next() as usize % WORDS.len()].as_bytes(), &mut len);
            push(b" ", &mut len);
        }
        push(b"\n", &mut len);
        line += 1;
    }
    len
}

fn roundtrip(name: &str, data: &[u8], enc: &mut Encoder, scratch: &mut [u8], back: &mut [u8]) -> bool {
    let clen = enc.compress(data, scratch);
    let result = decompress(&scratch[..clen], back);
    let ok = matches!(result, Ok(n) if n == data.len() && back[..n] == *data);
    let ratio = if data.is_empty() { 0 } else { clen * 1000 / data.len() };
    unsafe {
        printf(c"%-10.*s %7zu -> %7zu (%3zu.%zu%%) %s\n".as_ptr(), name.len() as i32, name.as_ptr(),
            data.len(), clen, ratio / 10, ratio % 10, if ok { c"ok".as_ptr() } else { c"FAILED".as_ptr() });
    }
    ok
}

static mut BIG: [u8; 1 << 18] = [0; 1 << 18];
static mut SCRATCH: [u8; 1 << 19] = [0; 1 << 19];
static mut BACK: [u8; 1 << 18] = [0; 1 << 18];
static mut ENCODER: Option<Encoder> = None;

fn main() {
    // SAFETY: single-threaded; each static is borrowed once, here.
    let (big, scratch, back, enc) = unsafe {
        ENCODER = Some(Encoder::new());
        (
            &mut *core::ptr::addr_of_mut!(BIG),
            &mut *core::ptr::addr_of_mut!(SCRATCH),
            &mut *core::ptr::addr_of_mut!(BACK),
            (*core::ptr::addr_of_mut!(ENCODER)).as_mut().unwrap(),
        )
    };

    let mut tokens = [Token::Literal(0); 16];
    let mut n = 0;
    enc.tokenize(b"abcabcabcabcX", |t| {
        tokens[n] = t;
        n += 1;
    });
    unsafe { printf(c"tokens of abcabcabcabcX:".as_ptr()) };
    for t in &tokens[..n] {
        match t {
            Token::Literal(b) => unsafe { printf(c" '%c'".as_ptr(), *b as i32); },
            Token::Match { dist, len } => unsafe { printf(c" <%u,%u>".as_ptr(), *dist as u32, *len as u32); },
        }
    }
    unsafe { printf(c"\n".as_ptr()) };

    let mut all_ok = true;
    all_ok &= roundtrip("empty", b"", enc, scratch, back);
    all_ok &= roundtrip("one", b"x", enc, scratch, back);
    all_ok &= roundtrip("runs", &[b'a'; 1000], enc, scratch, back);
    all_ok &= roundtrip("pangram", b"the quick brown fox jumps over the lazy dog; the quick brown fox jumps again", enc, scratch, back);

    let mut rng = Lcg(99);
    let mut random = [0u8; 4096];
    random.iter_mut().for_each(|b| *b = rng.next() as u8);
    all_ok &= roundtrip("random", &random, enc, scratch, back);

    let n = make_log(&mut big[..20_000], 1);
    all_ok &= roundtrip("log 20k", &big[..n], enc, scratch, back);

    // Corrupt streams are errors, never panics.
    let clen = enc.compress(b"hello hello hello hello", scratch);
    let mut bad = [0u8; 64];
    bad[..clen].copy_from_slice(&scratch[..clen]);
    let truncated = decompress(&bad[..clen - 2], back);
    bad[clen - 2] = 0x7f; // a distance past the start
    let corrupted = decompress(&bad[..clen], back);
    let too_big = decompress(&[0xff, 0xff, 0xff, 0x7f], &mut back[..16]);
    unsafe {
        printf(c"truncated %d, bad distance %d, too big %d\n".as_ptr(),
            (truncated == Err(DecodeError::Truncated)) as i32,
            matches!(corrupted, Err(DecodeError::BadDistance { .. })) as i32,
            (too_big == Err(DecodeError::OutputFull)) as i32);
    }

    // Benchmark: 256 KiB of log text, compressed and checked three times.
    let n = make_log(big, 7);
    let mut total = 0;
    for _ in 0..3 {
        all_ok &= roundtrip("log 256k", &big[..n], enc, scratch, back);
        total += n;
    }
    unsafe { printf(c"processed %zu bytes, all ok %d\n".as_ptr(), total, all_ok as i32) };
    unsafe { exit(if all_ok { 0 } else { 1 }) }
}

unsafe extern "C" {
    fn exit(code: i32) -> !;
}

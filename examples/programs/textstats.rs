//! Text statistics over stdin: line, word and sentence counts, letter
//! frequencies in a `[u32; 26]` with a bar chart of the most common
//! letters, the longest word, a word-length histogram, a palindrome
//! finder and a simple Caesar-cipher round trip.
//!
//! Rewritten for flux from `original/textstats.rs`, which works on a
//! `static TEXT: &[u8] = b"..."`: no byte strings or byte literals (the
//! text is read from stdin, characters are constants), no range slicing
//! (words are `Span`s of indices into the text), no `u8` ASCII methods,
//! `core::array::from_fn`, `sort_by`, `matches!`, `iter_mut`, slice `==`
//! or `as_ptr` on slices (bytes are printed with `putchar`), and array
//! methods called through slices.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
    fn putchar(c: i32) -> i32;
    fn getchar() -> i32;
}

const NEWLINE: u8 = 10;
const APOSTROPHE: u8 = 39;
const COMMA: u8 = 44;
const PERIOD: u8 = 46;
const BANG: u8 = 33;
const QUESTION: u8 = 63;
const UPPER_A: u8 = 65;
const UPPER_Z: u8 = 90;
const LOWER_A: u8 = 97;
const LOWER_Z: u8 = 122;
const THE: [u8; 3] = [116, 104, 101];

const MAX_TEXT: usize = 4096;

fn is_alphabetic(b: u8) -> bool {
    match b {
        UPPER_A..=UPPER_Z | LOWER_A..=LOWER_Z => true,
        _ => false,
    }
}

fn to_lowercase(b: u8) -> u8 {
    match b {
        UPPER_A..=UPPER_Z => b + 32,
        _ => b,
    }
}

fn is_word_byte(b: u8) -> bool {
    is_alphabetic(b) || b == APOSTROPHE
}

/// A word: the bytes `start..end` of the text.
#[derive(Clone, Copy, PartialEq)]
struct Span {
    start: usize,
    end: usize,
}

impl Span {
    fn len(&self) -> usize {
        self.end - self.start
    }
}

/// An iterator over the words of a byte string.
struct Words<'a> {
    text: &'a [u8],
    pos: usize,
}

impl<'a> Iterator for Words<'a> {
    type Item = Span;

    fn next(&mut self) -> Option<Span> {
        while self.pos < self.text.len() && !is_word_byte(self.text[self.pos]) {
            self.pos += 1;
        }
        if self.pos == self.text.len() {
            return None;
        }
        let start = self.pos;
        while self.pos < self.text.len() && is_word_byte(self.text[self.pos]) {
            self.pos += 1;
        }
        Some(Span { start, end: self.pos })
    }
}

fn words(text: &[u8]) -> Words<'_> {
    Words { text, pos: 0 }
}

fn letter_counts(text: &[u8]) -> [u32; 26] {
    let mut counts = [0u32; 26];
    for &b in text {
        if is_alphabetic(b) {
            counts[(to_lowercase(b) - LOWER_A) as usize] += 1;
        }
    }
    counts
}

fn is_palindrome(text: &[u8], word: Span) -> bool {
    word.len() > 2
        && (0..word.len() / 2).all(|i| to_lowercase(text[word.start + i]) == to_lowercase(text[word.end - 1 - i]))
}

fn eq_ignore_case(text: &[u8], word: Span, other: &[u8]) -> bool {
    word.len() == other.len() && (0..other.len()).all(|i| to_lowercase(text[word.start + i]) == other[i])
}

fn longest_word(text: &[u8]) -> Option<Span> {
    words(text).max_by_key(|w| w.len())
}

fn caesar(text: &[u8], span: Span, shift: u8, out: &mut [u8]) {
    for i in 0..span.len() {
        let b = text[span.start + i];
        out[i] = match b {
            LOWER_A..=LOWER_Z => (b - LOWER_A + shift) % 26 + LOWER_A,
            UPPER_A..=UPPER_Z => (b - UPPER_A + shift) % 26 + UPPER_A,
            _ => b,
        };
    }
}

fn print_bytes(label: *const i8, bytes: &[u8], span: Span) {
    unsafe { printf(c"%s".as_ptr(), label) };
    for i in span.start..span.end {
        unsafe { putchar(bytes[i] as i32) };
    }
    unsafe { putchar(NEWLINE as i32) };
}

/// Sorts `v` by a comparator (insertion sort, stable).
fn sort_by<T: Copy, F: Fn(&T, &T) -> core::cmp::Ordering>(v: &mut [T], compare: F) {
    for i in 1..v.len() {
        let x = v[i];
        let mut j = i;
        while j > 0 && compare(&v[j - 1], &x).is_gt() {
            v[j] = v[j - 1];
            j -= 1;
        }
        v[j] = x;
    }
}

fn main() {
    let mut buffer = [0u8; MAX_TEXT];
    let mut len = 0;
    loop {
        let c = unsafe { getchar() };
        if c < 0 || len == MAX_TEXT {
            break;
        }
        buffer[len] = c as u8;
        len += 1;
    }
    analyze(&buffer, len);
}

fn analyze(buffer: &[u8; MAX_TEXT], len: usize) {
    // Everything past `len` is zero, which is not a word byte, so words
    // and letters see only the text; counts of bytes stop at `len`.
    let text: &[u8] = buffer;
    let bytes = text.iter().take(len);
    let lines = bytes.filter(|&&b| b == NEWLINE).count() + 1;
    let word_count = words(text).count();
    let sentences = text
        .iter()
        .take(len)
        .filter(|b| match b {
            &&PERIOD | &&BANG | &&QUESTION => true,
            _ => false,
        })
        .count();
    let counts = letter_counts(text);
    let counts_slice: &[u32] = &counts;
    let letters: u32 = counts_slice.iter().map(|c| *c).sum();
    unsafe {
        printf(c"%zu bytes, %zu lines, %zu words, %zu sentences, %u letters\n".as_ptr(), len, lines, word_count,
            sentences, letters);
    }

    let mut order = [0usize; 26];
    for i in 0..26 {
        order[i] = i;
    }
    sort_by(&mut order, |&a, &b| counts[b].cmp(&counts[a]).then(a.cmp(&b)));
    let top = counts[order[0]];
    let order_slice: &[usize] = &order;
    for &i in order_slice.iter().take(8) {
        let bar = counts[i] * 40 / top;
        unsafe { printf(c"%c %3u ".as_ptr(), (LOWER_A + i as u8) as i32, counts[i]) };
        for _ in 0..bar {
            unsafe { printf(c"#".as_ptr()) };
        }
        unsafe { printf(c"\n".as_ptr()) };
    }
    let missing: u32 = counts_slice.iter().filter(|&&c| c == 0).count() as u32;
    unsafe { printf(c"%u letters never appear\n".as_ptr(), missing) };

    if let Some(w) = longest_word(text) {
        print_bytes(c"longest word: ".as_ptr(), text, w);
    }

    let mut histogram = [0u32; 16];
    for w in words(text) {
        histogram[w.len().min(15)] += 1;
    }
    let histogram_slice: &[u32] = &histogram;
    for (len, &n) in histogram_slice.iter().enumerate().skip(1).filter(|&(_, &n)| n > 0) {
        unsafe { printf(c"length %2zu: %u\n".as_ptr(), len, n) };
    }

    let mut palindromes = 0;
    for w in words(text).filter(|w| is_palindrome(text, *w)) {
        palindromes += 1;
        print_bytes(c"palindrome: ".as_ptr(), text, w);
    }
    let the_count = words(text).filter(|w| eq_ignore_case(text, *w, &THE)).count();
    unsafe { printf(c"%d palindromes, 'the' x%zu\n".as_ptr(), palindromes, the_count) };

    let comma = text.iter().position(|&b| b == COMMA).unwrap();
    let sentence = Span { start: 0, end: comma };
    let mut encoded = [0u8; 64];
    let mut decoded = [0u8; 64];
    let n = sentence.len();
    caesar(text, sentence, 13, &mut encoded);
    caesar(&encoded, Span { start: 0, end: n }, 13, &mut decoded);
    print_bytes(c"rot13: ".as_ptr(), &encoded, Span { start: 0, end: n });
    let round_trip = (0..n).all(|i| decoded[i] == text[i]);
    unsafe { printf(c"round trip ok: %d\n".as_ptr(), round_trip as i32) };
}

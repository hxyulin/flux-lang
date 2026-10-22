//! Text statistics over a static byte string: line, word and sentence
//! counts, letter frequencies in a `[u32; 26]` with a bar chart of the
//! most common letters, the longest word, a word-length histogram, a
//! palindrome finder and a simple Caesar-cipher round trip.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

static TEXT: &[u8] = b"It was the best of times, it was the worst of times, it was the age of
wisdom, it was the age of foolishness, it was the epoch of belief, it was the
epoch of incredulity, it was the season of Light, it was the season of Darkness,
it was the spring of hope, it was the winter of despair. A man, a plan, a canal:
Panama! Was it a car or a cat I saw? Madam, level the radar at noon. Kayak racecar
civic refer rotor stats tenet. Never odd or even.";

fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'\''
}

/// An iterator over the words of a byte string.
struct Words<'a> {
    text: &'a [u8],
    pos: usize,
}

impl<'a> Iterator for Words<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<&'a [u8]> {
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
        Some(&self.text[start..self.pos])
    }
}

fn words(text: &[u8]) -> Words<'_> {
    Words { text, pos: 0 }
}

fn letter_counts(text: &[u8]) -> [u32; 26] {
    let mut counts = [0u32; 26];
    for &b in text {
        if b.is_ascii_alphabetic() {
            counts[(b.to_ascii_lowercase() - b'a') as usize] += 1;
        }
    }
    counts
}

fn is_palindrome(word: &[u8]) -> bool {
    word.len() > 2
        && word.iter().zip(word.iter().rev()).all(|(a, b)| a.eq_ignore_ascii_case(b))
}

fn longest_word(text: &[u8]) -> Option<&[u8]> {
    words(text).max_by_key(|w| w.len())
}

fn caesar(text: &[u8], shift: u8, out: &mut [u8]) {
    for (o, &b) in out.iter_mut().zip(text) {
        *o = match b {
            b'a'..=b'z' => (b - b'a' + shift) % 26 + b'a',
            b'A'..=b'Z' => (b - b'A' + shift) % 26 + b'A',
            _ => b,
        };
    }
}

fn print_bytes(label: &core::ffi::CStr, bytes: &[u8]) {
    unsafe { printf(c"%s%.*s\n".as_ptr(), label.as_ptr(), bytes.len() as i32, bytes.as_ptr()) };
}

fn main() {
    let lines = TEXT.iter().filter(|&&b| b == b'\n').count() + 1;
    let word_count = words(TEXT).count();
    let sentences = TEXT.iter().filter(|b| matches!(b, b'.' | b'!' | b'?')).count();
    let letters: u32 = letter_counts(TEXT).iter().sum();
    unsafe {
        printf(c"%zu bytes, %zu lines, %zu words, %zu sentences, %u letters\n".as_ptr(), TEXT.len(), lines,
            word_count, sentences, letters);
    }

    let counts = letter_counts(TEXT);
    let mut order: [usize; 26] = core::array::from_fn(|i| i);
    order.sort_by(|&a, &b| counts[b].cmp(&counts[a]).then(a.cmp(&b)));
    let top = counts[order[0]];
    for &i in &order[..8] {
        let bar = counts[i] * 40 / top;
        unsafe { printf(c"%c %3u ".as_ptr(), (b'a' + i as u8) as i32, counts[i]) };
        for _ in 0..bar {
            unsafe { printf(c"#".as_ptr()) };
        }
        unsafe { printf(c"\n".as_ptr()) };
    }
    let missing: u32 = counts.iter().filter(|&&c| c == 0).count() as u32;
    unsafe { printf(c"%u letters never appear\n".as_ptr(), missing) };

    if let Some(w) = longest_word(TEXT) {
        print_bytes(c"longest word: ", w);
    }

    let mut histogram = [0u32; 16];
    for w in words(TEXT) {
        histogram[w.len().min(15)] += 1;
    }
    for (len, &n) in histogram.iter().enumerate().skip(1).filter(|&(_, &n)| n > 0) {
        unsafe { printf(c"length %2zu: %u\n".as_ptr(), len, n) };
    }

    let mut palindromes = 0;
    for w in words(TEXT).filter(|w| is_palindrome(w)) {
        palindromes += 1;
        print_bytes(c"palindrome: ", w);
    }
    let the_count = words(TEXT).filter(|w| w.eq_ignore_ascii_case(b"the")).count();
    unsafe { printf(c"%d palindromes, 'the' x%zu\n".as_ptr(), palindromes, the_count) };

    let sentence = &TEXT[..TEXT.iter().position(|&b| b == b',').unwrap()];
    let mut encoded = [0u8; 64];
    let mut decoded = [0u8; 64];
    let n = sentence.len();
    caesar(sentence, 13, &mut encoded[..n]);
    caesar(&encoded[..n], 13, &mut decoded[..n]);
    print_bytes(c"rot13: ", &encoded[..n]);
    unsafe { printf(c"round trip ok: %d\n".as_ptr(), (&decoded[..n] == sentence) as i32) };
}

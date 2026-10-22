//! flux variant of ../backtracking.rs, minimally rewritten to compile with flux:
//! bool::then_some becomes if/else.
//!
//! Backtracking search with bitsets: a const-generic `BitSet<W>` of `W`
//! 64-bit words with the bitwise operator traits, `insert`/`remove`/
//! `contains`, `len` through `count_ones` and an iterator over set bits
//! using `trailing_zeros`; N-queens counted for n = 1..=12 with bitmask
//! recursion (and one board printed), and a sudoku solver that keeps a
//! candidate bitset per row, column and box, picks the most constrained
//! cell first, and counts the guesses it makes.

use core::ops::{BitAnd, BitOr, BitXor, Not};

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct BitSet<const W: usize> {
    words: [u64; W],
}

impl<const W: usize> BitSet<W> {
    const CAPACITY: usize = W * 64;

    const fn empty() -> Self {
        BitSet { words: [0; W] }
    }

    fn full(n: usize) -> Self {
        let mut s = Self::empty();
        for i in 0..n {
            s.insert(i);
        }
        s
    }

    fn insert(&mut self, i: usize) -> bool {
        assert!(i < Self::CAPACITY);
        let (w, b) = (i / 64, i % 64);
        let was = self.words[w] >> b & 1 == 1;
        self.words[w] |= 1 << b;
        !was
    }

    fn remove(&mut self, i: usize) {
        self.words[i / 64] &= !(1 << (i % 64));
    }

    fn contains(&self, i: usize) -> bool {
        i < Self::CAPACITY && self.words[i / 64] >> (i % 64) & 1 == 1
    }

    fn len(&self) -> u32 {
        self.words.iter().map(|w| w.count_ones()).sum()
    }

    fn is_empty(&self) -> bool {
        self.words.iter().all(|&w| w == 0)
    }

    fn first(&self) -> Option<usize> {
        self.iter().next()
    }

    fn iter(&self) -> Bits<W> {
        Bits { words: self.words, word: 0 }
    }
}

struct Bits<const W: usize> {
    words: [u64; W],
    word: usize,
}

impl<const W: usize> Iterator for Bits<W> {
    type Item = usize;
    fn next(&mut self) -> Option<usize> {
        while self.word < W {
            let w = &mut self.words[self.word];
            if *w != 0 {
                let bit = w.trailing_zeros() as usize;
                *w &= *w - 1;
                return Some(self.word * 64 + bit);
            }
            self.word += 1;
        }
        None
    }
}

impl<const W: usize> IntoIterator for &BitSet<W> {
    type Item = usize;
    type IntoIter = Bits<W>;
    fn into_iter(self) -> Bits<W> {
        self.iter()
    }
}

macro_rules! bitop {
    ($tr:ident, $method:ident, $op:tt) => {
        impl<const W: usize> $tr for BitSet<W> {
            type Output = Self;
            fn $method(self, rhs: Self) -> Self {
                let mut out = self;
                for (a, b) in out.words.iter_mut().zip(rhs.words) {
                    *a = *a $op b;
                }
                out
            }
        }
    };
}

bitop!(BitAnd, bitand, &);
bitop!(BitOr, bitor, |);
bitop!(BitXor, bitxor, ^);

impl<const W: usize> Not for BitSet<W> {
    type Output = Self;
    fn not(self) -> Self {
        BitSet { words: self.words.map(|w| !w) }
    }
}

// ---- N-queens ----

fn queens(n: u32, cols: u32, diag1: u32, diag2: u32) -> u64 {
    let all = (1u32 << n) - 1;
    if cols == all {
        return 1;
    }
    let mut free = all & !(cols | diag1 | diag2);
    let mut count = 0;
    while free != 0 {
        let bit = free & free.wrapping_neg();
        free ^= bit;
        count += queens(n, cols | bit, (diag1 | bit) << 1, (diag2 | bit) >> 1);
    }
    count
}

/// The first solution in lexicographic order, as a column per row.
fn first_solution<const N: usize>() -> Option<[usize; N]> {
    fn place<const N: usize>(row: usize, board: &mut [usize; N]) -> bool {
        if row == N {
            return true;
        }
        for col in 0..N {
            let safe = (0..row).all(|r| {
                let c = board[r];
                c != col && c.abs_diff(col) != row - r
            });
            if safe {
                board[row] = col;
                if place(row + 1, board) {
                    return true;
                }
            }
        }
        false
    }
    let mut board = [0; N];
    if place(0, &mut board) { Some(board) } else { None }
}

// ---- Sudoku ----

type Set = BitSet<1>;

struct Sudoku {
    cells: [u8; 81],
    rows: [Set; 9],
    cols: [Set; 9],
    boxes: [Set; 9],
    guesses: u32,
}

impl Sudoku {
    fn parse(s: &str) -> Option<Sudoku> {
        let mut sudoku = Sudoku {
            cells: [0; 81],
            rows: [Set::empty(); 9],
            cols: [Set::empty(); 9],
            boxes: [Set::empty(); 9],
            guesses: 0,
        };
        let digits = s.bytes().filter(|b| !b.is_ascii_whitespace());
        let mut n = 0;
        for (i, b) in digits.enumerate() {
            let d = match b {
                b'.' | b'0' => 0,
                b'1'..=b'9' => b - b'0',
                _ => return None,
            };
            if i >= 81 {
                return None;
            }
            if d != 0 && !sudoku.place(i, d) {
                return None;
            }
            n += 1;
        }
        if n == 81 { Some(sudoku) } else { None }
    }

    fn box_of(i: usize) -> usize {
        (i / 27) * 3 + (i % 9) / 3
    }

    fn candidates(&self, i: usize) -> Set {
        let used = self.rows[i / 9] | self.cols[i % 9] | self.boxes[Self::box_of(i)];
        !used & Set::full(10) & !Set::full(1)
    }

    fn place(&mut self, i: usize, d: u8) -> bool {
        if !self.candidates(i).contains(d as usize) {
            return false;
        }
        self.cells[i] = d;
        self.rows[i / 9].insert(d as usize);
        self.cols[i % 9].insert(d as usize);
        self.boxes[Self::box_of(i)].insert(d as usize);
        true
    }

    fn clear(&mut self, i: usize) {
        let d = core::mem::take(&mut self.cells[i]) as usize;
        self.rows[i / 9].remove(d);
        self.cols[i % 9].remove(d);
        self.boxes[Self::box_of(i)].remove(d);
    }

    fn solve(&mut self) -> bool {
        // The empty cell with the fewest candidates.
        let best = (0..81)
            .filter(|&i| self.cells[i] == 0)
            .map(|i| (self.candidates(i).len(), i))
            .min();
        let Some((count, i)) = best else { return true };
        if count == 0 {
            return false;
        }
        if count > 1 {
            self.guesses += count - 1;
        }
        for d in &self.candidates(i) {
            self.place(i, d as u8);
            if self.solve() {
                return true;
            }
            self.clear(i);
        }
        false
    }

    fn print(&self) {
        for (r, row) in self.cells.chunks(9).enumerate() {
            if r % 3 == 0 && r > 0 {
                unsafe { printf(c"------+-------+------\n".as_ptr()) };
            }
            for (c, d) in row.iter().enumerate() {
                let sep = if c == 8 { c"\n" } else if c % 3 == 2 { c" | " } else { c" " };
                unsafe { printf(c"%d%s".as_ptr(), *d as i32, sep.as_ptr()) };
            }
        }
    }

    fn is_valid_solution(&self) -> bool {
        let all = Set::full(10) & !Set::full(1);
        self.rows.iter().chain(&self.cols).chain(&self.boxes).all(|s| *s == all)
    }
}

const PUZZLES: [&str; 3] = [
    "53..7.... 6..195... .98....6. 8...6...3 4..8.3..1 7...2...6 .6....28. ...419..5 ....8..79",
    // "Arto Inkala's hardest sudoku".
    "8........ ..36..... .7..9.2.. .5...7... ....457.. ...1...3. ..1....68 ..85...1. .9....4..",
    "11....... ......... ......... ......... ......... ......... ......... ......... .........",
];

fn main() {
    // Bitset basics.
    let mut a: BitSet<2> = BitSet::empty();
    for i in [0, 3, 64, 100, 127] {
        a.insert(i);
    }
    let mut b: BitSet<2> = BitSet::empty();
    for i in (0..128).step_by(3) {
        b.insert(i);
    }
    let both = a & b;
    let either = a | b;
    unsafe {
        printf(c"a has %u, b has %u, a&b has %u, a|b has %u, a^b has %u, !a has %u; a&b =".as_ptr(),
            a.len(), b.len(), both.len(), either.len(), (a ^ b).len(), (!a).len());
    }
    for i in &both {
        unsafe { printf(c" %zu".as_ptr(), i) };
    }
    unsafe { printf(c"; first of b above 64: %zu\n".as_ptr(), (b & !BitSet::<2>::full(65)).first().unwrap()) };
    assert!(!a.contains(1) && a.contains(127) && !a.contains(500) && BitSet::<3>::empty().is_empty());

    // N-queens counts.
    let mut total = 0;
    for n in 1..=12 {
        let c = queens(n, 0, 0, 0);
        total += c;
        unsafe { printf(c"%u-queens: %llu\n".as_ptr(), n, c) };
    }
    if let Some(board) = first_solution::<8>() {
        for &col in &board {
            for c in 0..8 {
                unsafe { printf(if c == col { c"Q ".as_ptr() } else { c". ".as_ptr() }) };
            }
            unsafe { printf(c"\n".as_ptr()) };
        }
    }
    assert!(first_solution::<3>().is_none());

    // Sudoku.
    for (k, p) in PUZZLES.iter().enumerate() {
        match Sudoku::parse(p) {
            None => unsafe { printf(c"puzzle %zu: invalid\n".as_ptr(), k); },
            Some(mut s) => {
                let givens = s.cells.iter().filter(|&&c| c != 0).count();
                if s.solve() {
                    unsafe { printf(c"puzzle %zu: %zu givens, solved with %u guesses, valid %d\n".as_ptr(),
                        k, givens, s.guesses, s.is_valid_solution() as i32) };
                    s.print();
                } else {
                    unsafe { printf(c"puzzle %zu: no solution\n".as_ptr(), k) };
                }
            }
        }
    }
    unsafe { exit((total % 256) as i32) }
}

unsafe extern "C" {
    fn exit(code: i32) -> !;
}

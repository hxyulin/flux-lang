//! Sorting algorithms on slices: insertion sort, quicksort (Lomuto, with
//! an insertion-sort cutoff), merge sort with a scratch buffer and heap
//! sort, generic over `T: Ord + Copy`, checked against each other on
//! pseudo-random data from a xorshift generator.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

const N: usize = 200;

struct XorShift(u64);

impl XorShift {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound
    }
}

fn is_sorted<T: Ord>(v: &[T]) -> bool {
    v.windows(2).all(|w| w[0] <= w[1])
}

fn insertion_sort<T: Ord + Copy>(v: &mut [T]) {
    for i in 1..v.len() {
        let x = v[i];
        let mut j = i;
        while j > 0 && v[j - 1] > x {
            v[j] = v[j - 1];
            j -= 1;
        }
        v[j] = x;
    }
}

fn partition<T: Ord + Copy>(v: &mut [T]) -> usize {
    let last = v.len() - 1;
    // Median of three into the last slot.
    let mid = v.len() / 2;
    if v[mid] < v[0] {
        v.swap(mid, 0);
    }
    if v[last] < v[0] {
        v.swap(last, 0);
    }
    if v[mid] < v[last] {
        v.swap(mid, last);
    }
    let pivot = v[last];
    let mut store = 0;
    for i in 0..last {
        if v[i] < pivot {
            v.swap(i, store);
            store += 1;
        }
    }
    v.swap(store, last);
    store
}

fn quicksort<T: Ord + Copy>(v: &mut [T]) {
    if v.len() <= 8 {
        insertion_sort(v);
        return;
    }
    let p = partition(v);
    let (left, right) = v.split_at_mut(p);
    quicksort(left);
    quicksort(&mut right[1..]);
}

fn merge<T: Ord + Copy>(v: &mut [T], mid: usize, scratch: &mut [T]) {
    let (mut i, mut j, mut k) = (0, mid, 0);
    while i < mid && j < v.len() {
        if v[j] < v[i] {
            scratch[k] = v[j];
            j += 1;
        } else {
            scratch[k] = v[i];
            i += 1;
        }
        k += 1;
    }
    while i < mid {
        scratch[k] = v[i];
        i += 1;
        k += 1;
    }
    while j < v.len() {
        scratch[k] = v[j];
        j += 1;
        k += 1;
    }
    v.copy_from_slice(&scratch[..v.len()]);
}

fn merge_sort<T: Ord + Copy>(v: &mut [T], scratch: &mut [T]) {
    if v.len() <= 1 {
        return;
    }
    let mid = v.len() / 2;
    merge_sort(&mut v[..mid], scratch);
    merge_sort(&mut v[mid..], scratch);
    merge(v, mid, scratch);
}

fn sift_down<T: Ord + Copy>(v: &mut [T], mut root: usize, end: usize) {
    loop {
        let mut child = 2 * root + 1;
        if child >= end {
            return;
        }
        if child + 1 < end && v[child] < v[child + 1] {
            child += 1;
        }
        if v[root] >= v[child] {
            return;
        }
        v.swap(root, child);
        root = child;
    }
}

fn heap_sort<T: Ord + Copy>(v: &mut [T]) {
    let n = v.len();
    for i in (0..n / 2).rev() {
        sift_down(v, i, n);
    }
    for end in (1..n).rev() {
        v.swap(0, end);
        sift_down(v, 0, end);
    }
}

fn checksum(v: &[i64]) -> i64 {
    v.iter().enumerate().fold(0i64, |acc, (i, &x)| acc.wrapping_mul(31).wrapping_add(x ^ i as i64))
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Card {
    rank: u8,
    suit: u8,
}

fn main() {
    let mut rng = XorShift(0x2545F4914F6CDD1D);
    let mut data = [0i64; N];
    for x in data.iter_mut() {
        *x = rng.below(1000) as i64 - 500;
    }

    let mut a = data;
    insertion_sort(&mut a);
    let mut b = data;
    quicksort(&mut b);
    let mut c = data;
    let mut scratch = [0i64; N];
    merge_sort(&mut c, &mut scratch);
    let mut d = data;
    heap_sort(&mut d);

    unsafe {
        printf(c"sorted: %d %d %d %d\n".as_ptr(), is_sorted(&a) as i32, is_sorted(&b) as i32,
            is_sorted(&c) as i32, is_sorted(&d) as i32);
        printf(c"agree: %d\n".as_ptr(), (a == b && b == c && c == d) as i32);
        printf(c"min %lld max %lld median %lld\n".as_ptr(), a[0], a[N - 1], a[N / 2]);
        printf(c"checksum %lld\n".as_ptr(), checksum(&a));
    }
    for x in &a[..10] {
        unsafe { printf(c"%lld ".as_ptr(), *x) };
    }
    unsafe { printf(c"\n".as_ptr()) };

    // Sorting a struct with a derived ordering.
    let mut hand = [Card { rank: 0, suit: 0 }; 13];
    for card in hand.iter_mut() {
        *card = Card { rank: rng.below(13) as u8 + 2, suit: rng.below(4) as u8 };
    }
    quicksort(&mut hand);
    let distinct = hand.windows(2).filter(|w| w[0] != w[1]).count() + 1;
    for card in &hand {
        unsafe { printf(c"%d/%d ".as_ptr(), card.rank as i32, card.suit as i32) };
    }
    unsafe { printf(c"\ndistinct %zu sorted %d\n".as_ptr(), distinct, is_sorted(&hand) as i32) };
}

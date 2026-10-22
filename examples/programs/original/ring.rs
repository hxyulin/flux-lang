//! A fixed-capacity ring buffer generic over `T: Copy`, with push that
//! overwrites the oldest element, pop from both ends, indexing, an
//! iterator over `&RingBuffer`, and a moving-average filter and a
//! "last N commands" history built on top of it.

use core::ops::Index;

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

const CAP: usize = 8;

#[derive(Clone, Copy)]
struct RingBuffer<T: Copy> {
    items: [T; CAP],
    head: usize,
    len: usize,
}

impl<T: Copy + Default> RingBuffer<T> {
    fn new() -> Self {
        RingBuffer { items: [T::default(); CAP], head: 0, len: 0 }
    }
}

impl<T: Copy> RingBuffer<T> {
    fn len(&self) -> usize {
        self.len
    }

    fn is_full(&self) -> bool {
        self.len == CAP
    }

    /// Appends at the back; when full, overwrites and returns the oldest.
    fn push(&mut self, value: T) -> Option<T> {
        let tail = (self.head + self.len) % CAP;
        if self.is_full() {
            let old = self.items[self.head];
            self.items[tail] = value;
            self.head = (self.head + 1) % CAP;
            Some(old)
        } else {
            self.items[tail] = value;
            self.len += 1;
            None
        }
    }

    fn pop_front(&mut self) -> Option<T> {
        if self.len == 0 {
            return None;
        }
        let v = self.items[self.head];
        self.head = (self.head + 1) % CAP;
        self.len -= 1;
        Some(v)
    }

    fn pop_back(&mut self) -> Option<T> {
        if self.len == 0 {
            return None;
        }
        self.len -= 1;
        Some(self.items[(self.head + self.len) % CAP])
    }

    fn get(&self, i: usize) -> Option<&T> {
        if i < self.len { Some(&self.items[(self.head + i) % CAP]) } else { None }
    }

    fn iter(&self) -> Iter<'_, T> {
        Iter { ring: self, front: 0, back: self.len }
    }
}

impl<T: Copy> Index<usize> for RingBuffer<T> {
    type Output = T;
    fn index(&self, i: usize) -> &T {
        self.get(i).expect("ring buffer index out of range")
    }
}

struct Iter<'a, T: Copy> {
    ring: &'a RingBuffer<T>,
    front: usize,
    back: usize,
}

impl<'a, T: Copy> Iterator for Iter<'a, T> {
    type Item = &'a T;
    fn next(&mut self) -> Option<&'a T> {
        if self.front == self.back {
            return None;
        }
        let item = self.ring.get(self.front);
        self.front += 1;
        item
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let n = self.back - self.front;
        (n, Some(n))
    }
}

impl<T: Copy> ExactSizeIterator for Iter<'_, T> {}

impl<'a, T: Copy> DoubleEndedIterator for Iter<'a, T> {
    fn next_back(&mut self) -> Option<&'a T> {
        if self.front == self.back {
            return None;
        }
        self.back -= 1;
        self.ring.get(self.back)
    }
}

impl<'a, T: Copy> IntoIterator for &'a RingBuffer<T> {
    type Item = &'a T;
    type IntoIter = Iter<'a, T>;
    fn into_iter(self) -> Iter<'a, T> {
        self.iter()
    }
}

/// Moving average over the last CAP samples.
struct MovingAverage {
    window: RingBuffer<i32>,
    sum: i64,
}

impl MovingAverage {
    fn new() -> Self {
        MovingAverage { window: RingBuffer::new(), sum: 0 }
    }

    fn add(&mut self, sample: i32) -> i64 {
        if let Some(old) = self.window.push(sample) {
            self.sum -= old as i64;
        }
        self.sum += sample as i64;
        self.sum / self.window.len() as i64
    }
}

#[derive(Clone, Copy, Default, PartialEq, Debug)]
enum Command {
    #[default]
    Nop,
    Move(i32, i32),
    Jump,
    Fire(u8),
}

fn print_command(c: &Command) {
    unsafe {
        match *c {
            Command::Nop => printf(c" nop".as_ptr()),
            Command::Move(dx, dy) => printf(c" move(%d,%d)".as_ptr(), dx, dy),
            Command::Jump => printf(c" jump".as_ptr()),
            Command::Fire(n) => printf(c" fire(%d)".as_ptr(), n as i32),
        };
    }
}

fn main() {
    let mut ring: RingBuffer<u32> = RingBuffer::new();
    let mut evicted = 0;
    for i in 1..=13u32 {
        if let Some(old) = ring.push(i * i) {
            evicted += old;
        }
    }
    unsafe { printf(c"len %zu evicted sum %u\n".as_ptr(), ring.len(), evicted) };
    for x in &ring {
        unsafe { printf(c"%u ".as_ptr(), *x) };
    }
    unsafe { printf(c"\nreversed:".as_ptr()) };
    for x in ring.iter().rev() {
        unsafe { printf(c" %u".as_ptr(), *x) };
    }
    let total: u32 = ring.iter().sum();
    let evens = ring.iter().filter(|x| **x % 2 == 0).count();
    unsafe { printf(c"\ntotal %u evens %zu first %u last %u\n".as_ptr(), total, evens, ring[0], ring[ring.len() - 1]) };

    let front = ring.pop_front();
    let back = ring.pop_back();
    unsafe {
        printf(c"popped %u and %u, now %zu\n".as_ptr(), front.unwrap_or(0), back.unwrap_or(0), ring.len());
    }
    while ring.pop_front().is_some() {}
    unsafe { printf(c"empty: %d\n".as_ptr(), ring.get(0).is_none() as i32) };

    let mut avg = MovingAverage::new();
    let samples = [10, 20, 30, 40, 50, 60, 70, 80, 90, 100, -400, 5];
    for s in samples {
        let a = avg.add(s);
        unsafe { printf(c"%lld ".as_ptr(), a) };
    }
    unsafe { printf(c"\n".as_ptr()) };

    let mut history: RingBuffer<Command> = RingBuffer::new();
    let script = [
        Command::Move(1, 0), Command::Jump, Command::Fire(3), Command::Move(0, -1), Command::Nop,
        Command::Fire(1), Command::Jump, Command::Move(2, 2), Command::Fire(9), Command::Jump,
    ];
    for c in script {
        history.push(c);
    }
    unsafe { printf(c"history:".as_ptr()) };
    history.iter().for_each(print_command);
    let shots: u32 = history.iter().filter_map(|c| match c {
        Command::Fire(n) => Some(*n as u32),
        _ => None,
    }).sum();
    let last_jump = history.iter().rposition(|c| *c == Command::Jump);
    unsafe { printf(c"\nshots %u, last jump at %zd\n".as_ptr(), shots, last_jump.map_or(-1, |i| i as isize)) };
}

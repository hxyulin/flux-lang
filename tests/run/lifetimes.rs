//! References in structs and return values: a parser over a byte slice
//! holding a reference, methods returning references into `self` and
//! `Option<&T>`, functions choosing between references by their
//! lifetimes, `mut` bindings of references reassigned in loops, `break`
//! with a reference, references returned from closures and through
//! generic functions, linked nodes borrowing each other, structs
//! holding `&mut` and promoted constants returned as `&'static`.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn show(label: i32, value: i64) {
    unsafe {
        printf(c"%d: %lld\n".as_ptr(), label, value);
    }
}

struct Parser<'a> {
    src: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(src: &'a [u8]) -> Parser<'a> {
        Parser { src, pos: 0 }
    }

    fn peek(&self) -> Option<&'a u8> {
        if self.pos < self.src.len() {
            Some(&self.src[self.pos])
        } else {
            None
        }
    }

    fn bump(&mut self) -> Option<u8> {
        let c = *self.peek()?;
        self.pos += 1;
        Some(c)
    }

    fn number(&mut self) -> Option<i64> {
        let mut value = 0i64;
        let mut any = false;
        while let Some(&c) = self.peek() {
            if c < 48 || c > 57 {
                break;
            }
            value = value * 10 + (c - 48) as i64;
            self.pos += 1;
            any = true;
        }
        if any { Some(value) } else { None }
    }

    fn sum(&mut self) -> i64 {
        let mut total = 0;
        loop {
            match self.number() {
                Some(n) => total += n,
                None => {}
            }
            if self.bump().is_none() {
                return total;
            }
        }
    }
}

struct Pair {
    a: i64,
    b: i64,
}

impl Pair {
    fn larger(&self) -> &i64 {
        if self.a >= self.b { &self.a } else { &self.b }
    }

    fn larger_mut(&mut self) -> &mut i64 {
        if self.a >= self.b { &mut self.a } else { &mut self.b }
    }
}

fn longest<'a>(x: &'a [i64], y: &'a [i64]) -> &'a [i64] {
    if x.len() >= y.len() { x } else { y }
}

fn find(v: &[i64], target: i64) -> Option<&i64> {
    let mut i = 0;
    while i < v.len() {
        if v[i] == target {
            return Some(&v[i]);
        }
        i += 1;
    }
    None
}

fn first<T>(v: &[T]) -> &T {
    &v[0]
}

fn largest(v: &[i64]) -> &i64 {
    let mut best = &v[0];
    let mut i = 1;
    while i < v.len() {
        if v[i] > *best {
            best = &v[i];
        }
        i += 1;
    }
    best
}

struct Node<'a> {
    value: i64,
    next: Option<&'a Node<'a>>,
}

fn chain_sum(mut node: &Node<'_>) -> i64 {
    let mut total = node.value;
    while let Some(next) = node.next {
        total += next.value;
        node = next;
    }
    total
}

struct Slot<'a> {
    target: &'a mut i64,
}

impl Slot<'_> {
    fn add(&mut self, n: i64) {
        *self.target += n;
    }
}

fn zero() -> &'static i64 {
    &0
}

fn table() -> &'static [i64; 3] {
    &[10, 20, 30]
}

fn main() {
    let input: [u8; 7] = [49, 50, 43, 51, 48, 43, 52];
    let mut p = Parser::new(&input);
    show(1, p.sum());
    let mut pair = Pair { a: 3, b: 9 };
    show(2, *pair.larger());
    *pair.larger_mut() += 100;
    show(3, pair.b);
    let short = [1, 2];
    let long = [3, 4, 5];
    show(4, longest(&short, &long).len() as i64);
    match find(&long, 4) {
        Some(r) => show(5, *r),
        None => show(5, -1),
    }
    show(6, match find(&long, 7) {
        Some(r) => *r,
        None => -1,
    });
    show(7, *first(&long) + *largest(&long));
    let tail = Node { value: 5, next: None };
    let mid = Node { value: 6, next: Some(&tail) };
    let head = Node { value: 7, next: Some(&mid) };
    show(8, chain_sum(&head));
    let mut counter = 0;
    {
        let mut slot = Slot { target: &mut counter };
        slot.add(4);
        slot.add(5);
    }
    show(9, counter);
    let values = [8, 3, 11, 2];
    let mut smallest = &values[0];
    for i in 1..values.len() {
        if values[i] < *smallest {
            smallest = &values[i];
        }
    }
    show(10, *smallest);
    let found = loop {
        let mut i = 0;
        while i < values.len() {
            if values[i] > 10 {
                break;
            }
            i += 1;
        }
        break &values[i];
    };
    show(11, *found);
    let pick = |i: usize| &values[i];
    show(12, *pick(1) + *pick(2));
    show(13, *zero() + table()[2]);
    let r: &i64;
    if values[0] > 5 {
        r = &values[1];
    } else {
        r = &values[2];
    }
    show(14, *r);
}

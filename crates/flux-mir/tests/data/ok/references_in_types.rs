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

    fn next(&mut self) -> Option<u8> {
        let c = *self.peek()?;
        self.pos += 1;
        Some(c)
    }

    fn rest(&self) -> &'a [u8] {
        self.src
    }
}

struct Pair {
    a: i32,
    b: i32,
}

impl Pair {
    fn first(&self) -> &i32 {
        &self.a
    }

    fn first_mut(&mut self) -> &mut i32 {
        &mut self.a
    }

    fn bigger(&self) -> &i32 {
        if self.a > self.b { &self.a } else { &self.b }
    }
}

fn longest<'a>(x: &'a i32, y: &'a i32) -> &'a i32 {
    if *x > *y { x } else { y }
}

fn get(v: &[i32], i: usize) -> Option<&i32> {
    if i < v.len() { Some(&v[i]) } else { None }
}

fn id<T>(x: T) -> T {
    x
}

fn pick<'a, 'b: 'a>(x: &'a i32, y: &'b i32, first: bool) -> &'a i32 {
    if first { x } else { y }
}

fn statik() -> &'static i32 {
    &42
}

fn main() {
    let bytes = [1u8, 2, 3];
    let mut p = Parser::new(&bytes);
    let mut sum = 0;
    while let Some(c) = p.next() {
        sum += c as i32;
    }
    let _ = p.peek();
    let _ = p.rest();
    let mut pair = Pair { a: 1, b: 2 };
    let f = *pair.first();
    *pair.first_mut() += 10;
    let g = *pair.bigger();
    let x = 5;
    let y = 7;
    let l = *longest(&x, &y);
    let v = [1, 2, 3];
    let o = get(&v, 1);
    let n = match o {
        Some(r) => *r,
        None => 0,
    };
    let r = id(&x);
    let mut m = &x;
    if n > 1 {
        m = &y;
    }
    let k;
    k = &v[0];
    let br = loop {
        break &y;
    };
    let c = |i: usize| &v[i];
    let cv = *c(2);
    let pk = *pick(&x, &y, false);
    let total = sum + f + g + l + n + *r + *m + *k + *br + cv + pk + *statik();
    if total == 0 {
        loop {}
    }
}

//! Inherent impls: associated functions and constructors through `Self`,
//! methods taking `self`, `mut self`, `&self` and `&mut self` on named,
//! tuple and unit structs, auto-ref and auto-deref of receivers, UFCS
//! calls, two-phase borrows, `&mut self` methods mutating nested structs,
//! methods returning structs on borrowed and unborrowed receivers, and
//! consuming methods.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

struct Unit;

impl Unit {
    fn answer(self) -> u32 {
        42
    }

    fn twice(&self, n: u32) -> u32 {
        n * 2
    }
}

struct Vec2 {
    x: i64,
    y: i64,
}

impl Vec2 {
    fn new(x: i64, y: i64) -> Self {
        Self { x, y }
    }

    fn zero() -> Vec2 {
        Self::new(0, 0)
    }

    fn dot(&self, other: &Vec2) -> i64 {
        self.x * other.x + self.y * other.y
    }

    fn add(&self, other: &Self) -> Self {
        Vec2::new(self.x + other.x, self.y + other.y)
    }

    fn scaled(self, k: i64) -> Vec2 {
        Vec2 {
            x: self.x * k,
            y: self.y * k,
        }
    }

    fn translate(&mut self, dx: i64, dy: i64) {
        self.x += dx;
        (*self).y += dy;
    }

    fn get_x(&self) -> i64 {
        self.x
    }

    fn set_x(&mut self, x: i64) {
        self.x = x;
    }
}

struct Rgb(u8, u8, u8);

impl Rgb {
    fn gray(v: u8) -> Self {
        Self(v, v, v)
    }

    fn packed(&self) -> u32 {
        (self.0 as u32) << 16 | (self.1 as u32) << 8 | self.2 as u32
    }

    fn brighten(&mut self, by: u8) {
        self.0 += by;
        self.1 += by;
        self.2 += by;
    }

    fn inverted(mut self) -> Rgb {
        self.0 = 255 - self.0;
        self.1 = 255 - self.1;
        self.2 = 255 - self.2;
        self
    }
}

struct Body {
    id: u16,
    pos: Vec2,
    vel: Vec2,
    color: Rgb,
    mass: i128,
    alive: bool,
}

impl Body {
    fn spawn(id: u16, x: i64, y: i64) -> Body {
        Body {
            id,
            pos: Vec2::new(x, y),
            vel: Vec2::zero(),
            color: Rgb::gray(100),
            mass: 1,
            alive: true,
        }
    }

    fn step(&mut self) {
        let v = &self.vel;
        let dx = v.x;
        let dy = v.y;
        self.pos.translate(dx, dy);
        self.vel.translate(1, -1);
        self.color.brighten(17);
        self.mass *= -3;
        self.alive = !self.alive;
    }

    fn momentum(&self) -> Vec2 {
        Vec2::new(self.vel.x * self.mass as i64, self.vel.y * self.mass as i64)
    }

    fn position(&self) -> Vec2 {
        self.pos.add(&Vec2::zero())
    }

    fn into_color(self) -> Rgb {
        self.color
    }

    fn show(&self) {
        unsafe {
            printf(
                c"body %u pos (%lld,%lld) vel (%lld,%lld) color %06x alive %d\n".as_ptr(),
                self.id as u32,
                self.pos.x,
                self.pos.y,
                self.vel.x,
                self.vel.y,
                self.color.packed(),
                self.alive as i32,
            );
            printf(
                c"  mass %016llx%016llx\n".as_ptr(),
                (self.mass >> 64) as u64,
                self.mass as u64,
            );
        }
    }
}

struct Acc {
    total: u64,
    count: u32,
    small: u8,
}

impl Acc {
    fn empty() -> Self {
        Acc {
            total: 0,
            count: 0,
            small: 0,
        }
    }

    fn get(&self) -> u64 {
        self.total
    }

    fn set(&mut self, v: u64) {
        self.total = v;
        self.count += 1;
        self.small += 7;
    }

    fn feed(&mut self, n: u64) {
        if n > 0 {
            self.set(self.get() * 3 + n);
            self.feed(n - 1);
        }
    }

    fn finish(self) -> (u64, u32) {
        (self.total + self.small as u64, self.count)
    }
}

fn twice_x(v: &mut Vec2) {
    v.set_x(v.get_x() * 2);
}

fn grow(mut b: Body) -> Body {
    b.step();
    b.vel.translate(5, 5);
    b
}

fn main() {
    let u = Unit;
    let a = u.twice(Unit.answer()) + Unit::twice(&Unit, 1);
    let ur = &u;
    let b = ur.twice(3) + u.answer();
    unsafe {
        printf(c"unit %u %u\n".as_ptr(), a, b);
    }

    // Unborrowed and borrowed receivers.
    let p = Vec2::new(3, -4);
    let q = Vec2 { x: 10, y: 20 };
    let r = p.add(&q);
    let d = p.dot(&q) + Vec2::dot(&r, &p);
    let s = r.scaled(-2);
    unsafe {
        printf(c"vec %lld %lld %lld %lld\n".as_ptr(), d, s.x, s.y, p.get_x());
    }

    let mut m = Vec2::zero();
    m.translate(7, 8);
    let mr = &mut m;
    mr.translate(1, 1);
    mr.set_x(mr.get_x() + 100);
    twice_x(mr);
    Vec2::translate(mr, -1, -2);
    let sum = m.add(&p).add(&Vec2::new(1, 1));
    let shared = &m;
    let via_ref = shared.add(&q);
    unsafe {
        printf(
            c"mut vec %lld %lld sum %lld %lld via %lld %lld\n".as_ptr(),
            m.x,
            m.y,
            sum.x,
            sum.y,
            via_ref.x,
            via_ref.y,
        );
    }

    // Two-phase borrows.
    let mut t = Vec2::new(5, 6);
    t.set_x(t.get_x() + t.dot(&Vec2::new(1, 1)));
    t.translate(t.x, t.y);
    unsafe {
        printf(c"two-phase %lld %lld\n".as_ptr(), t.x, t.y);
    }

    // Tuple structs and consuming `mut self`.
    let mut c = Rgb::gray(16);
    c.brighten(200);
    let packed = c.packed();
    let inv = c.inverted();
    let ir = &inv;
    unsafe {
        printf(
            c"rgb %06x %06x %06x\n".as_ptr(),
            packed,
            ir.packed(),
            Rgb(1, 2, 3).inverted().packed(),
        );
    }

    // Nested structs mutated through `&mut self`.
    let mut body = Body::spawn(9, 100, -100);
    body.vel.translate(3, 4);
    let mut i = 0;
    while i < 4 {
        body.step();
        let br = &mut body;
        br.pos.translate(i, -i);
        i += 1;
    }
    body.show();
    let mo = body.momentum();
    let po = (&body).position();
    unsafe {
        printf(
            c"momentum %lld %lld position %lld %lld\n".as_ptr(),
            mo.x,
            mo.y,
            po.x,
            po.y,
        );
    }
    let grown = grow(body);
    grown.show();
    let color = grown.into_color();
    unsafe {
        printf(c"color %06x\n".as_ptr(), color.packed());
    }

    // Methods returning structs on a borrowed receiver.
    let local = Body::spawn(1, 2, 3);
    let lr = &local;
    let lp = lr.position();
    let lm = Body::momentum(lr);
    local.show();
    unsafe {
        printf(c"local %lld %lld %lld %lld\n".as_ptr(), lp.x, lp.y, lm.x, lm.y);
    }

    // Two-phase borrows in a loop and in recursion.
    let mut acc = Acc::empty();
    acc.set(acc.get() + 5);
    acc.feed(6);
    let mut j = 0;
    while j < 10 {
        acc.set(acc.get() + j);
        j += 1;
    }
    let (total, count) = acc.finish();
    unsafe {
        printf(c"acc %llu %u\n".as_ptr(), total, count);
    }
}

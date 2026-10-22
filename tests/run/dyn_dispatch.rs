//! Trait objects in harder places: diamond supertraits upcast along
//! each path, methods returning references, memory-passed arguments
//! (arrays) and results (sret structs and tuples) through vtables,
//! `&mut dyn` passed on and reborrowed, closures capturing trait
//! objects, and `dyn Fn` with several arguments returning a tuple.

unsafe extern "C" { fn printf(format: *const i8, ...) -> i32; }

trait A { fn a(&self) -> i32 { 1 } }
trait B: A { fn b(&self) -> i32 { 2 } }
trait C2: A { fn c2(&self) -> i32 { 3 } }
trait D: B + C2 { fn d(&self) -> i32; fn name(&self) -> &str; }

#[derive(Clone, Copy)]
struct Big { x: [i64; 4], y: (i32, i32, i32) }

trait Make {
    fn make(&self, k: i64) -> Big;
    fn sum(&self, arr: [i64; 3]) -> i64;
    fn pair(&self) -> (i64, i64, i64);
    fn unit(&mut self, v: i32);
    fn get(&self) -> i32;
}

struct S { n: i32, label: &'static str }
impl A for S { fn a(&self) -> i32 { self.n } }
impl B for S {}
impl C2 for S { fn c2(&self) -> i32 { self.n * 3 } }
impl D for S { fn d(&self) -> i32 { self.a() + self.b() + self.c2() } fn name(&self) -> &str { self.label } }

impl Make for S {
    fn make(&self, k: i64) -> Big { Big { x: [k, k + 1, k + 2, self.n as i64], y: (1, 2, self.n) } }
    fn sum(&self, arr: [i64; 3]) -> i64 { arr[0] + arr[1] + arr[2] + self.n as i64 }
    fn pair(&self) -> (i64, i64, i64) { (self.n as i64, 2, 3) }
    fn unit(&mut self, v: i32) { self.n += v; }
    fn get(&self) -> i32 { self.n }
}

fn via_c(x: &dyn C2) -> i32 { x.a() + x.c2() }
fn chain(d: &dyn D) -> i32 {
    let b: &dyn B = d;
    let a: &dyn A = b;
    let c: &dyn C2 = d;
    a.a() * 1000 + b.b() * 100 + via_c(c) + d.d()
}

fn twice(m: &mut dyn Make) {
    m.unit(1);
    m.unit(2);
}

fn main() {
    let s = S { n: 5, label: "five" };
    let mut t = S { n: 1, label: "one" };
    let d: &dyn D = &s;
    let n = d.name();
    let m: &dyn Make = &s;
    let big = m.make(10);
    twice(&mut t);
    let mm: &mut dyn Make = &mut t;
    twice(mm);
    mm.unit(10);
    let closure = move || d.d() + m.get();
    let f: &dyn Fn() -> i32 = &closure;
    let g: &dyn Fn(i32, i64) -> (i64, i32) = &|a: i32, b: i64| (b * 2, a + 1);
    let (p, q) = g(3, 4);
    unsafe {
        printf(c"%d %d %d\n".as_ptr(), chain(d), n.as_bytes()[0] as i32, n.len() as i32);
        printf(c"%lld %lld %d %d\n".as_ptr(), big.x[0] + big.x[3], m.sum([1, 2, 3]), big.y.2, m.pair().0 as i32);
        printf(c"%d %d %lld %d\n".as_ptr(), t.n, f(), p, q);
    }
}

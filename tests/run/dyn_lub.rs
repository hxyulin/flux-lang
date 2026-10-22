//! Trait objects with associated types bounded by traits, traits with
//! defaulted parameters (`dyn Def` is `dyn Def<i64>`), and branches,
//! arrays and `match` arms meeting at a trait object type.

unsafe extern "C" { fn printf(format: *const i8, ...) -> i32; }
trait Val { type X: Copy; fn get(&self) -> Self::X; }
struct A; struct B;
impl Val for A { type X = i32; fn get(&self) -> i32 { 1 } }
impl Val for B { type X = i32; fn get(&self) -> i32 { 2 } }
trait Def<T = i64> { fn d(&self) -> T; }
impl Def for A { fn d(&self) -> i64 { 40 } }
fn pick<'a>(c: bool, a: &'a A, b: &'a B) -> &'a dyn Val<X = i32> {
    if c { a } else { b }
}
fn pick2(n: i32, a: &A, b: &B) -> i32 {
    let v = [a as &dyn Val<X = i32>, b, a];
    let m: &dyn Val<X = i32> = match n { 0 => a, _ => b };
    let w = if n > 0 { b as &dyn Val<X = i32> } else { a };
    v[n as usize].get() + m.get() * 10 + w.get() * 100
}
fn main() {
    let (a, b) = (A, B);
    let d: &dyn Def = &a;
    unsafe { printf(c"%d %d %d %lld\n".as_ptr(), pick(true, &a, &b).get(), pick(false, &a, &b).get(), pick2(1, &a, &b), d.d()); }
}

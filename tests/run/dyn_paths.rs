//! Trait objects' methods named by paths (`Tr::get(d)`, `<dyn Tr as
//! Tr>::add`, `<dyn Tr>::get`), as function items called directly and
//! passed to generic code (`o.map(Tr::get)`), dispatched through vtables.

unsafe extern "C" { fn printf(format: *const i8, ...) -> i32; }
trait Tr { fn get(&self) -> i32; fn add(&self, x: i32) -> i32 { self.get() + x } }
struct A(i32);
impl Tr for A { fn get(&self) -> i32 { self.0 } }
fn main() {
    through_fn();
    let a = A(3);
    let d: &dyn Tr = &a;
    let x = Tr::get(d) + <dyn Tr as Tr>::add(d, 10) + <dyn Tr>::get(d) * 100;
    let g = Tr::add;
    unsafe { printf(c"%d %d %d\n".as_ptr(), x, 0, g(d, 1)); }
}

fn apply<F: Fn(&dyn Tr) -> i32>(f: F, x: &dyn Tr) -> i32 { f(x) }
fn through_fn() {
    let a = A(3);
    let d: &dyn Tr = &a;
    let o: Option<&dyn Tr> = Some(d);
    let v = o.map(Tr::get).unwrap_or(0);
    unsafe { printf(c"%d %d\n".as_ptr(), v + apply(|t: &dyn Tr| t.get() + 1, d), 6); }
}

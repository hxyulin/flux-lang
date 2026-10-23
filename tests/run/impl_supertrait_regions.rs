trait Parent {
    fn read(self) -> i32;
}
trait Child: Parent {}
trait Grandchild: Child {}
#[derive(Clone, Copy)]
struct S<'a>(&'a i32);
struct Owner<T>(T);
impl<'a> Parent for S<'a> {
    fn read(self) -> i32 { *self.0 }
}
impl<'a> Child for S<'a> {}
impl<'a> Grandchild for S<'a> {}
impl<T: Parent> Parent for Owner<T> {
    fn read(self) -> i32 { self.0.read() + 1 }
}
impl<T: Child> Child for Owner<T> {}
impl<T: Grandchild> Grandchild for Owner<T> {}
fn direct<T: Grandchild>(x: T) -> i32 { x.read() }
fn callback<T: Grandchild, F: FnOnce(T) -> i32>(x: T, f: F) -> i32 { f(x) }
struct Pair<'a, 'b>(&'a i32, &'b i32);
impl<'a, 'b: 'a> Parent for Pair<'a, 'b> {
    fn read(self) -> i32 { *self.0 + *self.1 }
}
impl<'a, 'b: 'a> Child for Pair<'a, 'b> {}
impl<'a, 'b: 'a> Grandchild for Pair<'a, 'b> {}
struct Repeated<'a, 'b>(&'a i32, &'b i32);
impl<'a> Parent for Repeated<'a, 'a> {
    fn read(self) -> i32 { *self.0 + *self.1 }
}
impl<'a> Child for Repeated<'a, 'a> {}
impl<'a> Grandchild for Repeated<'a, 'a> {}
struct StaticOnly<'a>(&'a i32);
impl Parent for StaticOnly<'static> {
    fn read(self) -> i32 { *self.0 }
}
impl<'a: 'static> Child for StaticOnly<'a> {}
impl<'a: 'static> Grandchild for StaticOnly<'a> {}
static VALUE: i32 = 7;
fn main() {
    let n = 41;
    let m = 1;
    println!("{}", direct(S(&n)));
    println!("{}", direct(Owner(S(&n))));
    println!("{}", callback(S(&n), |x| x.read()));
    println!("{}", callback(Owner(S(&n)), |x| x.read()));
    println!("{}", direct(Pair(&m, &n)));
    println!("{}", direct(Repeated(&m, &n)));
    println!("{}", direct(StaticOnly(&VALUE)));
}

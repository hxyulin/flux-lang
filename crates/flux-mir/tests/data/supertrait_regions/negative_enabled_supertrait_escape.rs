trait Parent { fn read(self) -> &'static i32; }
trait Child: Parent {}
struct S<'a>(&'a i32);
impl Parent for S<'static> { fn read(self) -> &'static i32 { self.0 } }
impl<'a> Child for S<'a> {}
fn read<T: Child>(t: T) -> &'static i32 { t.read() }
fn escape<'a>(x: &'a i32) -> &'static i32 { read(S(x)) }
fn main() { let n = 42; println!("{}", *escape(&n)); }

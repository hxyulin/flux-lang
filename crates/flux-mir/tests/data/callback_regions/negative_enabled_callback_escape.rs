trait Mark { fn read(self) -> &'static i32; }
#[derive(Clone, Copy)]
struct S<'a>(&'a i32);
impl Mark for S<'static> { fn read(self) -> &'static i32 { self.0 } }
fn read<T: Mark>(x: T) -> Option<&'static i32> { Some(x.read()) }
fn escape<'a>(x: &'a i32) -> &'static i32 { [S(x)].into_iter().find_map(read).unwrap() }
fn main() { let n = 42; println!("{}", *escape(&n)); }

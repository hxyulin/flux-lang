fn params<'a, 'b: 'a, T: ?Sized + Clone + 'a, const N: usize, U = u8>(x: &'a T) {}
fn bounds<F>(f: F) where F: for<'x> Fn(&'x u8) -> u8 + Send, Vec<u8>: Clone, 'static: 'static {}
fn args() -> Result<Vec<Vec<u8>>, Box<dyn Error + Send + Sync>> {}
fn assoc<I: Iterator<Item = u8>, M: Conv<'static, Body<'static> = u8>, E: Ev<T<'static>: Default>>() {}
fn consts() -> Array<u8, 3, { N + 1 }, -1> {}
fn capture<'a>(x: &'a u8) -> impl Iterator<Item = u8> + use<'a> {}
fn ret_bounds(x: &u8) -> impl Fn(u8) -> u8 + '_ {}
struct Wrapper<T, const N: usize = 3>([T; N]);
trait Gat { type Item<'a>: Copy where Self: 'a; }
impl<T> Gat for Wrapper<T> { type Item<'a> = &'a T where Self: 'a; }
impl <T as Trait>::Assoc {}

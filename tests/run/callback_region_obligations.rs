trait Mark {
    fn keep(self) -> Option<Self> where Self: Sized { Some(self) }
    fn read(self) -> &'static i32;
}
#[derive(Clone, Copy)]
struct S<'a>(&'a i32);
impl Mark for S<'static> {
    fn read(self) -> &'static i32 { self.0 }
}
fn read<T: Mark>(x: T) -> Option<&'static i32> { Some(x.read()) }
fn keep<T>(x: T) -> Option<T> { Some(x) }
fn local(x: &i32) -> Option<&i32> { Some(x) }
fn generic<T: Copy>(x: T) -> T {
    let callback = keep::<T>;
    let cloned = callback.clone();
    [x].into_iter().find_map(cloned).unwrap()
}
struct Wrap<T>(T);
impl<T: Mark> Wrap<T> {
    fn keep(x: T) -> Option<T> { Some(x) }
}
fn first<I: Iterator<Item = &'static i32>>(mut x: I) -> Option<&'static i32> { x.next() }
struct IterWrap<I>(I);
impl<I: Iterator<Item = &'static i32>> IterWrap<I> {
    fn first(mut x: I) -> Option<&'static i32> { x.next() }
}
fn main() {
    static N: i32 = 42;
    println!("{}", *[S(&N)].into_iter().find_map(read).unwrap());
    println!("{}", [S(&N)].into_iter().find_map(S::keep).unwrap().0);
    println!("{}", [S(&N)].into_iter().find_map(Wrap::keep).unwrap().0);
    println!("{}", [[&N].into_iter()].into_iter().find_map(first).unwrap());
    println!("{}", [[&N].into_iter()].into_iter().find_map(IterWrap::first).unwrap());
    let n = 42;
    println!("{}", [ &n ].into_iter().find_map(local).unwrap());
    println!("{}", generic(&n));
}

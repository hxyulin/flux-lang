// `impl` blocks' lifetimes that rustc rejects: a `Drop` impl for some
// lifetimes only, `'static` or one named twice (E0366), or with bounds
// on lifetimes the struct does not have or imply (E0367, in the list or
// the `where` clause, between lifetimes or on a type); and a lifetime
// that only a trait `impl` block's associated type names (E0207). A
// bound the struct's fields imply (`&'a &'b i32` that `'b: 'a`) is fine.

struct Fixed<'a>(&'a i32);

impl Drop for Fixed<'static> {
    fn drop(&mut self) {}
}

struct Twice<'a, 'b>(&'a i32, &'b i32);

impl<'a> Drop for Twice<'a, 'a> {
    fn drop(&mut self) {}
}

struct Listed<'a, 'b>(&'a i32, &'b i32);

impl<'a, 'b: 'a> Drop for Listed<'a, 'b> {
    fn drop(&mut self) {}
}

struct Where<'a, 'b>(&'a i32, &'b i32);

impl<'x, 'y> Drop for Where<'x, 'y>
where
    'y: 'x,
{
    fn drop(&mut self) {}
}

struct OnType<'a, T>(&'a i32, T);

impl<'a, T: 'a> Drop for OnType<'a, T> {
    fn drop(&mut self) {}
}

struct Nested<'a, 'b>(&'a &'b i32);

impl<'a, 'b: 'a> Drop for Nested<'a, 'b> {
    fn drop(&mut self) {}
}

struct Held<'a, T>(&'a T);

impl<'a, T: 'a> Drop for Held<'a, T> {
    fn drop(&mut self) {}
}

trait Tr {
    type Out;
}

struct S;

impl<'a> Tr for S {
    type Out = &'a i32;
}

fn main() {}

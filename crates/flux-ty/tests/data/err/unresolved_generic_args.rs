// A generic argument that names nothing is an unresolved type, as rustc
// reports it, not a constant.
struct A<const N: usize>;
const C: usize = 1;
mod m {}

fn f(_: Option<Missing>, _: A<Gone>, _: Option<C>, _: Result<m::Nope, ()>) {}

fn main() {}

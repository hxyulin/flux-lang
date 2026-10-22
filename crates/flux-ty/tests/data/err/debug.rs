// `#[derive(Debug)]` needs every field to implement `Debug`, reported at
// the field in rustc's words; manual impls name `core::fmt`'s types.

use core::fmt;

struct NoDebug;

#[derive(Debug)]
struct S {
    a: i32,
    b: NoDebug,
}

#[derive(Debug)]
enum E {
    A(NoDebug),
    B { x: u8 },
}

#[derive(Debug)]
struct W<T>(T);

struct Manual;

impl fmt::Debug for Manual {
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Ok(())
    }
}

fn show<T: fmt::Debug>(_: &T) {}

fn main() {
    show(&W(Manual));
    show(&[1, 2]);
    show(&(1, Some(true), [Manual], Ok::<u8, ()>(1), core::cmp::Ordering::Less));
    show(&NoDebug);
}

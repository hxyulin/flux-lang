// Where `impl Trait` may not be (E0562), or is not supported yet.

struct S {
    f: impl Copy,
}

struct T(impl Copy);

const C: impl Copy = 1;

static ST: impl Copy = 1;

type A = impl Copy;

trait Tr {
    type Out;

    fn m(&self) -> impl Copy;
}

impl Tr for S {
    type Out = impl Copy;

    fn m(&self) -> impl Copy {
        1
    }
}

fn fn_pointers(f: fn(impl Copy)) -> fn() -> impl Copy {
    loop {}
}

fn fn_bounds<F: Fn() -> impl Copy, G: Fn(impl Copy)>(f: F, g: G) {}

fn where_bounds()
where
    impl Copy: Tr,
{
}

fn nested() -> impl Iterator<Item = impl Copy> {
    0..1
}

fn use_in_argument(x: impl Copy + use<>) {}

fn no_trait() -> impl 'static {}

unsafe extern "C" {
    fn ext(x: impl Copy) -> impl Copy;
}

fn bodies() {
    let x: impl Copy = 1;
    let v: Option<impl Copy> = None;
    let c = |y: impl Copy| -> impl Copy { y };
    let e = 1 as impl Copy;
}

fn main() {}

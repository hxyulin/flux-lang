//! Borrowed generic receivers choose the blanket method before the bound method.
//! rustc prints `1 1`; Flux prints `2 1`, with and without optimization.
//! Found during method-resolution review; also reproduced at baseline bfa06e5.
//! The first call probes Self = &T before the pointee's T: A bound candidate.
//! The owned-T bound-priority regression is fixed; this borrowed case remains.

trait A {
    fn pick(&self) -> u8;
}

trait B {
    fn pick(self) -> u8;
}

struct S;

impl A for S {
    fn pick(&self) -> u8 {
        1
    }
}

impl<T> B for &T {
    fn pick(self) -> u8 {
        2
    }
}

fn borrowed<T: A>(x: &T) -> u8 {
    x.pick()
}

fn owned<T: A>(x: T) -> u8 {
    x.pick()
}

fn main() {
    println!("{} {}", borrowed(&S), owned(S));
}

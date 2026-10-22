// `?` on a `ControlFlow` where rustc's `FromResidual` bound does not
// hold (E0277): in a function returning an `Option`, a `Result`, a
// `ControlFlow` with another `Break` type or none of them, and
// `Option` and `Result` in one returning a `ControlFlow`.

use core::ops::ControlFlow;

fn a(c: ControlFlow<i32>) -> Option<i32> {
    c?;
    None
}

fn b(c: ControlFlow<i32>) -> Result<i32, i32> {
    c?;
    Ok(1)
}

fn c(c: ControlFlow<i32>) -> ControlFlow<u8> {
    c?;
    ControlFlow::Continue(())
}

fn d(c: Option<i32>) -> ControlFlow<u8> {
    c?;
    ControlFlow::Continue(())
}

fn e(c: Result<i32, i32>) -> ControlFlow<i32> {
    c?;
    ControlFlow::Continue(())
}

fn f(c: ControlFlow<i32>) -> i32 {
    c?;
    1
}

fn main() {}

use core::future::Future;
use core::pin::pin;
use core::task::{Context, Waker};
fn main() {
    let mut future=pin!(async {
        panic!("async source location");
    });
    let mut context=Context::from_waker(Waker::noop());
    let _=future.as_mut().poll(&mut context);
}

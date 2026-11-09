use core::future::Future;
use core::pin::pin;
use core::task::{Context, Waker};
#[track_caller]
async fn fail() {
    core::future::ready(()).await;
    panic!("async function source location");
}
fn main() {
    let mut future = pin!(fail());
    let mut context = Context::from_waker(Waker::noop());
    let _ = future.as_mut().poll(&mut context);
}

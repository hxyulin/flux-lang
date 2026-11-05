// `?` on a `Poll` of something other than a `Result`, and on an `Option`
// in a function returning `Poll<Result<..>>`.
use std::task::Poll;

fn plain(p: Poll<u32>) -> Result<u32, ()> {
    let v = p?;
    Ok(0)
}

fn option(o: Option<u32>) -> Poll<Result<u32, ()>> {
    let v = o?;
    Poll::Ready(Ok(v))
}

fn main() {}

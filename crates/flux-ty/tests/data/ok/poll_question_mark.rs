// `?` on Results in functions returning Poll, and on Poll values.
use std::task::Poll;

#[derive(Debug)]
struct Error(#[allow(dead_code)] u32);

impl From<u8> for Error {
    fn from(e: u8) -> Error {
        Error(e as u32 + 100)
    }
}

fn parse(x: u32) -> Result<u32, u8> {
    if x < 10 { Ok(x) } else { Err(x as u8) }
}

fn ready(x: u32) -> Poll<Result<u32, Error>> {
    if x == 0 { Poll::Pending } else { Poll::Ready(parse(x).map_err(Error::from)) }
}

fn next(x: u32) -> Poll<Option<Result<u32, Error>>> {
    match x {
        0 => Poll::Pending,
        1 => Poll::Ready(None),
        _ => Poll::Ready(Some(parse(x).map_err(|e| Error(e as u32)))),
    }
}

// `?` on a `Result` in functions returning `Poll`.
fn poll_result(x: u32) -> Poll<Result<u32, Error>> {
    let v = parse(x)?;
    Poll::Ready(Ok(v * 2))
}

fn poll_stream(x: u32) -> Poll<Option<Result<u32, Error>>> {
    let v = parse(x)?;
    Poll::Ready(Some(Ok(v + 1)))
}

// `?` on a `Poll` passes `Pending` (and `None`) through.
fn forward(x: u32) -> Result<Poll<u32>, Error> {
    let p: Poll<u32> = ready(x)?;
    Ok(p.map(|v| v + 1000))
}

fn forward_stream(x: u32) -> Poll<Result<Poll<Option<u32>>, Error>> {
    let p = next(x)?;
    Poll::Ready(Ok(p))
}

// The return type names the output through a projection.
trait Step {
    type Output;
    fn step(&mut self) -> Poll<Self::Output>;
}

struct Steps(u32);

impl Step for Steps {
    type Output = Result<u32, Error>;
    fn step(&mut self) -> Poll<Self::Output> {
        self.0 += 4;
        let v = parse(self.0)?;
        Poll::Ready(Ok(v))
    }
}

fn main() {
    let mut s = Steps(0);
    for _ in 0..3 {
        println!("{:?}", s.step());
    }
    for x in [0u32, 1, 3, 42] {
        println!("{:?} {:?}", poll_result(x), poll_stream(x));
        println!("{:?} {:?}", forward(x), forward_stream(x));
    }
}

use std::error::Error;

trait Event {
    type Failure: Into<Box<dyn Error + Send + Sync>>;
}

impl<T: Event> Event for Box<T> {
    type Failure = T::Failure;
}

impl<M, T: Event> Event for (M, Box<T>) {
    type Failure = T::Failure;
}

struct Source;

impl Event for Source {
    type Failure = std::io::Error;
}

fn main() {
    let error = <Box<Source> as Event>::Failure::from(std::io::ErrorKind::InvalidData);
    let wrapped = <(u32, Box<Source>) as Event>::Failure::from(std::io::ErrorKind::TimedOut);
    println!("{:?} {:?}", error.kind(), wrapped.kind());
}

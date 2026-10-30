// A receiver token supplied by the caller keeps its context even when the
// macro supplies `&` and `mut` from a different expansion.
struct Counter(u32);

macro_rules! methods {
    ($this:ident) => {
        fn read(&$this) -> u32 { $this.0 }
        fn increment(&mut $this) { $this.0 += 1; }
        fn take($this) -> u32 { $this.0 }
    };
}

macro_rules! forward {
    () => { methods!(self); };
}

impl Counter {
    forward!();
}

fn main() {
    let mut counter = Counter(41);
    assert_eq!(counter.read(), 41);
    counter.increment();
    assert_eq!(counter.read(), 42);
    println!("{}", counter.take());
}

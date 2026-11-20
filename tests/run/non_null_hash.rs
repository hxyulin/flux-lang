use std::hash::{DefaultHasher, Hash, Hasher};
use std::ptr::NonNull;

fn digest(value: impl Hash) -> u64 {
    let mut state = DefaultHasher::new();
    value.hash(&mut state);
    state.finish()
}

fn main() {
    struct Pointee(u8);
    let items = [Pointee(1), Pointee(2)];
    let thin = NonNull::from(&items[0]);
    let short = NonNull::from(&items[..1]);
    let full = NonNull::from(&items[..]);

    assert_eq!(digest(thin), digest(thin.as_ptr()));
    assert_eq!(digest(short), digest(short.as_ptr()));
    assert_eq!(digest(full), digest(full.as_ptr()));
    assert_ne!(short, full);
    println!("{}", items[0].0);
}

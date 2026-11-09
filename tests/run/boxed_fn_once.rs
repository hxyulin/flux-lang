use std::cell::Cell;

struct Guard<'a>(&'a Cell<u32>);

impl Drop for Guard<'_> {
    fn drop(&mut self) { self.0.set(self.0.get() + 1); }
}

#[repr(align(64))]
struct Aligned;

fn invoke<F: FnOnce() -> String>(f: F) -> String { f() }

fn main() {
    let drops = Cell::new(0);
    let text = String::from("owned callback");
    let guard = Guard(&drops);
    let call: Box<dyn FnOnce() -> String + '_> = Box::new(move || {
        let _keep = &guard;
        text
    });
    assert_eq!(invoke(call), "owned callback");
    assert_eq!(drops.get(), 1);

    let guard = Guard(&drops);
    let unused: Box<dyn FnOnce() + '_> = Box::new(move || { drop(guard); });
    drop(unused);
    assert_eq!(drops.get(), 2);

    let aligned = Aligned;
    let zero: Box<dyn FnOnce() -> usize> = Box::new(move || {
        let _keep = &aligned;
        17
    });
    assert_eq!(zero(), 17);

    let borrowed = 23;
    let read: Box<dyn FnOnce() -> u32 + '_> = Box::new(|| borrowed);
    assert_eq!(read(), 23);

    let shared: Box<dyn Fn(u32) -> u32> = Box::new(|x| x + 1);
    assert_eq!(shared(3) + shared(4), 9);
    let mut count = 0;
    let mut mutable: Box<dyn FnMut() -> u32> = Box::new(move || { count += 1; count });
    assert_eq!(mutable(), 1);
    assert_eq!(mutable(), 2);

    let text = String::from("sized callback");
    assert_eq!(invoke(Box::new(move || text)), "sized callback");
    println!("{}", drops.get());
}

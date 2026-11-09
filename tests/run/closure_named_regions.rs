fn named<'a>(value: &'a u32) -> &'a u32 {
    (|| (|| Ok::<&'a u32, ()>(value))())().unwrap()
}

fn identity<T>(value: T) -> T { value }

fn argument<'a>(value: &'a u32) -> &'a u32 {
    (|| identity::<&'a u32>(value))()
}

trait Counter {
    fn value(&self) -> u32;
    fn increment(&mut self);

    fn through_closure(&mut self) {
        (|| {
            if self.value() < 10 { self.increment(); }
        })();
    }
}

impl<T: Counter + ?Sized> Counter for &mut T {
    fn value(&self) -> u32 { (**self).value() }
    fn increment(&mut self) { (**self).increment(); }
}

impl Counter for u32 {
    fn value(&self) -> u32 { *self }
    fn increment(&mut self) { *self += 1; }
}

fn main() {
    let value = 42;
    assert_eq!(*named(&value), 42);
    assert_eq!(*argument(&value), 42);
    let mut counter = 7u32;
    counter.through_closure();
    assert_eq!(counter, 8);
    let reference = &mut counter;
    let mut change = || { *reference += 1; let _shared = &reference; };
    change();
    assert_eq!(counter, 9);
    println!("{} {} {}", named(&value), argument(&value), counter);
}

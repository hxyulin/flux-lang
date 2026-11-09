fn identity<T>(value: T) -> T { value }

fn borrowed<'a>(value: &'a u32) -> &'a u32 {
    let pointer: fn(&'a u32) -> &'a u32 = identity::<&'a u32>;
    pointer(value)
}

fn mutable<'a>(value: &'a mut u32) -> &'a mut u32 {
    let pointer: fn(&'a mut u32) -> &'a mut u32 = identity::<&'a mut u32>;
    pointer(value)
}

fn apply<'a>(pointer: fn(&'a u32) -> &'a u32, value: &'a u32) -> &'a u32 {
    pointer(value)
}

fn closure<'a>(value: &'a u32) -> &'a u32 {
    (|| apply(identity::<&'a u32>, value))()
}

struct Table { size: fn() -> usize }

fn size<T: 'static>() -> usize { std::mem::size_of::<T>() }

fn table() -> &'static Table {
    &Table { size: size::<Box<dyn std::error::Error + Send + Sync>> }
}

fn main() {
    let mut value = 41;
    assert_eq!(*borrowed(&value), 41);
    assert_eq!(*closure(&value), 41);
    *mutable(&mut value) += 1;
    assert_eq!(value, 42);
    let pointer: fn(Box<dyn Iterator<Item = u32>>) -> Option<u32> =
        <Box<dyn Iterator<Item = u32>> as Iterator>::last;
    let iter: Box<dyn Iterator<Item = u32>> = Box::new(vec![1u32, 42].into_iter());
    assert_eq!(pointer(iter), Some(42));
    let size = (table().size)();
    assert_eq!(size, std::mem::size_of::<Box<dyn std::error::Error + Send + Sync>>());
    println!("{} {}", value, size);
}

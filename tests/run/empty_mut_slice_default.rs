struct NoDefault(u64);
#[repr(align(64))]
struct Aligned;
struct Dropping;
impl Drop for Dropping {
    fn drop(&mut self) {
        panic!("empty default must not drop an element");
    }
}
fn empty<'a, T>() -> &'a mut [T] {
    Default::default()
}
fn main() {
    let mut values = [NoDefault(7), NoDefault(9)];
    let mut remaining: &mut [NoDefault] = &mut values;
    let original = std::mem::take(&mut remaining);
    assert!(remaining.is_empty());
    assert_eq!(original.len(), 2);
    original[0].0 = 11;
    assert_eq!(original[1].0, 9);
    let another = std::mem::take(&mut remaining);
    assert!(another.is_empty());
    assert!(remaining.is_empty());
    let a = empty::<Aligned>();
    let b = empty::<Aligned>();
    assert!(a.is_empty());
    assert!(b.is_empty());
    assert_eq!(a.as_ptr().addr() % 64, 0);
    assert_eq!(b.as_ptr().addr() % 64, 0);
    assert!(empty::<Dropping>().is_empty());
    assert_eq!(values[0].0, 11);
    println!("slice-default {} {}", values[0].0, values[1].0);
}

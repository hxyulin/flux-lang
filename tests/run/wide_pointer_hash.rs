use std::hash::{Hash, Hasher};
#[derive(Default)]
struct Words(Vec<usize>);
impl Hasher for Words {
    fn finish(&self) -> u64 {
        0
    }
    fn write(&mut self, _: &[u8]) {
        panic!("pointer hash used byte writes")
    }
    fn write_usize(&mut self, value: usize) {
        self.0.push(value);
    }
}
fn check<T: ?Sized>(pointer: *const T, words: usize, metadata: Option<usize>) -> usize {
    let mut via_trait = Words::default();
    pointer.hash(&mut via_trait);
    let mut via_function = Words::default();
    std::ptr::hash(pointer, &mut via_function);
    assert_eq!(via_trait.0, via_function.0);
    assert_eq!(via_trait.0.len(), words);
    assert_eq!(via_trait.0[0], pointer as *const () as usize);
    if let Some(metadata) = metadata {
        assert_eq!(via_trait.0[1], metadata);
    } else if words == 2 {
        assert_ne!(via_trait.0[1], 0);
    }
    words
}
trait Value {
    fn get(&self) -> u8;
}
impl Value for u8 {
    fn get(&self) -> u8 {
        *self
    }
}
trait Other {
    fn other(&self) -> u8;
}
impl Other for u8 {
    fn other(&self) -> u8 {
        *self + 8
    }
}
struct Tail<T: ?Sized> {
    prefix: u8,
    tail: T,
}
fn main() {
    let mut array = [3u8, 5, 7];
    let thin = check(array.as_ptr(), 1, None);
    check(
        std::ptr::slice_from_raw_parts(std::ptr::null::<u8>(), 11),
        2,
        Some(11),
    );
    let slice = check(&array[..] as *const [u8], 2, Some(3));
    let text = check("hello" as *const str, 2, Some(5));
    let value = 9u8;
    let object: &dyn Value = &value;
    assert_eq!(object.get(), 9);
    let dynamic = check(object as *const dyn Value, 2, None);
    let other: &dyn Other = &value;
    assert_eq!(other.other(), 17);
    let mut first = Words::default();
    let mut second = Words::default();
    (object as *const dyn Value).hash(&mut first);
    (other as *const dyn Other).hash(&mut second);
    assert_eq!(first.0[0], second.0[0]);
    assert_ne!(first.0[1], second.0[1]);
    let mutable = &mut array[..] as *mut [u8];
    let mut via_mutable = Words::default();
    mutable.hash(&mut via_mutable);
    assert_eq!(via_mutable.0.len(), 2);
    assert_eq!(via_mutable.0[1], 3);
    let tail = Tail {
        prefix: 1u8,
        tail: [2u8, 3],
    };
    let wide: &Tail<[u8]> = &tail;
    let unsized_tail = check(wide as *const Tail<[u8]>, 2, Some(2));
    println!(
        "{} {} {} {} {} {}",
        thin,
        slice,
        text,
        dynamic,
        via_mutable.0.len(),
        unsized_tail
    );
}

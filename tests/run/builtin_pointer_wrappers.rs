#[inline(never)]
fn len_and_last(values: &[u16]) -> (usize, u16) {
    let len = values.len();
    let base = values.as_ptr();
    assert!(len > 0);
    // SAFETY: len - 1 is the index of the last element.
    (len, unsafe { *base.add(len - 1) })
}

fn main() {
    let values = [3, 17, 257, 1025];
    let (len, last) = len_and_last(&values);
    println!("{len} {last}");

    let values = [(); 3];
    let base = values.as_ptr();
    // SAFETY: the offset is within the slice, and its elements have size zero.
    assert_eq!(unsafe { base.add(values.len()) }, base);
}

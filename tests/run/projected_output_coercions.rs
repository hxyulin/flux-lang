struct Bytes<'a>(&'a [u8]);

fn bytes(v: &Vec<Vec<u8>>, i: usize) -> &[u8] {
    // SAFETY: the caller passes an index within the vector.
    unsafe { v.get_unchecked(i) }
}

fn wrapped(v: &Vec<Vec<u8>>, i: usize) -> Bytes<'_> {
    // SAFETY: the caller passes an index within the vector.
    Bytes(unsafe { v.get_unchecked(i) })
}

fn mutable(v: &mut Vec<Vec<u8>>, i: usize) -> &mut [u8] {
    // SAFETY: the caller passes an index within the vector.
    unsafe { v.get_unchecked_mut(i) }
}

fn main() {
    let mut v = vec![vec![1, 2, 3], vec![4, 5]];
    assert_eq!(bytes(&v, 0), &[1, 2, 3]);
    assert_eq!(wrapped(&v, 1).0, &[4, 5]);
    mutable(&mut v, 1)[0] = 9;
    assert_eq!(bytes(&v, 1), &[9, 5]);
    let empty: Option<&[u8]> = None;
    let slice: &[u8] = empty.map_or(&[], |x| x);
    assert_eq!(slice.len(), 0);
    println!("{} {}", bytes(&v, 0).len(), bytes(&v, 1)[0]);
}

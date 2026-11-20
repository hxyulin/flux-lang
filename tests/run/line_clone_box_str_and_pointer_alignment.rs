fn main() {
    let mut lines = "first\r\nsecond\nthird".lines();
    assert_eq!(lines.next(), Some("first"));
    let mut copy = lines.clone();
    assert_eq!(copy.next(), Some("second"));
    assert_eq!(copy.next(), Some("third"));
    assert_eq!(lines.next(), Some("second"));

    let boxed: Box<str> = "flux".into();
    let address = boxed.as_ptr();
    let string = boxed.into_string();
    assert_eq!(string.as_ptr(), address);
    assert_eq!(string.as_str(), "flux");

    let mut value = 0u64;
    let aligned = &raw mut value;
    let misaligned = aligned.cast::<u8>().wrapping_add(1).cast::<u64>();
    assert!(aligned.is_aligned());
    assert!(aligned.cast_const().is_aligned());
    assert!(!misaligned.is_aligned());
    assert!(!misaligned.cast_const().is_aligned());
    println!("{string}");
}

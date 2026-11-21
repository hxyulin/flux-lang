fn main() {
    let mut text = String::with_capacity(64);
    text.push_str("café");
    let original_capacity = text.capacity();
    text.shrink_to(16);
    let bounded_capacity = text.capacity();
    assert!(bounded_capacity >= 16 && bounded_capacity <= original_capacity);
    text.shrink_to(original_capacity + 1);
    assert_eq!(text.capacity(), bounded_capacity);
    text.shrink_to(0);
    assert!(text.capacity() >= text.len() && text.capacity() <= bounded_capacity);
    println!("{text} {}", text.len());
}

fn main() {
    let mut alphabetic = 0usize;
    let mut alphanumeric = 0usize;
    let mut digest = 0u64;
    for code in 0..=0x10ffff {
        if let Some(c) = char::from_u32(code) {
            let a = c.is_alphabetic();
            let b = c.is_alphanumeric();
            assert_eq!((&c).is_alphabetic(), a);
            assert_eq!((&c).is_alphanumeric(), b);
            alphabetic += a as usize;
            alphanumeric += b as usize;
            let flags = (a as u64) | ((b as u64) << 1);
            digest = digest.wrapping_mul(31).wrapping_add((code as u64) * 4 + flags);
        }
    }
    assert!('\u{345}'.is_alphabetic());
    assert!('Ⅳ'.is_alphanumeric());
    assert!('²'.is_alphanumeric());
    assert!(!'β'.is_ascii());
    assert!(!'💡'.is_alphabetic());
    println!("unicode {:?} {alphabetic} {alphanumeric} {digest}", std::char::UNICODE_VERSION);
}

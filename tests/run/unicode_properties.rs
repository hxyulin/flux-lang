fn main() {
    let mut alphabetic = 0usize;
    let mut alphanumeric = 0usize;
    let mut lowercase = 0usize;
    let mut uppercase = 0usize;
    let mut digest = 0u64;
    for code in 0..=0x10ffff {
        if let Some(c) = char::from_u32(code) {
            let a = c.is_alphabetic();
            let b = c.is_alphanumeric();
            assert_eq!((&c).is_alphabetic(), a);
            assert_eq!((&c).is_alphanumeric(), b);
            let lower = c.is_lowercase();
            let upper = c.is_uppercase();
            assert_eq!((&c).is_lowercase(), lower);
            assert_eq!((&c).is_uppercase(), upper);
            lowercase += lower as usize;
            uppercase += upper as usize;
            alphabetic += a as usize;
            alphanumeric += b as usize;
            let flags = (a as u64) | ((b as u64) << 1) | ((lower as u64) << 2) | ((upper as u64) << 3);
            digest = digest.wrapping_mul(31).wrapping_add((code as u64) * 16 + flags);
        }
    }
    assert!('\u{345}'.is_alphabetic());
    assert!('Ⅳ'.is_alphanumeric());
    assert!('²'.is_alphanumeric());
    assert!(!'β'.is_ascii());
    assert!(!'💡'.is_alphabetic());
    println!("unicode {:?} {alphabetic} {alphanumeric} {lowercase} {uppercase} {digest}", std::char::UNICODE_VERSION);
}

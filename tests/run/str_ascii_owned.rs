fn main() {
    for source in ["", "aAzZ09_!", "ALL UPPER", "all lower", "Rust\0ÉéΣσßİı🦀Az"] {
        let lower = source.to_ascii_lowercase();
        let upper = source.to_ascii_uppercase();
        assert_eq!(lower.len(), source.len());
        assert_eq!(upper.len(), source.len());
        for (index, byte) in source.bytes().enumerate() {
            assert_eq!(lower.as_bytes()[index], byte.to_ascii_lowercase());
            assert_eq!(upper.as_bytes()[index], byte.to_ascii_uppercase());
        }
        println!("{lower:?} {upper:?}");
    }
    // Include every ASCII code point and every UTF-8 sequence length.
    let mut source = String::new();
    for byte in 0..=127u8 { source.push(byte as char); }
    for ch in ['é', 'Ω', '中', '🦀', '\u{10ffff}'] { source.push(ch); }
    let snapshot = source.clone();
    let mut lower = source.to_ascii_lowercase();
    let upper = source.to_ascii_uppercase();
    assert_eq!(source, snapshot);
    lower.push('!');
    assert_eq!(source, snapshot);
    assert_eq!(upper.len(), source.len());
    assert_eq!(&source[128..], &upper[128..]);
    assert_eq!(&source[128..], &lower[128..lower.len()-1]);
    println!("bytes {} {} {}", source.len(), lower.len(), upper.len());
}

const fn classify(bytes: &[u8]) -> u32 {
    match bytes {
        b"" => 0,
        b"\x7fELF" => 1,
        b"!<arch>\n" => 2,
        b"a\0\xff" | br#"raw\bytes"# => 3,
        [b'A', 0..=255] => 4,
        _ => 5,
    }
}
const VALUES: [u32; 4] = [
    classify(b""),
    classify(b"\x7fELF"),
    classify(b"a\0\xff"),
    classify(b"no"),
];
fn array(bytes: &[u8; 2]) -> u32 {
    match bytes {
        b"AB" => 1,
        b"A\0" => 2,
        [b'A', _] => 3,
        _ => 4,
    }
}
fn borrowed<'a>(bytes: &'a [u8]) -> Option<&'a [u8]> {
    match bytes {
        same @ b"ABC" => Some(same),
        _ => None,
    }
}
fn guarded(bytes: &[u8], visits: &mut u32) -> u32 {
    match bytes {
        b"AB"
            if {
                *visits += 1;
                false
            } =>
        {
            1
        }
        b"AB" => 2,
        _ => 3,
    }
}
fn nested(value: &Option<(&[u8], bool)>) -> u32 {
    match value {
        Some((b"a", true)) | Some((b"b", false)) => 7,
        Some((b"", _)) => 8,
        _ => 9,
    }
}
fn let_else(bytes: &[u8]) -> bool {
    let b"AB" = bytes else {
        return false;
    };
    true
}
fn main() {
    assert_eq!(VALUES, [0, 1, 3, 5]);
    let b"" = b"";
    let empty: &[u8; 0] = b"";
    let value = match empty {
        b"" => 11,
    };
    assert_eq!(value, 11);
    assert_eq!(classify(b"!<arch>\n"), 2);
    assert_eq!(classify(br#"raw\bytes"#), 3);
    assert_eq!(classify(b"\x7fELF\0"), 5);
    let mut sum = 0u64;
    for a in 0u16..256 {
        for b in 0u16..256 {
            let bytes = [a as u8, b as u8];
            let expected = if a == 65 {
                if b == 66 {
                    1
                } else if b == 0 {
                    2
                } else {
                    3
                }
            } else {
                4
            };
            assert_eq!(array(&bytes), expected);
            assert_eq!(classify(&bytes), if a == 65 { 4 } else { 5 });
            sum += u64::from(array(&bytes));
        }
    }
    let text = [65u8, 66, 67];
    let view = borrowed(&text).unwrap();
    assert_eq!(view.as_ptr(), text.as_ptr());
    assert_eq!(view.len(), 3);
    assert!(borrowed(b"AB").is_none());
    let mut visits = 0;
    assert_eq!(guarded(b"AB", &mut visits), 2);
    assert_eq!(guarded(b"A", &mut visits), 3);
    assert_eq!(visits, 1);
    assert_eq!(nested(&Some((b"a", true))), 7);
    assert_eq!(nested(&Some((b"", false))), 8);
    assert_eq!(nested(&Some((b"a", false))), 9);
    assert!(let_else(b"AB"));
    assert!(!let_else(b"ABC"));
    println!("byte patterns {sum} {:?} {visits}", VALUES);
}

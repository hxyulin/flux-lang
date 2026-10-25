use std::borrow::Cow;

fn inspect(bytes: &[u8], digest: &mut u64, borrowed: &mut usize) {
    let text = String::from_utf8_lossy(bytes);
    if let Cow::Borrowed(value) = &text {
        if !bytes.is_empty() {
            assert_eq!(value.as_ptr(), bytes.as_ptr());
        }
        *borrowed += 1;
    }
    for byte in text.bytes() {
        *digest = digest.wrapping_mul(131).wrapping_add(byte as u64);
    }
    *digest = digest.wrapping_mul(131).wrapping_add(text.len() as u64);
}

fn main() {
    let mut digest = 0u64;
    let mut borrowed = 0usize;
    inspect(b"", &mut digest, &mut borrowed);
    inspect("ASCII, café, 世界, 🦀".as_bytes(), &mut digest, &mut borrowed);
    for first in 0..256usize {
        inspect(&[first as u8], &mut digest, &mut borrowed);
        for second in 0..256usize {
            inspect(&[first as u8, second as u8], &mut digest, &mut borrowed);
        }
    }
    for bytes in [
        &b"a\xf0\x90\x80b\xffc"[..],
        &b"\xed\xa0\x80\xf4\x90\x80\x80"[..],
        &b"\xe2\x82\xac\xe2\x82"[..],
        &b"\xf0\x9f\xa6\x80\xc0\xaf\xe0\x80\x80"[..],
    ] {
        for end in 0..=bytes.len() {
            inspect(&bytes[..end], &mut digest, &mut borrowed);
        }
    }
    println!("{digest} {borrowed}");

    let original = String::from("borrowed");
    let mut text: Cow<'_, str> = Cow::Borrowed(&original);
    let shared = text.clone();
    assert!(matches!(shared, Cow::Borrowed(_)));
    text.to_mut().push_str(" copy");
    let mut independent = text.clone();
    independent.to_mut().push('!');
    println!("{} {} {:?}", original, text, independent);
    assert_eq!(text.into_owned(), "borrowed copy");

    let values = [2u32, 3, 5];
    let mut copy: Cow<'_, [u32]> = Cow::Borrowed(&values);
    copy.to_mut().push(7);
    let owned = copy.into_owned();
    println!("{:?} {:?}", values, owned);

    let mut scalar: Cow<'_, u32> = Cow::Owned(19);
    *scalar.to_mut() += 1;
    println!("{} {}", *scalar.clone(), scalar.into_owned());
    let owned = String::from("keep allocation");
    let pointer = owned.as_ptr();
    let cow: Cow<'_, str> = Cow::Owned(owned);
    let moved = cow.into_owned();
    assert_eq!(pointer, moved.as_ptr());
}

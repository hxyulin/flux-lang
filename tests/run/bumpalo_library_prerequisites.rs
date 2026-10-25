use std::char::{decode_utf16, REPLACEMENT_CHARACTER};
use std::mem::{size_of, zeroed};

#[derive(Debug, PartialEq)]
struct Zero { bytes: [u64; 3], flag: bool, ch: char }

fn main() {
    assert_eq!(REPLACEMENT_CHARACTER, char::REPLACEMENT_CHARACTER);
    let zero: Zero = unsafe { zeroed() };
    assert_eq!(zero, Zero { bytes: [0; 3], flag: false, ch: '\0' });
    let pointer: *const u8 = unsafe { zeroed() }; assert!(pointer.is_null());
    let mut storage = [1u8, 2, 3];
    let base = std::ptr::NonNull::new(storage.as_mut_ptr()).unwrap();
    let next = unsafe { base.add(1) };
    assert!(base < next); assert_eq!(base.cmp(&next), std::cmp::Ordering::Less);
    assert_eq!(base.partial_cmp(&base), Some(std::cmp::Ordering::Equal));
    assert_eq!(format!("{:p}", base), format!("{:p}", base.as_ptr()));
    let wide = std::ptr::NonNull::slice_from_raw_parts(base, 3);
    assert_eq!(format!("{:p}", wide), format!("{:p}", wide.as_ptr()));
    assert_eq!(isize::max_value(), isize::MAX); assert_eq!(i128::min_value(), i128::MIN);
    let value: () = unsafe { zeroed() }; assert_eq!(size_of::<()>(), 0); assert_eq!(value, ());
    let samples: &[&[u16]] = &[
        &[], &[0, 65, 0xffff], &[0xd800, 0xdc00], &[0xdbff, 0xdfff],
        &[0xdc00], &[0xd800], &[0xd800, 65, 0xdc00], &[0xd800, 0xd801, 0xdc00],
    ];
    for &sample in samples {
        let mut iter = decode_utf16(sample.iter().copied());
        loop {
            println!("hint {:?}", iter.size_hint());
            let Some(value) = iter.next() else { break };
            match value {
                Ok(value) => println!("char {:?}", value),
                Err(error) => println!("error {} {:?} {}", error.unpaired_surrogate(), error, error),
            }
        }
        assert_eq!(iter.next(), None);
    }
    let mut digest = 0u64;
    let mut escaped = 0u64;
    for scalar in 0..=0x10ffffu32 {
        let Some(ch) = char::from_u32(scalar) else { continue };
        let mut words = [0u16; 2];
        let count = if scalar <= 0xffff { words[0] = scalar as u16; 1 }
            else { let value = scalar - 0x10000; words[0] = 0xd800 | (value >> 10) as u16; words[1] = 0xdc00 | (value & 0x3ff) as u16; 2 };
        let mut iter = decode_utf16(words[..count].iter().copied());
        assert_eq!(iter.next(), Some(Ok(ch))); assert_eq!(iter.next(), None);
        digest = digest.wrapping_add(scalar as u64);
        for output in ch.escape_debug() {
            escaped = escaped.wrapping_mul(1099511628211) ^ output as u64;
        }
    }
    let mut iter = decode_utf16([0xd800, 65, 66]);
    assert!(iter.next().unwrap().is_err());
    let copy = iter.clone(); assert_eq!(iter.collect::<Vec<_>>(), copy.collect::<Vec<_>>());
    let mut text = String::from("aé中z");
    assert!(text.get_mut(2..3).is_none());
    text.get_mut(0..1).unwrap().make_ascii_uppercase();
    text[6..=6].make_ascii_uppercase();
    unsafe { text.get_unchecked_mut(..1).as_bytes_mut()[0] = b'B'; }
    assert_eq!(unsafe { text.get_unchecked(1..3) }, "é");
    assert_eq!(text, "Bé中Z");
    let address = text.as_mut_ptr(); assert_eq!(address as *const u8, text.as_ptr());
    for ch in ['\0', '\t', '"', '\'', 'a', 'é', '\u{301}', '\u{7f}', '\u{10ffff}'] {
        let mut escape = ch.escape_debug(); let original = escape.to_string();
        assert_eq!(escape.len(), original.chars().count());
        let _ = escape.next();
        println!("escape remaining {:?} display {:?}", escape.clone().collect::<String>(), escape.to_string());
    }
    println!("decode all scalars {digest} escape {escaped} layout {}", size_of::<std::alloc::LayoutErr>());
}

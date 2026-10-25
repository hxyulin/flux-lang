use std::num::IntErrorKind;

fn main() {
    let text = "aé中z";
    for mid in 0..=text.len() + 1 {
        println!("split {mid} {:?}", text.split_at_checked(mid));
    }
    let mut values = vec![10, 20, 30, 40];
    unsafe {
        assert_eq!(values.get_unchecked(1..3), [20, 30]);
        assert_eq!(values.get_unchecked(4..), []);
        values.get_unchecked_mut(1..=2).copy_from_slice(&[7, 9]);
    }
    values.extend_from_within(1..=2);
    assert_eq!(values, [10, 7, 9, 40, 7, 9]);
    values.extend_from_within(..0);
    let mut cloned = vec![String::from("one"), String::from("two")];
    cloned.extend_from_within(..);
    assert_eq!(cloned, ["one", "two", "one", "two"]);
    assert!([1, 2].iter().ne([1, 3].iter()));
    assert!([1, 2].iter().ne([1].iter()));
    assert!(![1, 2].iter().ne([1, 2].iter()));
    const BITS: u64 = match u64::from_str_radix("fF00", 16) { Ok(value) => value, Err(_) => 0 };
    assert_eq!(BITS, 65280);
    let samples = ["", "+", "-", "0", "+0", "-0", "11", "ff", "aZ", "_1", " 1", "10000000000000000", "-8000000000000000", "FFFFFFFFFFFFFFFF", "é"];
    for radix in 2..=36 {
        for sample in samples {
            println!("parse {radix} {sample:?} {:?} {:?} {:?} {:?}",
                u8::from_str_radix(sample, radix), i8::from_str_radix(sample, radix),
                u64::from_str_radix(sample, radix), i128::from_str_radix(sample, radix));
        }
    }
    assert_eq!(i128::from_str_radix("-80000000000000000000000000000000", 16), Ok(i128::MIN));
    assert_eq!(u128::from_str_radix("ffffffffffffffffffffffffffffffff", 16), Ok(u128::MAX));
    assert_eq!(u8::from_str_radix("100", 16).unwrap_err().kind(), &IntErrorKind::PosOverflow);
    println!("keel APIs {} {}", values.len(), BITS);
}

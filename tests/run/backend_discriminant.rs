use std::hash::{Hash, Hasher};
use std::mem::discriminant;
use std::io::Write;

enum Message { Empty, Text(String), Pair { a: u64, b: u64 } }
#[repr(i8)]
enum Signed { Negative = -7, Positive = 3 }
#[repr(u64)]
enum Unsigned { Large = u64::MAX }
const NEGATIVE: std::mem::Discriminant<Signed> = discriminant(&Signed::Negative);
trait Family { type Value; }
struct F;
impl Family for F { type Value = Signed; }
fn family_discriminant<T: Family>(value: &T::Value) -> std::mem::Discriminant<T::Value> {
    discriminant(value)
}
fn main() {
    let empty = discriminant(&Message::Empty);
    let a = Message::Text(String::from("first"));
    let b = Message::Text(String::from("second"));
    assert_eq!(discriminant(&a), discriminant(&b));
    assert_ne!(discriminant(&a), empty);
    assert_eq!(family_discriminant::<F>(&Signed::Negative), NEGATIVE);
    let value = 7u32;
    assert_ne!(discriminant(&Some(&value)), discriminant(&None::<&u32>));
    let mut left = std::collections::hash_map::DefaultHasher::new();
    let mut right = left.clone();
    discriminant(&a).hash(&mut left); discriminant(&b).hash(&mut right);
    assert_eq!(left.finish(), right.finish());
    assert!(matches!(a, Message::Text(ref text) if text == "first"));
    let pair = Message::Pair { a: 7, b: 9 };
    println!("{:?} {:?} {:?} {:?} {:?}", empty, discriminant(&pair), NEGATIVE,
        discriminant(&Signed::Positive), discriminant(&Unsigned::Large));
    let mut sink = std::io::sink();
    assert_eq!(sink.write(b"discarded").unwrap(),9);
    sink.write_all(&[]).unwrap(); sink.flush().unwrap();
    let x: core::ffi::c_double = 1.5; let y: core::ffi::c_float = 2.25;
    assert_eq!(x+y as f64,3.75);
}

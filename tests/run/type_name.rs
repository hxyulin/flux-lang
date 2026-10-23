use std::any::type_name;

struct Named;
struct Café;
fn describe<T: ?Sized>(_: &T) -> &'static str { type_name::<T>() }
fn borrowed<'a>(x: &'a u8) -> &'static str {
    let _ = x;
    type_name::<&'a u8>()
}
fn main() {
    for (got, expected) in [
        (type_name::<u32>(), "u32"),
        (type_name::<bool>(), "bool"),
        (type_name::<()>(), "()"),
        (type_name::<(u8,)>(), "(u8,)"),
        (type_name::<[u16; 3]>(), "[u16; 3]"),
        (type_name::<[u8]>(), "[u8]"),
        (type_name::<str>(), "str"),
        (type_name::<&u8>(), "&u8"),
        (type_name::<&mut u8>(), "&mut u8"),
        (type_name::<*const u8>(), "*const u8"),
        (type_name::<fn(i32) -> u8>(), "fn(i32) -> u8"),
    ] {
        assert_eq!(got, expected);
        println!("{}", got);
    }
    assert_eq!(std::any::type_name::<u64>(), core::any::type_name::<u64>());
    assert!(type_name::<Named>().ends_with("Named"));
    assert!(type_name::<Café>().ends_with("Café"));
    // Qualified paths are deliberately not compared: API spelling is diagnostic.
    assert!(type_name::<Option<Vec<Named>>>().contains("Option"));
    assert!(type_name::<Option<Vec<Named>>>().contains("Named"));
    let retained;
    { let value = 7u8; retained = borrowed(&value); }
    assert_eq!(retained, "&u8");
    assert_eq!(describe("abc"), "str");
    assert!(!describe(&|| 42).is_empty());
    println!("static descriptors survive values");
}

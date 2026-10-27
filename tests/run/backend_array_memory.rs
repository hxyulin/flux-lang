use std::mem::{ManuallyDrop,transmute_copy};
fn main() {
    let mut value=String::from("retained");
    let original=&value as *const String;
    assert_eq!(core::array::from_ref(&value).as_ptr(),original);
    core::array::from_mut(&mut value)[0].push_str("!");assert_eq!(value,"retained!");
    let raw=ManuallyDrop::new(String::from("moved"));
    let moved:String=unsafe {transmute_copy(&raw)};assert_eq!(moved,"moved");drop(moved);
    let bytes=[0x11u8,0x22,0x33,0x44,0x55,0x66,0x77,0x88];
    let word:u32=unsafe {transmute_copy(&bytes)};assert_eq!(word,u32::from_ne_bytes([0x11,0x22,0x33,0x44]));
    let z:()=unsafe{transmute_copy(&bytes)};
    let _:()=z;
    #[repr(align(128))]struct Aligned([u8;128]);
    let aligned:Aligned=unsafe{transmute_copy(&[7u8;128])};assert_eq!(aligned.0[127],7);
    println!("array memory {} {}",value,word);
}

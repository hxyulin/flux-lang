use std::cmp::Ordering;
use std::hash::{Hash, Hasher};
use std::mem::ManuallyDrop;
use std::pin::Pin;

#[derive(Clone)]
struct Owned(String);

#[derive(Copy)]
struct CustomClone(u32);
impl Clone for CustomClone {
    fn clone(&self) -> Self { Self(self.0 + 1) }
}

struct AddressHasher(usize);
impl Hasher for AddressHasher {
    fn finish(&self) -> u64 { self.0 as u64 }
    fn write(&mut self, _: &[u8]) { panic!("expected address hash"); }
    fn write_usize(&mut self, value: usize) { self.0 = value; }
}

fn identity(value: String) -> String { value }
unsafe fn raw(_: *const ()) {}
fn zero() {}
fn borrowed(value: &u32) -> &u32 { value }
fn thirteen(_: u8, _: u8, _: u8, _: u8, _: u8, _: u8,
          _: u8, _: u8, _: u8, _: u8, _: u8, _: u8, _: u8) {}
extern "C" fn c_identity(value: u32) -> u32 { value }
extern "C-unwind" fn unwind_identity(value: u32) -> u32 { value }
unsafe extern "C" fn c_raw(_: *const ()) {}

fn check<T: Eq + Ord + Hash>(value: T, address: usize) {
    assert!(value.eq(&value));
    assert!(!value.ne(&value));
    assert_eq!(value.cmp(&value), Ordering::Equal);
    assert_eq!(value.partial_cmp(&value), Some(Ordering::Equal));
    let mut hasher = AddressHasher(0);
    value.hash(&mut hasher);
    assert_eq!(hasher.0, address);
}

fn main() {
    let original = ManuallyDrop::new(Owned(String::from("owned")));
    let duplicate: ManuallyDrop<Owned> = original.clone();
    println!("{} {}", ManuallyDrop::into_inner(original).0, ManuallyDrop::into_inner(duplicate).0);

    let custom = ManuallyDrop::new(CustomClone(7));
    println!("{}", ManuallyDrop::into_inner(custom.clone()).0);

    let mut value = 41;
    let mut inner = Pin::new(&mut value);
    let mut outer = Pin::new(&mut inner);
    *outer.as_mut().as_deref_mut().get_mut() += 1;
    println!("{}", value);

    struct Fixed { value: u32, _pin: std::marker::PhantomPinned }
    let mut fixed = Box::pin(Fixed { value: 7, _pin: std::marker::PhantomPinned });
    let mut nested = Pin::new(&mut fixed);
    println!("{}", nested.as_mut().as_deref_mut().as_ref().get_ref().value);

    let f = zero as fn();
    check(f, f as usize);
    let f = thirteen as fn(u8, u8, u8, u8, u8, u8, u8, u8, u8, u8, u8, u8, u8);
    check(f, f as usize);
    let f = identity as fn(String) -> String;
    check(f, f as usize);
    let f = raw as unsafe fn(*const ());
    check(f, f as usize);
    let pair = (std::ptr::null::<()>(), f);
    assert_eq!(pair, pair);
    let f = c_identity as extern "C" fn(u32) -> u32;
    check(f, f as usize);
    let f = c_raw as unsafe extern "C" fn(*const ());
    check(f, f as usize);
    let f = borrowed as fn(&u32) -> &u32;
    check(f, f as usize);
    let f = unwind_identity as extern "C-unwind" fn(u32) -> u32;
    check(f, f as usize);
    println!("pointer contracts passed");
}

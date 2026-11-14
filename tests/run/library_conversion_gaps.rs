use std::cell::Cell;
use std::rc::Rc;

fn shared<T: ?Sized>(value: Box<T>) -> Rc<T> { value.into() }
trait Number { fn number(&self) -> u32; }
#[repr(align(64))]
struct Value<'a>(u32, &'a Cell<u32>);
impl Number for Value<'_> { fn number(&self) -> u32 { self.0 } }
impl Drop for Value<'_> { fn drop(&mut self) { self.1.set(self.1.get() + 1); } }
#[derive(Debug)]
struct Error(u32);
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.0) }
}
impl std::error::Error for Error {}
#[derive(Debug)]
struct Other;
impl std::fmt::Display for Other {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str("other") }
}
impl std::error::Error for Other {}
fn main() {
    let string: Box<str> = "héllo".into();
    let address = string.as_ptr();
    let bytes = string.into_boxed_bytes();
    assert_eq!(bytes.as_ptr(), address);
    assert_eq!(&*bytes, "héllo".as_bytes());
    let drops = Cell::new(0);
    let value: Rc<dyn Number + '_> = shared(Box::new(Value(42, &drops)) as Box<dyn Number>);
    assert_eq!(value.number(), 42);
    let weak = Rc::downgrade(&value);
    drop(value);
    assert!(weak.upgrade().is_none());
    assert_eq!(drops.get(), 1);
    let values: Box<[Value<'_>]> = vec![Value(7, &drops), Value(8, &drops)].into_boxed_slice();
    let values = shared(values);
    assert_eq!(values[1].0, 8);
    assert_eq!((&values[0] as *const Value<'_> as usize) % 64, 0);
    drop(values);
    assert_eq!(drops.get(), 3);
    assert_eq!(&*shared(Box::<str>::from("shared")), "shared");
    assert_eq!(shared(Box::new(())).as_ref(), &());
    let empty: Box<[()]> = vec![(); 3].into_boxed_slice();
    assert_eq!(shared(empty).len(), 3);

    let plain: Box<dyn std::error::Error> = Box::new(Error(3));
    assert_eq!(plain.downcast_ref::<Error>().unwrap().0, 3);
    assert_eq!(plain.downcast::<Error>().unwrap().0, 3);
    let mut send: Box<dyn std::error::Error + Send> = Box::new(Error(5));
    send.downcast_mut::<Error>().unwrap().0 += 1;
    assert_eq!(send.downcast::<Error>().unwrap().0, 6);
    let mut both: Box<dyn std::error::Error + Send + Sync> = Box::new(Error(7));
    let pointer = &*both as *const _ as *const ();
    both.downcast_mut::<Error>().unwrap().0 += 1;
    assert!(both.downcast_ref::<Other>().is_none());
    let both = both.downcast::<Other>().unwrap_err();
    assert_eq!(&*both as *const _ as *const (), pointer);
    let concrete = both.downcast::<Error>().unwrap();
    assert_eq!(&*concrete as *const _ as *const (), pointer);
    assert_eq!(concrete.0, 8);

    let address: core::net::IpAddr = "2001:db8::42".parse().unwrap();
    let same: std::net::IpAddr = address;
    assert_eq!(address, same);
    let data = [1, 2, 3];
    assert!(std::ptr::addr_eq(&data[..1], &data[..2]));
    assert!(!std::ptr::addr_eq(&data[..1], &data[1..]));
    println!("{} {} {} {}", bytes.len(), drops.get(), concrete.0, address);
}

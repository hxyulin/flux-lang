use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::mem::{size_of, size_of_val, align_of, align_of_val};
trait Value { fn value(&self) -> u32; fn set(&mut self, value: u32); }
trait Extra: Value { fn doubled(&self) -> u32 { self.value() * 2 } }
struct Probe { label: u8, log: Arc<Mutex<Vec<u8>>> }
impl Drop for Probe { fn drop(&mut self) { self.log.lock().unwrap().push(self.label); } }
#[repr(align(64))]
struct Aligned { number: u32, probe: Probe }
impl Value for Aligned {
    fn value(&self) -> u32 { self.number }
    fn set(&mut self, number: u32) { self.number = number; }
}
impl Extra for Aligned {}
struct Empty;
impl Value for Empty { fn value(&self) -> u32 { 0 } fn set(&mut self, _: u32) {} }
impl Value for u32 { fn value(&self) -> u32 { *self } fn set(&mut self, value: u32) { *self = value; } }
struct Plain<T: ?Sized> { tag: u8, tail: T }
static PLAIN: Plain<u32> = Plain { tag: 33, tail: 72 };
static DYNAMIC: &Plain<dyn Value + Sync> = &PLAIN;
struct Header<T: ?Sized> { probe: Probe, tail: T }
impl<T: ?Sized> Drop for Header<T> { fn drop(&mut self) { self.probe.log.lock().unwrap().push(0); } }
struct Outer<T: ?Sized> { probe: Probe, header: Header<T> }
#[repr(C, align(128))]
struct HighPrefix<T: ?Sized> { tag: u8, tail: T }
fn probe(label: u8, log: &Arc<Mutex<Vec<u8>>>) -> Probe { Probe { label, log: log.clone() } }
fn outer(log: &Arc<Mutex<Vec<u8>>>) -> Outer<Aligned> {
    Outer { probe: probe(1, log), header: Header { probe: probe(2, log), tail: Aligned { number: 37, probe: probe(3, log) } } }
}
fn check(value: &Outer<dyn Value>) {
    assert_eq!(value.probe.label, 1);
    assert_eq!(value.header.probe.label, 2);
    assert_eq!(value.header.tail.value(), 37);
    assert_eq!(size_of_val(value), size_of::<Outer<Aligned>>());
    assert_eq!(align_of_val(value), align_of::<Outer<Aligned>>());
}
fn main() {
    assert_eq!(DYNAMIC.tag, 33);
    assert_eq!(DYNAMIC.tail.value(), 72);
    assert_eq!(size_of_val(DYNAMIC), size_of::<Plain<u32>>());
    assert_eq!(align_of_val(DYNAMIC), align_of::<Plain<u32>>());
    let log = Arc::new(Mutex::new(Vec::new()));
    let boxed: Box<Outer<dyn Value>> = Box::new(outer(&log));
    check(&boxed);
    println!("{} {}", size_of_val(&*boxed), align_of_val(&*boxed));
    drop(boxed);
    assert_eq!(*log.lock().unwrap(), vec![1,0,2,3]);
    log.lock().unwrap().clear();
    let concrete = Arc::new(outer(&log));
    let shared: Arc<Outer<dyn Value>> = concrete.clone();
    drop(concrete);
    check(&shared);
    assert!(log.lock().unwrap().is_empty());
    drop(shared);
    assert_eq!(*log.lock().unwrap(), vec![1,0,2,3]);
    log.lock().unwrap().clear();
    let concrete = Rc::new(outer(&log));
    let shared: Rc<Outer<dyn Value>> = concrete.clone();
    drop(concrete);
    check(&shared);
    drop(shared);
    assert_eq!(*log.lock().unwrap(), vec![1,0,2,3]);
    log.lock().unwrap().clear();
    let mut concrete = outer(&log);
    {
        let dynamic: &mut Outer<dyn Value> = &mut concrete;
        dynamic.header.tail.set(91);
        assert_eq!(dynamic.header.tail.value(), 91);
        assert_eq!(dynamic.header.probe.label, 2);
    }
    assert_eq!(concrete.header.tail.number, 91);
    let concrete_pointer: std::ptr::NonNull<Outer<Aligned>> = std::ptr::NonNull::from(&mut concrete);
    let pointer: std::ptr::NonNull<Outer<dyn Value>> = concrete_pointer;
    // SAFETY: concrete remains live and no conflicting reference is used here.
    unsafe { assert_eq!(pointer.as_ref().header.tail.value(), 91); }
    drop(concrete);
    log.lock().unwrap().clear();
    let fancy: Box<Header<dyn Extra>> = Box::new(Header { probe: probe(4, &log), tail: Aligned { number: 17, probe: probe(5, &log) } });
    assert_eq!(fancy.tail.doubled(), 34);
    let base: Box<Header<dyn Value>> = fancy;
    assert_eq!(base.tail.value(), 17);
    drop(base);
    assert_eq!(*log.lock().unwrap(), vec![0,4,5]);
    log.lock().unwrap().clear();
    let high: Box<HighPrefix<dyn Value>> = Box::new(HighPrefix { tag: 9, tail: Aligned { number: 23, probe: probe(6, &log) } });
    assert_eq!(high.tag, 9);
    assert_eq!(high.tail.value(), 23);
    assert_eq!(size_of_val(&*high), size_of::<HighPrefix<Aligned>>());
    assert_eq!(align_of_val(&*high), 128);
    println!("{} {}", size_of_val(&*high), align_of_val(&*high));
    drop(high);
    assert_eq!(*log.lock().unwrap(), vec![6]);
    log.lock().unwrap().clear();
    let empty: Box<Header<dyn Value>> = Box::new(Header { probe: probe(7, &log), tail: Empty });
    assert_eq!(empty.tail.value(), 0);
    assert_eq!(size_of_val(&*empty), size_of::<Header<Empty>>());
    assert_eq!(align_of_val(&*empty), align_of::<Header<Empty>>());
    drop(empty);
    assert_eq!(*log.lock().unwrap(), vec![0,7]);
    println!("struct dyn tails complete");
}

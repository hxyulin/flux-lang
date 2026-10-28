use std::cell::{Ref, RefCell};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, RwLock};

static DROPS: AtomicUsize = AtomicUsize::new(0);
#[repr(align(64))]
struct Tracked(u32);
impl Drop for Tracked {
    fn drop(&mut self) {
        DROPS.fetch_add(1, Ordering::Relaxed);
    }
}
fn movable<T: Unpin>(_: T) {}
fn main() {
    let mut marker = std::marker::PhantomPinned;
    assert_eq!(std::mem::size_of_val(&marker), 0);
    movable(&marker);
    movable(&mut marker);
    movable(Box::new(marker));
    println!("{:?}", marker);
    let cell = RefCell::new([1u32, 2, 3]);
    let view: &RefCell<[u32]> = &cell;
    let first = view.borrow();
    let second = Ref::clone(&first);
    assert_eq!(&*first, &[1, 2, 3]);
    assert!(view.try_borrow_mut().is_err());
    drop(first);
    assert!(view.try_borrow_mut().is_err());
    drop(second);
    {
        let mut exclusive = view.borrow_mut();
        exclusive[1] = 17;
        assert!(view.try_borrow().is_err());
    }
    assert_eq!(&*view.borrow(), &[1, 17, 3]);
    let mutex = Mutex::new([5u32, 6]);
    let view: &Mutex<[u32]> = &mutex;
    view.lock().unwrap()[0] = 19;
    assert_eq!(&*view.lock().unwrap(), &[19, 6]);
    let rw = RwLock::new([7u32, 8, 9]);
    let view: &RwLock<[u32]> = &rw;
    {
        let first = view.read().unwrap();
        let second = view.read().unwrap();
        assert_eq!(&*first, &*second);
    }
    view.write().unwrap()[2] = 23;
    assert_eq!(&*view.read().unwrap(), &[7, 8, 23]);
    let boxed: Box<Mutex<[Tracked]>> = Box::new(Mutex::new([Tracked(31), Tracked(37)]));
    {
        let mut guard = boxed.lock().unwrap();
        assert_eq!(guard.len(), 2);
        assert_eq!(guard.as_ptr() as usize % 64, 0);
        guard[0].0 = 41;
        println!("{} {}", guard[0].0, guard[1].0);
    }
    drop(boxed);
    assert_eq!(DROPS.load(Ordering::Relaxed), 2);
    let empty: &RefCell<[u32]> = &RefCell::new([]);
    assert_eq!(empty.borrow().len(), 0);
    println!(
        "{} {} {}",
        cell.borrow()[1],
        mutex.lock().unwrap()[0],
        rw.read().unwrap()[2]
    );
}

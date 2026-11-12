use std::cell::Cell;
use std::rc::Rc;
struct Value<'a> { n: u32, clones: &'a Cell<u32>, drops: &'a Cell<u32> }
impl Clone for Value<'_> {
    fn clone(&self) -> Self {
        self.clones.set(self.clones.get() + 1);
        Value { n: self.n, clones: self.clones, drops: self.drops }
    }
}
impl Drop for Value<'_> { fn drop(&mut self) { self.drops.set(self.drops.get() + 1); } }
fn main() {
    let clones = Cell::new(0);
    let drops = Cell::new(0);
    let mut a = Rc::new(Value { n: 7, clones: &clones, drops: &drops });
    Rc::make_mut(&mut a).n += 1;
    assert_eq!(clones.get(), 0);
    let b = Rc::clone(&a);
    assert!(Rc::ptr_eq(&a, &b));
    assert_eq!(Rc::strong_count(&a), 2);
    assert!(Rc::get_mut(&mut a).is_none());
    Rc::make_mut(&mut a).n += 10;
    assert!(!Rc::ptr_eq(&a, &b));
    assert_eq!((a.n, b.n, clones.get()), (18, 8, 1));
    let c = Rc::clone(&b);
    let extracted = Rc::unwrap_or_clone(c);
    assert_eq!(extracted.n, 8);
    assert_eq!(clones.get(), 2);
    assert_eq!(Rc::strong_count(&b), 1);
    let moved = Rc::unwrap_or_clone(b);
    assert_eq!(clones.get(), 2);
    let shared = Rc::clone(&a);
    let returned = match Rc::try_unwrap(shared) { Err(owner) => owner, Ok(_) => panic!("shared") };
    assert_eq!(Rc::strong_count(&returned), 2);
    drop(returned);
    let unique = Rc::try_unwrap(a).ok().unwrap();
    assert_eq!(unique.n, 18);
    assert_eq!(drops.get(), 0);
    drop(unique); drop(extracted); drop(moved);
    assert_eq!(drops.get(), 3);
    let zst = Rc::new(());
    let other = zst.clone();
    assert!(Rc::ptr_eq(&zst, &other));
    drop(zst); drop(other);
    let mut sequence = Rc::new(vec![3u32, 5]);
    let original = sequence.clone();
    Rc::make_mut(&mut sequence).push(7);
    assert_eq!(original.as_slice(), &[3, 5]);
    assert_eq!(sequence.as_slice(), &[3, 5, 7]);
    let mut value=Rc::new(Value{n:23,clones:&clones,drops:&drops});
    let weak=Rc::downgrade(&value);
    assert_eq!(weak.weak_count(),1);
    assert!(Rc::get_mut(&mut value).is_none());
    let before=clones.get();
    Rc::make_mut(&mut value).n+=1;
    assert_eq!(clones.get(),before);
    assert!(weak.upgrade().is_none());
    let survivor=Rc::downgrade(&value);
    let moved=Rc::try_unwrap(value).ok().unwrap();
    assert_eq!(moved.n,24);
    assert!(survivor.upgrade().is_none());
    drop(moved);
    let slice:Rc<[u8]>=Rc::from(vec![1,2,3]);
    let weak_slice=Rc::downgrade(&slice);
    assert_eq!(&*weak_slice.upgrade().unwrap(),&[1,2,3]);
    drop(slice);
    assert!(weak_slice.upgrade().is_none());
    let empty=std::rc::Weak::<u8>::new();
    assert!(empty.ptr_eq(&empty.clone()));
    assert!(empty.upgrade().is_none());
    println!("rc clones {} drops {}", clones.get(), drops.get());
}

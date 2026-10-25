use std::cell::Cell;
use std::sync::{LazyLock, Mutex, OnceLock, PoisonError, RwLock};
use std::sync::atomic::{AtomicUsize, Ordering};

static CALLS: AtomicUsize = AtomicUsize::new(0);
static LAZY: LazyLock<Mutex<Vec<usize>>> = LazyLock::new(init);
fn init() -> Mutex<Vec<usize>> {
    CALLS.fetch_add(1, Ordering::Relaxed);
    Mutex::new(vec![11, 29])
}
static EMPTY: OnceLock<Box<[usize]>> = OnceLock::new();
static NUMBER: RwLock<usize> = RwLock::new(3);

struct Payload<'a>(&'a Cell<usize>);
impl<'a> Drop for Payload<'a> {
    fn drop(&mut self) { self.0.set(self.0.get() + 1); }
}

fn main() {
    assert!(EMPTY.get().is_none());
    assert_eq!(EMPTY.get_or_init(|| vec![7usize, 13].into_boxed_slice())[1], 13);
    LAZY.lock().unwrap_or_else(PoisonError::into_inner).push(47);
    assert_eq!(LazyLock::force(&LAZY).lock().unwrap().len(), 3);
    assert_eq!(CALLS.load(Ordering::Relaxed), 1);
    let first = NUMBER.read().unwrap();
    let second = NUMBER.read().unwrap();
    assert_eq!(*first + *second, 6);
    drop(first);
    drop(second);
    *NUMBER.write().unwrap() += 5;
    assert_eq!(*NUMBER.read().unwrap(), 8);
    let mut local = Mutex::new(String::from("local"));
    local.get_mut().unwrap().push_str(" value");
    assert!(!local.is_poisoned());
    local.clear_poison();
    assert_eq!(local.into_inner().unwrap(), "local value");
    let mut rw = RwLock::<usize>::default();
    *rw.get_mut().unwrap() = 17;
    rw.clear_poison();
    assert!(!rw.is_poisoned());
    assert_eq!(rw.into_inner().unwrap(), 17);
    let default: LazyLock<usize> = LazyLock::default();
    assert_eq!(*default, 0);
    let drops = Cell::new(0);
    {
        let unused = LazyLock::new(|| Payload(&drops));
        drop(unused);
        assert_eq!(drops.get(), 0);
        let captured = Payload(&drops);
        let unused = LazyLock::new(move || captured);
        drop(unused);
        assert_eq!(drops.get(), 1);
        let initialized = LazyLock::new(|| Payload(&drops));
        LazyLock::force(&initialized);
    }
    assert_eq!(drops.get(), 2);
    let mut error = PoisonError::new(31usize);
    *error.get_mut() += 1;
    assert_eq!(*error.get_ref(), 32);
    assert_eq!(error.into_inner(), 32);
    println!("sync 1 3 8 17 2 32");
}

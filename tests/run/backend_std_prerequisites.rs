use std::cell::{Cell, OnceCell};

const EMPTY: OnceCell<String> = OnceCell::new();
struct Owned<'a>(&'a Cell<usize>);
impl Drop for Owned<'_> {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}
fn main() {
    let mut cell = EMPTY;
    assert!(cell.get().is_none());
    let calls = Cell::new(0);
    assert_eq!(
        cell.get_or_init(|| {
            calls.set(calls.get() + 1);
            String::from("first")
        }),
        "first"
    );
    assert_eq!(
        cell.get_or_init(|| {
            calls.set(99);
            String::new()
        }),
        "first"
    );
    assert_eq!(calls.get(), 1);
    assert_eq!(
        cell.set(String::from("second")),
        Err(String::from("second"))
    );
    cell.get_mut().unwrap().push('!');
    let clone = cell.clone();
    assert_eq!(clone.into_inner().unwrap(), "first!");
    assert_eq!(cell.take().unwrap(), "first!");
    assert!(cell.get().is_none());
    assert_eq!(format!("{:?}", cell), "OnceCell(<uninit>)");
    cell.set(String::from("again")).unwrap();
    assert_eq!(format!("{:?}", cell), "OnceCell(\"again\")");
    let drops = Cell::new(0);
    {
        let c = OnceCell::new();
        c.set(Owned(&drops)).ok().unwrap();
        assert!(c.set(Owned(&drops)).is_err());
        assert_eq!(drops.get(), 1);
    }
    assert_eq!(drops.get(), 2);
    let path = std::env::current_dir().unwrap();
    assert!(path.is_absolute());
    assert_eq!(std::fs::canonicalize(".").unwrap(), path);
    println!("backend std {} {}", calls.get(), drops.get());
}

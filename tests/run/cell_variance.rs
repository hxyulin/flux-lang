use std::cell::{Cell, Ref, RefCell, RefMut};
use std::sync::{Arc, Weak};

fn same<'a>(x: Cell<&'a u32>) -> Cell<&'a u32> {
    x
}

fn equal<'a: 'b, 'b: 'a>(x: RefCell<&'b u32>) -> RefCell<&'a u32> {
    x
}

fn arc_shorten<'a, 'b: 'a>(x: Arc<&'b u32>) -> Arc<&'a u32> {
    x
}

fn weak_shorten<'a, 'b: 'a>(x: Weak<&'b u32>) -> Weak<&'a u32> {
    x
}

fn guard_shorten<'g, 'a, 'b: 'a>(x: Ref<'g, &'b u32>) -> Ref<'g, &'a u32> {
    x
}

fn mut_guard_shorten<'a, 'b: 'a>(x: RefMut<'b, u32>) -> RefMut<'a, u32> {
    x
}

fn observe(first: u32, second: u32) {
    // Both referents outlive every container and guard that holds them.
    let cell = same(Cell::new(&first));
    let shared = &cell;
    shared.set(&second);
    println!("{} {}", shared.replace(&first), cell.get());
    let nested = Cell::new(cell);
    println!("{}", nested.into_inner().get());

    let refs = equal(RefCell::new(&first));
    println!("{}", refs.replace(&second));
    {
        let guard = guard_shorten(refs.borrow());
        println!("{}", **guard);
    }
    *refs.borrow_mut() = &first;
    println!("{}", **refs.borrow());
    let number = RefCell::new(first);
    *mut_guard_shorten(number.borrow_mut()) = second;
    println!("{}", *number.borrow());

    let owner = Arc::new(Cell::new(&first));
    let weak = Arc::downgrade(&owner);
    let alias = owner.clone();
    alias.set(&second);
    println!("{} {}", owner.get(), weak.upgrade().unwrap().get());
    let owner = Arc::new(RefCell::new(&first));
    let weak = Arc::downgrade(&owner);
    let alias = owner.clone();
    println!("{}", alias.replace(&second));
    println!("{}", **weak.upgrade().unwrap().borrow());

    let covariant = arc_shorten(Arc::new(&first));
    let weak = weak_shorten(Arc::downgrade(&covariant));
    println!("{} {}", **covariant, **weak.upgrade().unwrap());
}

fn main() {
    observe(17, 29);
}

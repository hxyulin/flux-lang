//! WRONG REJECTION. An `impl Iterator<Item = &'static str> + 'a` built by
//! mapping a borrowed slice's iterator to a `&'static str` field.
//!
//! rustc: compiles; exits with status 11.
//! flux: "error: lifetime may not live long enough" at
//! `items.iter().map(|i| i.name)`. flux seems to give the closure's result
//! the lifetime of its parameter `&'a Item` instead of the field's
//! `'static`, so the item type does not meet `Item = &'static str`.
//! Annotating the closure's return type is "named lifetimes in types in
//! bodies are not supported yet". Found in round2/inventory.rs.

struct Item {
    name: &'static str,
    qty: u32,
}

fn names<'a>(items: &'a [Item]) -> impl Iterator<Item = &'static str> + 'a {
    items.iter().map(|i| i.name)
}

unsafe extern "C" {
    fn exit(code: i32) -> !;
}

fn main() {
    let items = [Item { name: "hammer", qty: 1 }, Item { name: "saw", qty: 2 }];
    let total: usize = names(&items).map(|n| n.len()).sum();
    unsafe { exit(total as i32 + items[1].qty as i32) }
}

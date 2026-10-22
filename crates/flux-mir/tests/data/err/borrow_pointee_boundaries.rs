// Shared references only permit reading through nested mutable ones.
fn shared_then_mut(r: &&mut i32) {
    **r = 2;
}

fn mut_then_shared(r: &mut &i32) {
    **r = 2;
}

fn raw_then_shared(p: *mut &i32) {
    unsafe { **p = 2 };
}

fn shared_then_const(r: &*const i32) {
    unsafe { **r = 2 };
}

fn borrow_shared_then_mut(r: &&mut i32) {
    let _ = &mut **r;
}
fn borrow_raw_then_shared(p: *mut &i32) {
    unsafe { let _ = &mut **p; }
}

// Deep borrows must still protect the pointee of a mutable reference.
struct S<'a> {
    value: &'a mut i32,
}
fn whole_mut(s: &mut S<'_>) {
    let r = &*s.value;
    let other = &mut *s;
    println!("{} {}", r, other.value);
}
fn whole_shared(s: &mut S<'_>) {
    let r = &mut *s.value;
    let other = &*s;
    println!("{} {}", r, other.value);
}

// Function items in iterator adapters must not erase input lifetimes.
fn escape_item() -> &'static str {
    let bytes = [b'a'];
    let text = core::str::from_utf8(&bytes).unwrap();
    [Ok::<_, ()>(text)].into_iter().map_while(Result::ok).next().unwrap()
}
fn keep(x: &str) -> Option<&str> { Some(x) }
fn escape_pointer() -> &'static str {
    let bytes = [b'b'];
    let text = core::str::from_utf8(&bytes).unwrap();
    let f: fn(&str) -> Option<&str> = keep;
    [text].into_iter().map_while(f).next().unwrap()
}
// Shortening the outer borrow must not shorten the invariant inner one.
fn identity<T>(x: T) -> Option<T> {
    Some(x)
}
fn invariant_item<'a>(p: &mut &'a i32) {
    let local = 0;
    let r = [p].into_iter().map_while(identity).next().unwrap();
    *r = &local;
}
fn shorten<'a, 'b: 'a>(x: &'b i32) -> Option<&'a i32> {
    Some(x)
}
fn escape_bound(p: &i32) -> &'static i32 {
    [p].into_iter().map_while(shorten).next().unwrap()
}
fn main() {}

//! Cloning function values copies the exact signature, including safety.

#[derive(Clone)]
struct Searcher {
    call: unsafe fn(&Searcher, &mut usize, &[u8], &[u8]) -> Option<usize>,
}

#[derive(Clone, Copy)]
struct Prefilter {
    call: unsafe fn(&Prefilter, &[u8]) -> Option<usize>,
}

struct NotClone(i32);

fn duplicate<T: Clone>(value: &T) -> T {
    value.clone()
}

fn copy_and_clone<T: Copy>(value: T) -> (T, T) {
    (value, duplicate(&value))
}

unsafe fn search(_: &Searcher, state: &mut usize, hay: &[u8], needle: &[u8]) -> Option<usize> {
    *state += 1;
    Some(hay.len() + needle.len() + *state)
}

unsafe fn prefilter(_: &Prefilter, hay: &[u8]) -> Option<usize> {
    Some(hay.len())
}

fn zero() -> i32 {
    7
}
fn one(n: NotClone) -> NotClone {
    NotClone(n.0 + 1)
}
fn two(a: i32, b: i32) -> i32 {
    a + b
}
fn three(a: i32, b: i32, c: i32) -> i32 {
    a + b + c
}
fn four(a: &i32, b: &mut i32, c: i32, d: i32) -> i32 {
    *b += c;
    *a + *b + d
}
unsafe fn unsafe_zero() -> i32 {
    11
}
fn identity(value: &i32) -> &i32 {
    value
}
fn named<'a>(value: &'a i32) -> i32 {
    *value
}

fn main() {
    let s = Searcher { call: search };
    let cloned = duplicate(&s);
    let p = Prefilter { call: prefilter };
    let (p1, p2) = copy_and_clone(p);
    let mut state = 0;
    unsafe {
        assert_eq!((cloned.call)(&s, &mut state, b"abcd", b"bc"), Some(7));
        assert_eq!((p1.call)(&p2, b"abc"), Some(3));
    }
    let f0: fn() -> i32 = zero;
    let f1: fn(NotClone) -> NotClone = one;
    let f2: fn(i32, i32) -> i32 = two;
    let f3: fn(i32, i32, i32) -> i32 = three;
    let f4: fn(&i32, &mut i32, i32, i32) -> i32 = four;
    let u0: unsafe fn() -> i32 = unsafe_zero;
    let safe_as_unsafe: unsafe fn() -> i32 = zero;
    let named_input: fn(&i32) -> i32 = named;
    assert_eq!(duplicate(&f0)(), 7);
    assert_eq!(duplicate(&f1)(NotClone(8)).0, 9);
    assert_eq!(f2.clone()(2, 3), 5);
    assert_eq!(f3.clone()(2, 3, 4), 9);
    let mut n = 5;
    assert_eq!(duplicate(&f4)(&2, &mut n, 3, 4), 14);
    assert_eq!(n, 8);
    unsafe {
        assert_eq!(duplicate(&u0)(), 11);
        assert_eq!(duplicate(&safe_as_unsafe)(), 7);
    }
    assert_eq!(duplicate(&named_input)(&n), 8);
    let mut replaced: fn(i32, i32) -> i32 = |a, b| a - b;
    replaced.clone_from(&f2);
    assert_eq!(replaced(9, 2), 11);
    // Function items have zero-sized values and clone without bounds on
    // their parameter or return types. Reference outputs remain tied to
    // their inputs when calling the cloned item (not a pointer).
    let (item, item2) = copy_and_clone(one);
    assert_eq!(item(NotClone(10)).0 + item2(NotClone(20)).0, 32);
    let unsafe_item = duplicate(&unsafe_zero);
    unsafe {
        assert_eq!(unsafe_item(), 11);
    }
    let id = duplicate(&identity);
    assert_eq!(*id(&n), 8);
    // Local inferred/elided pointer outputs use the existing conservative
    // input/output constraints; named pointer output signatures remain
    // unsupported. Cloning must preserve those constraints.
    let reference: fn(&i32) -> &i32 = identity;
    let reference_clone = duplicate(&reference);
    assert_eq!(*reference_clone(&n), 8);
    println!("function clones: {} {}", state, n);
}

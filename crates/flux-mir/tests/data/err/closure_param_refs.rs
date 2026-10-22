// Closures passed to generic functions, whose bounds say how their
// parameters' and results' lifetimes relate to the call's.

struct S {
    f: i32,
}

// A lifetime of the bound's own (`&Self::Item` of `filter`) kept in a
// capture.
fn escapes_to_capture() {
    let v = [1, 2];
    let mut keep: Option<&&i32> = None;
    let n = v
        .iter()
        .filter(|x| {
            keep = Some(x);
            true
        })
        .count();
    let _ = (n, keep);
}

// Returned where the bound's output is not the bound's own.
fn escapes_to_output() {
    let v = [1, 2];
    let m = v.iter().max_by_key(|x| x);
    let _ = m;
}

fn with_local<F: FnMut(&i32)>(mut f: F) {
    let x = 1;
    f(&x);
}

// A callee's own local, given to a closure that keeps it.
fn callee_local() {
    let mut keep: Option<&i32> = None;
    with_local(|x| keep = Some(x));
    let _ = keep;
}

// What a capture keeps of the items outlives what they borrow.
fn kept_items() {
    let mut keep: Option<&i32> = None;
    {
        let v = [1, 2];
        v.iter().for_each(|x| keep = Some(x));
    }
    let _ = keep;
}

// What `map`'s closure returns of its parameter outlives the array.
fn mapped_out() {
    let r;
    {
        let v = [S { f: 1 }];
        r = v.iter().map(|s| &s.f).next();
    }
    let _ = r;
}

// A result holding what the closure captures by reference, returned.
fn captured_local<'a>(r: Result<i32, i32>) -> Result<i32, &'a i32> {
    let x = 5;
    r.map_err(|_| &x)
}

// An item said to be `'static`, borrowed from `'a` data.
fn firsts<'a>(items: &'a [(u8, &'static str)]) -> impl Iterator<Item = &'static u8> + 'a {
    items.iter().map(|i| &i.0)
}

struct Applier<T, F> {
    f: F,
    last: Option<T>,
}

impl<T: Copy, F: FnMut(T)> Applier<T, F> {
    fn apply(&mut self, t: T) {
        self.last = Some(t);
        (self.f)(t)
    }
}

// A closure kept in a struct, given a local later through an `impl`
// block's bound.
fn given_later() {
    let mut out: Option<&i32> = None;
    let mut a = Applier {
        f: |x| out = Some(x),
        last: None,
    };
    {
        let local = 5;
        a.apply(&local);
    }
    drop(a);
    let _ = out;
}

fn first_ref<'a, I: Iterator<Item = &'a i32>>(mut it: I) -> Option<&'a i32> {
    it.next()
}

// Adapters' items, through a bound saying what they are.
fn adapted_out() {
    let r;
    {
        let v = [1, 2, 3];
        let m = v.iter().map(|x| x).filter(|x| **x > 1);
        r = first_ref(m);
    }
    let _ = r;
}

fn main() {}

// Errors involving `Clone`, `Copy`, `PartialEq` and `Eq`: derived impls
// whose fields do not implement the trait (reported at the field), derived
// impls missing their supertrait (at the struct's name), conflicting
// impls (at the derive's name), `Copy` impls for types with non-`Copy`
// fields, impls of `Clone` that do not match it, and uses of `clone`,
// `==` and `Copy` bounds on types without the impls they need. (`Copy`
// without `Clone` is not here: rustc skips that check once a `Copy` impl
// has E0204.)

struct NoTraits;

#[derive(PartialEq)]
struct OnlyPartialEq;

#[derive(Clone)]
struct OnlyClone;

#[derive(Clone, Copy, PartialEq, Eq)]
struct W<T>(T);

// E0277: a derived `Clone` needs each field to be `Clone`.
#[derive(Clone)]
struct CloneField {
    a: i32,
    b: NoTraits,
}

// E0204: a derived `Copy` needs each field to be `Copy`.
#[derive(Clone, Copy)]
struct CopyField {
    a: i32,
    b: OnlyClone,
}

// E0369: a derived `PartialEq` compares each field with `==`.
#[derive(PartialEq)]
struct EqField(i32, NoTraits);

// E0277: a derived `Eq` needs each field to be `Eq`.
#[derive(PartialEq, Eq)]
struct TotalField {
    a: u8,
    b: OnlyPartialEq,
}

// E0277: `Eq` needs `PartialEq`.
#[derive(Eq)]
struct EqAlone;

// E0119: a derive and an impl of the same trait.
#[derive(PartialEq)]
struct Twice;

impl PartialEq for Twice {
    fn eq(&self, _other: &Twice) -> bool {
        true
    }
}

// E0204: by hand, for a non-generic struct and for a parameter without
// the `Copy` bound.
struct Holds(OnlyClone);

impl Clone for Holds {
    fn clone(&self) -> Holds {
        Holds(self.0.clone())
    }
}

impl Copy for Holds {}

struct Gen<T>(T);

impl<T: Copy> Clone for Gen<T> {
    fn clone(&self) -> Gen<T> {
        Gen(self.0)
    }
}

impl<T: Copy> Copy for Gen<T> {}

struct Unbounded<T>(T);

impl<T: Clone> Clone for Unbounded<T> {
    fn clone(&self) -> Unbounded<T> {
        Unbounded(self.0.clone())
    }
}

impl<T: Clone> Copy for Unbounded<T> {}

// Impls of `Clone` that do not match the trait.
struct ByValue;

impl Clone for ByValue {
    fn clone(self) -> ByValue {
        ByValue
    }
}

struct WrongReturn;

impl Clone for WrongReturn {
    fn clone(&self) -> i32 {
        0
    }
}

struct Missing;

impl Clone for Missing {}

// E0599: `clone` on types that are not `Clone`: a struct, a parameter
// without the bound, and a derived impl whose bound is unmet.
fn no_clone(s: NoTraits, w: W<NoTraits>) {
    let _a = s.clone();
    let _b = w.clone();
}

fn param<T>(t: T) -> T {
    t.clone()
}

// E0277: `Copy` and `Clone` bounds.
fn need_copy<T: Copy>(_t: T) {}

fn need_clone<T: Clone>(_t: T) {}

fn bounds(w: W<NoTraits>, n: NoTraits, c: OnlyClone) {
    need_copy(w);
    need_clone(n);
    need_copy(c);
}

// E0599: a method of an impl block whose `Copy` bound is unmet.
impl<T: Copy> W<T> {
    fn get(&self) -> T {
        self.0
    }
}

fn unmet_impl_bound(w: W<OnlyClone>) {
    let _ = w.get();
}

// `==` on types without `PartialEq`, a derived impl whose bound is unmet,
// and a reference compared with a value.
fn compare(a: NoTraits, w: W<NoTraits>, p: &W<i32>, q: W<i32>) {
    let _a = a == a;
    let _b = w == w;
    let _c = p == q;
}

// E0308: a derived `clone` returns `Self`.
fn wrong_type(w: &W<i32>) -> W<u8> {
    w.clone()
}

// A derive on a struct whose field type does not resolve reports only
// that.
#[derive(Clone, PartialEq, Eq)]
struct Unresolved {
    x: Missing,
}

// Deeply nested, the derived impl's bounds are still checked: no
// `PartialEq` for the reference.
#[derive(Clone, PartialEq)]
struct Deep<T>(T);

fn deep(x: &Deep<Deep<Deep<Deep<Deep<Deep<Deep<Deep<Deep<NoTraits>>>>>>>>>) -> bool {
    x == x
}

fn main() {}

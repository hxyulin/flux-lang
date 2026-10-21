// Once a trait has conflicting impls (E0119), or `Copy` has an impl for a
// type that cannot be `Copy` (E0204), rustc checks that trait's impls no
// further: no missing items, mismatched signatures or unmet supertraits.

#[derive(Clone)]
struct OnlyClone;

#[derive(Clone)]
struct Twice;

impl Clone for Twice {
    fn clone(&self) -> Twice {
        Twice
    }
}

struct ByValue;

impl Clone for ByValue {
    fn clone(self) -> ByValue {
        ByValue
    }
}

struct MissingClone;

impl Clone for MissingClone {}

#[derive(Clone, Copy)]
struct CopyField(OnlyClone);

#[derive(Copy)]
struct CopyAlone;

struct W<T>(T);

impl<T: Clone> Clone for W<T> {
    fn clone(&self) -> W<T> {
        W(self.0.clone())
    }
}

impl<T> Copy for W<T> {}

fn main() {}

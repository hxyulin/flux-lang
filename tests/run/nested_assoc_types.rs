//! Associated types bounded by their own trait, nested deeper than
//! bounds were once expanded: methods and operators of
//! `<<<T as Dim>::Smaller as Dim>::Larger as Dim>::...` come from the
//! bounds of its trait's declaration, found where they are needed.

use core::ops::Add;

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

trait Dim: Clone + Copy {
    type Smaller: Dim;
    type Larger: Dim;
    type N: Add<Output = i32> + Copy;
    fn n(self) -> Self::N;
    fn smaller(self) -> Self::Smaller;
    fn larger(self) -> Self::Larger;
}

#[derive(Clone, Copy)]
struct D(i32);

#[derive(Clone, Copy)]
struct Up(i32);

#[derive(Clone, Copy)]
struct M(i32);

impl Add for M {
    type Output = i32;
    fn add(self, o: M) -> i32 {
        self.0 + o.0
    }
}

impl Dim for D {
    type Smaller = D;
    type Larger = Up;
    type N = M;
    fn n(self) -> M {
        M(self.0)
    }
    fn smaller(self) -> D {
        D(self.0 - 1)
    }
    fn larger(self) -> Up {
        Up(self.0 + 1)
    }
}

impl Dim for Up {
    type Smaller = D;
    type Larger = Up;
    type N = M;
    fn n(self) -> M {
        M(self.0 * 10)
    }
    fn smaller(self) -> D {
        D(self.0 - 1)
    }
    fn larger(self) -> Up {
        Up(self.0 + 1)
    }
}

fn deep<T: Dim>(t: T) -> i32 {
    let s = t.smaller().larger().smaller().larger().smaller().larger();
    let copy = s;
    let n = copy.n();
    n + s.n()
}

fn deeper<T: Dim>(
    x: <<<<<T as Dim>::Larger as Dim>::Larger as Dim>::Smaller as Dim>::Larger as Dim>::Smaller,
) -> i32 {
    let up = x.larger();
    let back = up.smaller().larger();
    (up.n() + up.n()) + (back.n() + back.n())
}

fn main() {
    unsafe {
        printf(c"%d %d %d\n".as_ptr(), deep(D(5)), deep(Up(5)), deeper::<D>(D(7)));
    }
}

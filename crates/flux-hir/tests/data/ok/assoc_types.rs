// Associated types in HIR: the path forms that name them (`Self::Out`,
// `T::Out`, `<T as Tr>::Out`, `<T>::Out`), bindings in bounds and `where`
// clauses, and trait parameters left to their defaults in bounds and impl
// headers.

struct S;

struct W<T>(T);

trait Named {
    fn name(&self) -> i32;
}

impl Named for S {
    fn name(&self) -> i32 {
        3
    }
}

trait Tr {
    type Out;
    type Inner: Named;

    fn get(&self) -> Self::Out;

    fn inner(&self) -> Self::Inner;

    fn both(&self) -> (Self::Out, <Self as Tr>::Out) {
        (self.get(), self.get())
    }
}

trait Convert<T = i32> {
    fn convert(&self) -> T;
}

impl Tr for S {
    type Out = i32;
    type Inner = S;

    fn get(&self) -> Self::Out {
        1
    }

    fn inner(&self) -> S {
        S
    }
}

impl<T: Tr> Tr for W<T> {
    type Out = T::Out;
    type Inner = T::Inner;

    fn get(&self) -> <T as Tr>::Out {
        self.0.get()
    }

    fn inner(&self) -> T::Inner {
        self.0.inner()
    }
}

impl Convert for S {
    fn convert(&self) -> i32 {
        2
    }
}

impl Convert<bool> for S {
    fn convert(&self) -> bool {
        true
    }
}

struct Flag;

impl Tr for Flag {
    type Out = bool;
    type Inner = S;

    fn get(&self) -> bool {
        true
    }

    fn inner(&self) -> S {
        S
    }
}

impl Convert for Flag {
    fn convert(&self) -> i32 {
        4
    }
}

impl Convert<bool> for Flag {
    fn convert(&self) -> bool {
        true
    }
}

fn forms<T: Tr>(t: &T) -> <T>::Out {
    let a: T::Out = t.get();
    let b: <T as Tr>::Out = a;
    let _c: i32 = <T::Inner as Named>::name(&t.inner());
    b
}

fn bindings<T: Tr<Out = i32>, U>(t: &T, u: &U) -> i32
where
    U: Tr<Out = bool> + Convert + Convert<bool>,
{
    let n: i32 = u.convert();
    let b: bool = Convert::<bool>::convert(u);
    if b && u.get() { t.get() + n } else { 0 }
}

fn concrete() -> i32 {
    let x: <S as Tr>::Out = S.get();
    let y: <W<S> as Tr>::Out = W(S).get();
    x + y + <S as Convert>::convert(&S)
}

fn main() {
    let _ = forms(&S) + concrete() + bindings(&S, &Flag);
    let _ = S.both();
}

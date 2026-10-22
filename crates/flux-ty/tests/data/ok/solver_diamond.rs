struct S;
trait A0 {}
trait B0 {}
impl A0 for S {}
impl B0 for S {}
trait A1 {}
impl<T: A0 + B0> A1 for T {}
trait B1 {}
impl<T: A0 + B0> B1 for T {}
trait A2 {}
impl<T: A1 + B1> A2 for T {}
trait B2 {}
impl<T: A1 + B1> B2 for T {}
trait A3 {}
impl<T: A2 + B2> A3 for T {}
trait B3 {}
impl<T: A2 + B2> B3 for T {}
trait A4 {}
impl<T: A3 + B3> A4 for T {}
trait B4 {}
impl<T: A3 + B3> B4 for T {}
trait A5 {}
impl<T: A4 + B4> A5 for T {}
trait B5 {}
impl<T: A4 + B4> B5 for T {}
trait A6 {}
impl<T: A5 + B5> A6 for T {}
trait B6 {}
impl<T: A5 + B5> B6 for T {}
trait A7 {}
impl<T: A6 + B6> A7 for T {}
trait B7 {}
impl<T: A6 + B6> B7 for T {}
trait A8 {}
impl<T: A7 + B7> A8 for T {}
trait B8 {}
impl<T: A7 + B7> B8 for T {}
trait A9 {}
impl<T: A8 + B8> A9 for T {}
trait B9 {}
impl<T: A8 + B8> B9 for T {}
trait A10 {}
impl<T: A9 + B9> A10 for T {}
trait B10 {}
impl<T: A9 + B9> B10 for T {}
trait A11 {}
impl<T: A10 + B10> A11 for T {}
trait B11 {}
impl<T: A10 + B10> B11 for T {}
trait A12 {}
impl<T: A11 + B11> A12 for T {}
trait B12 {}
impl<T: A11 + B11> B12 for T {}
trait A13 {}
impl<T: A12 + B12> A13 for T {}
trait B13 {}
impl<T: A12 + B12> B13 for T {}
trait A14 {}
impl<T: A13 + B13> A14 for T {}
trait B14 {}
impl<T: A13 + B13> B14 for T {}
trait A15 {}
impl<T: A14 + B14> A15 for T {}
trait B15 {}
impl<T: A14 + B14> B15 for T {}
trait A16 {}
impl<T: A15 + B15> A16 for T {}
trait B16 {}
impl<T: A15 + B15> B16 for T {}
trait Top {
    fn f(&self) {}
}
impl<T: A16 + B16> Top for T {}
fn main() {
    S.f();
}

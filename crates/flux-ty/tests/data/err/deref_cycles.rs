use core::ops::Deref;

struct A;
struct B;

impl Deref for A {
    type Target = B;
    fn deref(&self) -> &B {
        &B
    }
}

impl Deref for B {
    type Target = A;
    fn deref(&self) -> &A {
        &A
    }
}

struct Growing<T>(T);

impl<T> Deref for Growing<T> {
    type Target = Growing<Growing<T>>;
    fn deref(&self) -> &Self::Target {
        loop {}
    }
}

fn main() {
    let a = A;
    let _ = a.missing;
    let _ = a[0];
    let growing = Growing(0u8);
    let _ = growing.missing;
    let _ = growing[0];
}

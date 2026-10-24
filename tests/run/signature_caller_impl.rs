mod conditional {
    trait R {
        type X;
    }
    trait M {}
    trait P<X> {
        type A;
    }
    trait Q<X>: P<X, A = u8> {}
    impl<T: M> P<u8> for T {
        type A = u8;
    }
    pub fn forward<T, U, V>() -> u8
    where
        T: M,
        U: R<X = u8>,
        V: R<X = T>,
        V::X: Q<u8> + P<U::X, A = <V::X as P<U::X>>::A>,
    {
        37
    }
    pub fn reverse<T, U, V>() -> u8
    where
        T: M,
        U: R<X = u8>,
        V: R<X = T>,
        V::X: P<U::X, A = <V::X as P<U::X>>::A> + Q<u8>,
    {
        41
    }
    struct S;
    struct U;
    struct V;
    impl M for S {}
    impl Q<u8> for S {}
    impl R for U {
        type X = u8;
    }
    impl R for V {
        type X = S;
    }
    pub fn run() -> u8 {
        let a = forward::<S, U, V>();
        let b = reverse::<S, U, V>();
        assert_eq!(a, 37);
        assert_eq!(b, 41);
        a + b
    }
}
mod substituted {
    trait R {
        type X;
    }
    trait M {}
    trait P<X> {
        type A;
    }
    trait Q<X>: P<X, A = u8> {}
    impl<X: M> P<X> for X {
        type A = u8;
    }
    pub fn forward<T, U, V>() -> u8
    where
        T: M,
        U: R<X = T>,
        V: R<X = T>,
        V::X: Q<T> + P<U::X, A = <V::X as P<U::X>>::A>,
    {
        37
    }
    pub fn reverse<T, U, V>() -> u8
    where
        T: M,
        U: R<X = T>,
        V: R<X = T>,
        V::X: P<U::X, A = <V::X as P<U::X>>::A> + Q<T>,
    {
        41
    }
    struct S;
    struct U;
    struct V;
    impl M for S {}
    impl Q<S> for S {}
    impl R for U {
        type X = S;
    }
    impl R for V {
        type X = S;
    }
    pub fn run() -> u8 {
        let a = forward::<S, U, V>();
        let b = reverse::<S, U, V>();
        assert_eq!(a, 37);
        assert_eq!(b, 41);
        a + b
    }
}
fn main() {
    assert_eq!(conditional::run(), 78);
    assert_eq!(substituted::run(), 78);
    println!("signature impl 78 78");
}

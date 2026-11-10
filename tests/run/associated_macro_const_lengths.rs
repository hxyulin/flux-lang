use std::marker::PhantomData;
macro_rules! max {
    ($name:ident,$bytes:expr) => {
        pub const MAX: $name<O> = $name([0xFFu8; $bytes], PhantomData);
    };
}
macro_rules! def {
    ($name:ident,$bytes:expr) => {
        struct $name<O>([u8; $bytes], PhantomData<O>);
        impl<O> $name<O> {
            pub const ZERO: $name<O> = $name([0; $bytes], PhantomData);
            max!($name, $bytes);
        }
    };
}
def!(Usize, std::mem::size_of::<usize>());
fn main() {
    assert_eq!(Usize::<()>::MAX.0, [255u8; 8]);
}

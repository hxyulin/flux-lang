trait Mask {
    type Bool;
}

trait Field: Mask<Bool = bool> {}

trait Complex: Field {
    type Real: Field;
}

trait Simd: Mask {
    type Real: Mask<Bool = <Self as Mask>::Bool>;
}

impl<T: Complex> Simd for T {
    type Real = T::Real;
}

impl Mask for u32 {
    type Bool = bool;
}
impl Field for u32 {}
impl Complex for u32 {
    type Real = u32;
}

fn main() {
    let result: <u32 as Simd>::Real = 42;
    println!("{}", result);
}

// Associated types in `impl` headers need their traits (E0277), reported
// where they are written, while structs' bounds are reported at the
// whole type. A block whose header names one whose trait holds for no
// type overlaps no other, and one with an unconstrained parameter
// (E0207) is checked no further, as in rustc.

trait Tr {
    type Out: Copy;
}

trait Gen<U> {}

trait Marker {}

struct S<T>(T);

struct W<T: Copy>(T);

impl<T> Gen<W<T>> for S<T> {}

impl<T> Gen<(u8, <T as Tr>::Out)> for W<T> where T: Copy {}

impl<T> Gen<T> for S<<T as Tr>::Out> {}

impl<T> Marker for W<<T as Tr>::Out> {}

fn main() {}

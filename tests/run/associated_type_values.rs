//! Associated type projections can name values as well as appear in types.
trait Mask {
    const BITS: usize;
    fn new(value: u32) -> Self;
    fn first(self) -> usize;
}
trait Vector {
    type Mask: Mask;
    fn mask(self) -> Self::Mask;
}
struct Narrow(u32);
impl Mask for Narrow {
    const BITS: usize = 32;
    fn new(value: u32) -> Self { Narrow(value) }
    fn first(self) -> usize { self.0.trailing_zeros() as usize }
}
struct Wide(u64);
impl Mask for Wide {
    const BITS: usize = 64;
    fn new(value: u32) -> Self { Wide((value as u64) << 8) }
    fn first(self) -> usize { self.0.trailing_zeros() as usize }
}
struct A(u32);
struct B(u32);
impl Vector for A {
    type Mask = Narrow;
    fn mask(self) -> Narrow { Narrow(self.0) }
}
impl Vector for B {
    type Mask = Wide;
    fn mask(self) -> Wide { Wide(self.0 as u64) }
}
fn offsets<V: Vector>(value: V) {
    let topos = V::Mask::first;
    let make = <V as Vector>::Mask::new;
    println!("{} {} {}", V::Mask::BITS, topos(value.mask()), topos(make(8)));
}
fn main() {
    offsets(A(core::hint::black_box(16)));
    offsets(B(core::hint::black_box(512)));
}

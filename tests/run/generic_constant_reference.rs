use std::marker::PhantomData;

struct Table { size: fn() -> usize }
fn size<T>() -> usize { std::mem::size_of::<T>() }
struct Owner<T>(PhantomData<T>);
impl<T> Owner<T> {
    const TABLE: Table = Table { size: size::<T> };
}
fn table<T>() -> &'static Table { &Owner::<T>::TABLE }
fn literal<T>() -> &'static Table { &Table { size: size::<T> } }

trait Data { const VALUES: [u32; 2]; }
impl Data for u8 { const VALUES: [u32; 2] = [7, 11]; }
impl Data for u64 { const VALUES: [u32; 2] = [17, 23]; }
fn values<T: Data>() -> &'static [u32; 2] { &T::VALUES }

struct Numbers<const N: u32>;
impl<const N: u32> Numbers<N> {
    const VALUES: [u32; 2] = [N, N + 1];
}
fn number<const N: u32>() -> &'static u32 { &Numbers::<N>::VALUES[1] }

fn main() {
    let small = table::<u8>();
    let large = table::<u64>();
    assert_eq!((small.size)(), 1);
    assert_eq!((large.size)(), 8);
    assert_eq!((literal::<u8>().size)(), 1);
    assert_eq!((literal::<u64>().size)(), 8);
    assert_eq!(values::<u8>(), &[7, 11]);
    assert_eq!(values::<u64>(), &[17, 23]);
    assert_eq!(*number::<41>(), 42);
    println!("{} {} {} {}", (small.size)(), (large.size)(), values::<u64>()[1], number::<41>());
}

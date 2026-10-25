struct Pair<T, U = T>(T, U);
struct Bytes<T = u8, const N: usize = 3, const M: usize = N> {
    tag: T,
    first: [u8; N],
    second: [u8; M],
}
impl Bytes<u8, 3, 3> {
    fn new() -> Self { Self { tag: 9, first: [1, 2, 3], second: [4, 5, 6] } }
}
type Buffer<T = u8, const N: usize = 3> = Bytes<T, N>;
type Borrowed<'a, T = &'a u32, U = T> = (T, U);
fn keep<'a>(x: &'a u32) -> Borrowed<'a> { (x, x) }
enum Event<T, U = T> { First(T), Second(U) }
trait Dimension<const N: usize = 5> { fn dimension() -> usize { N } }
struct Marker;
impl Dimension for Marker {}
fn main() {
    let pair = Pair::<u16>(7, 8);
    let _: u16 = pair.1;
    let inferred: Pair<u32, u64> = Pair(11, 13);
    let bare = Pair(17u64, 19u32);
    let _: u32 = bare.1;
    let bytes: Buffer = Bytes::new();
    let wide = Bytes::<u16, 4> { tag: 23, first: [2; 4], second: [3; 4] };
    let _: [u8; 4] = wide.second;
    let event = Event::<u32>::Second(29);
    let first: Event<u32> = Event::First(31);
    let event_value = match event { Event::First(v) | Event::Second(v) => v };
    let first_value = match first { Event::First(v) | Event::Second(v) => v };
    let value = 37;
    let borrowed = keep(&value);
    assert_eq!(*borrowed.0 + *borrowed.1, 74);
    assert_eq!(bytes.first.len(), 3);
    assert_eq!(wide.second.len(), 4);
    assert_eq!(<Marker as Dimension>::dimension(), 5);
    println!("{} {} {} {} {} {} {} {} {} {} {} {}", pair.0, pair.1, inferred.0, inferred.1, bare.0, bare.1, bytes.tag, bytes.second[2], wide.tag, wide.second[3], event_value, first_value);
}

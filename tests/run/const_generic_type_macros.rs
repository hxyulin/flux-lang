struct Family<const N: u8>;
const fn family(s: &str) -> u8 { if s.len() == 3 { 1 } else { 6 } }
trait Send<R, O> { fn send(r: R) -> O; }
impl Send<u32, u32> for Family<6> { fn send(r: u32) -> u32 { r + 1 } }
impl Send<u32, u32> for Family<1> { fn send(r: u32) -> u32 { r + 2 } }
macro_rules! family { ($s:ident) => { Family<{family(stringify!($s))}> }; }
macro_rules! send { ($r:expr, $s:ident) => {{
    let result;
    result = <family!($s) as Send<_, _>>::send($r);
    result
}}; }
macro_rules! nested { () => { family!(new) }; }
macro_rules! array { () => { [u8; { 2 + 3 }] }; }
fn main() {
    let result: u32 = send!(41u32, number);
    let other: u32 = send!(40u32, new);
    let local = <nested!() as Send<_, _>>::send(5u32);
    let bytes: array!() = [1; 5];
    assert_eq!(result, 42);
    assert_eq!(other, 42);
    assert_eq!(local, 7);
    assert_eq!(bytes.len(), 5);
    println!("{} {} {} {}", result, other, local, bytes.len());
}

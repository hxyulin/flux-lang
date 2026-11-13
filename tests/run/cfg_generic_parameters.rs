// Conditional generic parameters are removed before names, defaults and bounds.
type Selected<#[cfg(any())] T, #[cfg_attr(all(), cfg(all()))] T = u16> = T;
struct Packet<
    #[cfg(any())] 'unused,
    'a,
    #[cfg(any())] T: Missing = Missing,
    T,
    #[cfg_attr(all(), cfg_attr(all(), cfg(any())))] const N: u8,
    const N: usize,
> {
    value: &'a T,
    bytes: [u8; N],
}
trait Read<#[cfg(any())] T, T> {
    fn read(&self) -> T;
}
impl<#[cfg(any())] T: Missing, T: Copy, const N: usize> Read<T> for Packet<'_, T, N> {
    fn read(&self) -> T { *self.value }
}
fn pass<#[cfg(any())] T: Missing, T>(value: T) -> T { value }
fn array<#[cfg(any())] const N: u8, const N: usize>() -> [u8; N] { [7; N] }
#[derive(Clone, Copy)]
struct Derived<#[cfg(any())] T, T>(T);
fn main() {
    let value: Selected = 29;
    let packet = Packet { value: &value, bytes: array::<3>() };
    let derived = Derived(pass(packet.read()));
    let copy = derived;
    println!("{} {} {}", copy.0, packet.bytes.len(), packet.bytes[2]);
}

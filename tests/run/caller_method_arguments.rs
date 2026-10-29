use std::sync::atomic::{AtomicUsize, Ordering};
static DROPS: AtomicUsize = AtomicUsize::new(0);
trait Acc<T> {
    fn add(&mut self, value: T);
}
trait Read<T> {
    fn read(&self) -> T;
}
struct Output(u32);
struct Owned(u32);
impl Drop for Owned {
    fn drop(&mut self) {
        DROPS.fetch_add(1, Ordering::SeqCst);
    }
}
impl Acc<u8> for Output {
    fn add(&mut self, value: u8) {
        self.0 += u32::from(value);
    }
}
impl Acc<u16> for Output {
    fn add(&mut self, value: u16) {
        self.0 += u32::from(value) * 10;
    }
}
impl Acc<Owned> for Output {
    fn add(&mut self, value: Owned) {
        self.0 += value.0;
    }
}
impl Read<u8> for Output {
    fn read(&self) -> u8 {
        self.0 as u8
    }
}
impl Read<u16> for Output {
    fn read(&self) -> u16 {
        self.0 as u16
    }
}
fn accumulate<N, E, O: Acc<N> + Acc<E>>(out: &mut O, n: N, e: E) {
    out.add(n);
    out.add(e);
}
fn accumulate_reversed<N, E, O: Acc<E> + Acc<N>>(out: &mut O, n: N, e: E) {
    out.add(e);
    out.add(n);
}
fn read<N, E, O: Read<N> + Read<E>>(out: &O) -> (N, E) {
    let n: N = out.read();
    let e: E = out.read();
    (n, e)
}
fn main() {
    let mut out = Output(0);
    accumulate(&mut out, 3u8, 7u16);
    assert_eq!(out.0, 73);
    accumulate_reversed(&mut out, 5u8, 11u16);
    assert_eq!(out.0, 188);
    accumulate(&mut out, 2u8, Owned(19));
    assert_eq!(out.0, 209);
    assert_eq!(DROPS.load(Ordering::SeqCst), 1);
    let (n, e): (u8, u16) = read(&out);
    assert_eq!(n, 209);
    assert_eq!(e, 209);
    println!(
        "caller {} {} {} drops {}",
        out.0,
        n,
        e,
        DROPS.load(Ordering::SeqCst)
    );
}

// A boxed or shared trait object of an Fn trait is called directly.
use std::sync::Arc;
trait G { type E; fn e(&self) -> Self::E; }
struct S;
impl G for S { type E = u32; fn e(&self) -> u32 { 4 } }
fn a<T: G, F>(g: T, mut f: F) -> u32 where F: FnMut(T::E) -> u32 { f(g.e()) }
fn b(f: Box<dyn Fn(u32) -> u32>) -> u32 { f(1) }
fn c(f: &Box<dyn Fn(u32) -> u32>) -> u32 { f(2) }
fn d(f: &mut Box<dyn FnMut(u32) -> u32>) -> u32 { f(3) }
fn e(f: &Arc<dyn Fn(u32) -> u32>) -> u32 { f(4) }
fn f2<F: FnMut(u32) -> u32>(f: &mut F) -> u32 { f(5) }
fn g<F: Fn(u32) -> u32>(f: &F) -> u32 { f(6) }
struct W<F>(F);
impl<F: FnMut(&mut dyn FnMut(u32))> W<F> { fn go(&mut self) -> u32 { let mut t = 0; (self.0)(&mut |x| t += x); t } }
fn h<F>(f: F) -> Option<u32> where F: FnOnce() -> u32 { Some(f()) }
fn main() {
    println!("{}", a(S, |x| x + 1));
    println!("{}", b(Box::new(|x| x * 10)));
    let bc: Box<dyn Fn(u32) -> u32> = Box::new(|x| x * 10);
    println!("{}", c(&bc));
    let mut k = 0;
    let mut bd: Box<dyn FnMut(u32) -> u32> = Box::new(move |x| { k += x; k });
    println!("{}", d(&mut bd));
    println!("{}", bd(7));
    let ae: Arc<dyn Fn(u32) -> u32> = Arc::new(|x| x + 100);
    println!("{}", e(&ae));
    println!("{}", ae(1));
    println!("{}", f2(&mut |x| x * 2));
    println!("{}", g(&|x| x * 3));
    println!("{}", W(|f: &mut dyn FnMut(u32)| { f(1); f(2) }).go());
    println!("{:?}", h(|| 9));
}

trait Limited {}
impl Limited for u8 {}
trait Compute { fn value(&self)->u32; }
struct Wrapper<T>(T);
impl<T:Limited> Wrapper<T> { fn value(&self)->u32{99} }
impl<T> Compute for Wrapper<T> { fn value(&self)->u32{7} }
fn generic<T>(value:&Wrapper<T>)->u32{value.value()}
fn constrained<T:Limited>(value:&Wrapper<T>)->u32{value.value()}
fn main(){
 assert_eq!(Wrapper(()).value(),7);
 assert_eq!(Wrapper(1u8).value(),99);
 assert_eq!(generic(&Wrapper(1u8)),7);
 assert_eq!(constrained(&Wrapper(1u8)),99);
 println!("{} {}",generic(&Wrapper(())),constrained(&Wrapper(1u8)));
}

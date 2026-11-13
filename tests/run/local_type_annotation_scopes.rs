use std::marker::PhantomData;
trait Deserialize<'de> { fn value() -> Self; }
trait Access<'de> {
    fn next<T: Deserialize<'de>>(&mut self) -> T { T::value() }
}
struct Input;
impl<'de> Access<'de> for Input {}
fn visit<'de, A: Access<'de>>(mut access: A) -> u16 {
    struct Wrapper<'de> { value: u8, lifetime: PhantomData<&'de ()> }
    type Alias<'de> = Wrapper<'de>;
    impl<'de> Deserialize<'de> for Wrapper<'de> {
        fn value() -> Self { Self { value: 37, lifetime: PhantomData } }
    }
    let outer: Alias<'de> = access.next::<Alias<'de>>();
    let inner = {
        struct Wrapper<'de> { value: u16, lifetime: PhantomData<&'de ()> }
        type Alias<'de> = Wrapper<'de>;
        impl<'de> Deserialize<'de> for Wrapper<'de> {
            fn value() -> Self { Self { value: 503, lifetime: PhantomData } }
        }
        let value: Alias<'de> = access.next::<Alias<'de>>();
        value.value
    };
    let after: Wrapper<'de> = access.next::<Wrapper<'de>>();
    outer.value as u16 + inner + after.value as u16
}
fn main() { println!("{}", visit(Input)); }

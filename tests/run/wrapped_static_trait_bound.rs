use core::marker::PhantomData;

struct Holder<T>(u32, PhantomData<T>);

trait Static: 'static {}

impl<T: 'static> Static for Holder<T> {}

trait Convert {
    type Output: Static;

    fn convert(self) -> Self::Output;
}

impl<T> Convert for Holder<T>
where
    Holder<T>: Static,
{
    type Output = Holder<T>;

    fn convert(self) -> Self::Output {
        Holder(self.0 + 1, self.1)
    }
}

fn main() {
    let holder = Holder::<u8>(6, PhantomData);
    println!("{}", holder.convert().0);
}

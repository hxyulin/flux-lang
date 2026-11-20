use core::marker::PhantomData;

struct Header<T: ?Sized>(PhantomData<T>);

impl<T: ?Sized> Header<T> {
    const FLAGS: u32 = 42;
}

macro_rules! global_flags {
    ($name:ident) => {
        static $name: u32 = Header::<dyn Fn() + 'static>::FLAGS;
    };
}

global_flags!(FLAGS);

trait TypeName {
    const NAME: &'static str;
}

struct Borrowed<'a>(&'a str);

impl TypeName for Borrowed<'_> {
    const NAME: &'static str = "borrowed";
}

const NAME: &'static str = <Borrowed<'static> as TypeName>::NAME;

fn main() {
    assert_eq!(FLAGS, 42);
    assert_eq!(Header::<&'static u8>::FLAGS, 42);
    println!("{FLAGS} {NAME}");
}

trait Encode {
    type Abi;

    fn encode(self) -> Self::Abi;
}

impl Encode for &u8 {
    type Abi = u32;

    fn encode(self) -> u32 {
        u32::from(*self)
    }
}

struct Token<'a>(&'a u8);

impl<'a> Encode for Token<'a> {
    type Abi = <&'a u8 as Encode>::Abi;

    fn encode(self) -> Self::Abi {
        self.0.encode()
    }
}

trait Optional: Encode {
    fn empty() -> Self::Abi;
}

impl Optional for &u8 {
    fn empty() -> Self::Abi {
        0
    }
}

impl<T: Optional> Encode for Option<T> {
    type Abi = T::Abi;

    fn encode(self) -> Self::Abi {
        match self {
            Some(value) => value.encode(),
            None => T::empty(),
        }
    }
}

trait Word<T = Self> {
    type Prim;
}

impl Word for u32 {
    type Prim = Self;
}

trait Family {
    type Out<T>;
}

impl Family for u32 {
    type Out<T> = T;
}

fn projected_values<'a>(value: &'a u8) -> (u32, u32, u32, u32) {
    let receiver: <<Token<'a> as Encode>::Abi as Word>::Prim = Token(value).encode();
    let argument: <u32 as Word<<&'a u8 as Encode>::Abi>>::Prim = receiver + 1;
    let family: <u32 as Family>::Out<<&'a u8 as Encode>::Abi> = argument + 1;
    let optional: <<Option<&'a u8> as Encode>::Abi as Word>::Prim = Some(value).encode();
    (receiver, argument, family, optional)
}

trait First {
    type Out;
}

trait Second {
    type Out;
}

impl<'a> First for &'a u8 {
    type Out = &'a u8;
}

impl<'a> Second for &'a u8 {
    type Out = &'a u8;
}

fn preserve_borrow<'a>(value: &'a u8) -> &'a u8 {
    let projected: <<&'a u8 as First>::Out as Second>::Out = value;
    projected
}

fn main() {
    let value = 7;
    let (a, b, c, d) = projected_values(&value);
    println!("{a} {b} {c} {d} {}", preserve_borrow(&value));
}

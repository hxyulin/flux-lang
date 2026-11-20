enum Signature {
    Number(u32),
    Owned(Box<u32>),
    Composite(&'static [&'static Signature]),
}

impl Signature {
    const fn composite(fields: &'static [&'static Signature]) -> Self {
        Self::Composite(fields)
    }
}

trait Type {
    const SIGNATURE: &'static Signature;
}

impl<T> Type for Vec<T> {
    const SIGNATURE: &'static Signature = &Signature::Number(3);
}

const SIGNATURE: &Signature =
    &Signature::composite(&[<Vec<i32>>::SIGNATURE, &Signature::Number(7)]);

fn main() {
    if let Signature::Composite(fields) = SIGNATURE
        && let Signature::Number(first) = fields[0]
        && let Signature::Number(second) = fields[1]
    {
        println!("{first} {second}");
    } else {
        panic!("expected two numeric signatures");
    }
}

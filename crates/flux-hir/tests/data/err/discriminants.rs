// Discriminants are lowered as constants in the enum's scope; a generic
// enum's may not name `Self` or its parameters, as rustc reports.

#[repr(u8)]
enum UsesSelf<T> {
    A = Self::C as u8,
    B(T),
    C,
}

#[repr(u8)]
enum UsesParam<T> {
    A = size::<T>(),
    B(T),
}

const fn size<T>() -> u8 {
    1
}

enum Unresolved {
    A = missing(),
    B = Self::Nope as isize,
}

fn main() {}

use std::mem::MaybeUninit;

trait Field<Key> {
    type Ty: ?Sized;
}

trait Layout {}
impl Layout for u32 {}

struct Record;

const _: () = {
    struct Key;
    impl Field<Key> for Record {
        type Ty = u32;
    }

    struct Storage(MaybeUninit<<Record as Field<Key>>::Ty>)
    where
        <Record as Field<Key>>::Ty: Layout;

    fn make() -> Storage {
        Storage(MaybeUninit::new(42))
    }

    impl Record {
        fn value() -> u32 {
            // SAFETY: make initializes the field.
            unsafe { make().0.assume_init() }
        }
    }
};

fn main() {
    println!("{}", Record::value());
}

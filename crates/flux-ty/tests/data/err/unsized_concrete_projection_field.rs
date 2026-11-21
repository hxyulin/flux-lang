use std::mem::MaybeUninit;

trait Field {
    type Ty: ?Sized;
}

struct Record;
impl Field for Record {
    type Ty = [u8];
}

struct Storage(MaybeUninit<<Record as Field>::Ty>);

fn main() {}

// Methods that take `self` by value need a sized `Self`: a trait object
// reached by dereferencing has none of them, and cannot be moved out.

trait Tr {
    fn by_value(self)
    where
        Self: Sized,
    {
    }
}

impl Tr for u8 {}
impl<T: Tr + ?Sized> Tr for &mut T {}

fn deref_dyn_iterator(r: &mut dyn Iterator<Item = i32>) {
    let _p = (*r).peekable();
}

fn deref_dyn_trait(r: &mut dyn Tr) {
    (*r).by_value();
}

fn move_out_of_box(b: Box<dyn Iterator<Item = i32>>) {
    let _x = *b;
}

fn move_out_of_param<T: ?Sized>(r: &mut T) {
    let _x = *r;
}

fn main() {}

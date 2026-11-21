use std::borrow::{Cow, ToOwned};

trait BorrowFrom<'a, T: ?Sized>: 'a {
    fn borrow_from(value: &'a T) -> Self;
}

impl<'a, B: ToOwned + ?Sized> BorrowFrom<'a, Cow<'_, B>> for Cow<'a, B> {
    fn borrow_from(value: &'a Cow<'_, B>) -> Self {
        Cow::Borrowed(value)
    }
}

fn main() {
    let owned: Cow<'_, str> = Cow::Owned(String::from("payload"));
    println!("{}", Cow::borrow_from(&owned));
}

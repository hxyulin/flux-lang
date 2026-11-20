trait Tag {
    fn tag(self) -> u8;
}

impl<A, R> Tag for &(dyn Fn(A) -> R + '_) {
    fn tag(self) -> u8 {
        1
    }
}

impl<A, R> Tag for &(dyn Fn(&A) -> R + '_) {
    fn tag(self) -> u8 {
        2
    }
}

trait BorrowTag: Tag {
    fn borrow_tag(self) -> u8;
}

impl<A, R> BorrowTag for &'_ (dyn Fn(&A) -> R + '_)
where
    Self: Tag,
{
    fn borrow_tag(self) -> u8 {
        self.tag()
    }
}

fn main() {
    let value: &dyn Fn(u8) -> u8 = &|x| x;
    let borrow: &dyn Fn(&u8) -> u8 = &|x| *x;
    let global: &dyn Fn(&'static u8) -> u8 = &|x| *x;
    println!("{} {} {} {}", value.tag(), borrow.tag(), global.tag(), borrow.borrow_tag());
}

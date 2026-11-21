use std::rc::Rc;

trait ReadValue {
    fn value(&self) -> i32;
}

trait IntoReader<'a, T> {
    fn into_reader(self: Rc<Self>) -> Rc<dyn ReadValue + 'a>;
    fn tag(&self) -> T;
}

struct Reader<'a>(&'a i32);

impl ReadValue for Reader<'_> {
    fn value(&self) -> i32 {
        *self.0
    }
}

impl<'a> IntoReader<'a, i32> for Reader<'a> {
    fn into_reader(self: Rc<Self>) -> Rc<dyn ReadValue + 'a> {
        self
    }

    fn tag(&self) -> i32 {
        7
    }
}

fn main() {
    let value = 35;
    let reader = Rc::new(Reader(&value));
    let tag = reader.tag();
    let erased = reader.into_reader();
    println!("{}", erased.value() + tag);
}

trait Family<'a> {
    type Slice: ?Sized;
}

struct Borrowed<'a, K: Family<'a>>(&'a K::Slice);

fn require_static<T: ?Sized + 'static>() {}

impl<'a, K: Family<'a>> Borrowed<'a, K>
where
    K::Slice: 'static,
{
    fn new(value: &'a K::Slice) -> Self {
        require_static::<K::Slice>();
        Self(value)
    }
}

struct Key;
impl Family<'_> for Key {
    type Slice = str;
}

fn main() {
    let input = String::from("borrowed");
    assert_eq!(Borrowed::<Key>::new(&input).0, "borrowed");
}

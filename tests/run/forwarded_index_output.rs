use std::ops::{Index, IndexMut};

struct Array<T>(Vec<T>);

impl<T, I> Index<I> for Array<T>
where
    [T]: Index<I>,
{
    type Output = <[T] as Index<I>>::Output;
    fn index(&self, index: I) -> &Self::Output {
        Index::index(self.0.as_slice(), index)
    }
}

impl<T, I> IndexMut<I> for Array<T>
where
    [T]: IndexMut<I>,
{
    fn index_mut(&mut self, index: I) -> &mut Self::Output {
        IndexMut::index_mut(self.0.as_mut_slice(), index)
    }
}

fn main() {
    let first = 20;
    let second = 20;
    let replacement = 22;
    let mut values = Array(vec![&first, &second]);
    values[0] = &replacement;
    let slice = &values[..];
    assert_eq!(slice, [&replacement, &second]);
    println!("{}", *slice[0] + *values[1]);
}

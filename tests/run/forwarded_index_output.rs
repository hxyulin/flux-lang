use std::ops::{Index, IndexMut};
use std::slice::SliceIndex;

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

fn vec_index<T, I: SliceIndex<[T]>>(values: &Vec<T>, index: I) -> &I::Output {
    values.index(index)
}

fn vec_index_mut<T, I: SliceIndex<[T]>>(values: &mut Vec<T>, index: I) -> &mut I::Output {
    values.index_mut(index)
}

struct BorrowedArray<'a>(Vec<Entry<'a>>);
struct Entry<'a>(&'a str);

impl<'a, I: SliceIndex<[Entry<'a>]>> Index<I> for BorrowedArray<'a> {
    type Output = I::Output;

    fn index(&self, index: I) -> &Self::Output {
        self.0.index(index)
    }
}

fn main() {
    let mut view = vec![1u32];
    AsMut::<Vec<u32>>::as_mut(&mut view).push(2);
    assert_eq!(AsRef::<Vec<u32>>::as_ref(&view).as_slice(), &[1, 2]);

    let mut vector = vec![1, 2, 3, 4];
    *vec_index_mut(&mut vector, 1) = 8;
    vec_index_mut(&mut vector, 2..4).copy_from_slice(&[9, 10]);
    assert_eq!(*vec_index(&vector, 1), 8);
    assert_eq!(vec_index(&vector, 1..=3), &[8, 9, 10]);


    let text = String::from("borrowed");
    let entries = BorrowedArray(vec![Entry(&text)]);
    assert_eq!(entries[0].0, "borrowed");

    let first = 20;
    let second = 20;
    let replacement = 22;
    let mut values = Array(vec![&first, &second]);
    values[0] = &replacement;
    let slice = &values[..];
    assert_eq!(slice, [&replacement, &second]);
    println!("{}", *slice[0] + *values[1]);
}

use core::iter::Map;
use core::slice::Iter;

struct Values<'a> {
    iter: Map<Iter<'a, u32>, fn(&u32) -> &u32>,
}
fn identity(value: &u32) -> &u32 {
    value
}
impl<'a> Iterator for Values<'a> {
    type Item = &'a u32;
    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }
}
fn values(items: &Vec<u32>) -> Values<'_> {
    Values {
        iter: items.iter().map(identity as fn(&u32) -> &u32),
    }
}
struct Occurrences<'a> {
    iter: Map<Iter<'a, Vec<u32>>, fn(&Vec<u32>) -> Values<'_>>,
}
impl<'a> Iterator for Occurrences<'a> {
    type Item = Values<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }
}
impl DoubleEndedIterator for Occurrences<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.iter.next_back()
    }
}
fn main() {
    let rows = vec![vec![11, 13], vec![17, 19]];
    let mut iter = Occurrences {
        iter: rows.iter().map(values as fn(&Vec<u32>) -> Values<'_>),
    };
    let mut back = iter.next_back().unwrap();
    let mut front = iter.next().unwrap();
    let a = front.next().unwrap();
    let b = back.next().unwrap();
    assert_eq!(*a, 11);
    assert_eq!(*b, 17);
    assert_eq!(a as *const u32, &rows[0][0] as *const u32);
    println!(
        "{} {} {} {}",
        a,
        b,
        front.next().unwrap(),
        back.next().unwrap()
    );
}

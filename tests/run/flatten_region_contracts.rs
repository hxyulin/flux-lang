use core::iter::{Flatten, Map};
use core::slice::Iter;
struct Values<'a> {
    iter: Map<Flatten<Iter<'a, Vec<u32>>>, fn(&u32) -> &u32>,
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
fn first(values: &[Vec<u32>]) -> Option<&u32> {
    values.iter().flatten().next()
}
fn first_mut(values: &mut [Vec<u32>]) -> Option<&mut u32> {
    values.iter_mut().flatten().next()
}
fn owned<'a>(values: Vec<Vec<&'a u32>>) -> Option<&'a u32> {
    values.into_iter().flatten().next()
}
fn main() {
    let mut data = vec![Vec::new(), vec![11, 13], vec![17]];
    assert_eq!(first(&data), Some(&11));
    *first_mut(&mut data).unwrap() += 12;
    let selected = first(&data).unwrap();
    assert_eq!(selected as *const u32, &data[1][0] as *const u32);
    let mut values = Values {
        iter: data.iter().flatten().map(identity as fn(&u32) -> &u32),
    };
    assert_eq!(values.next(), Some(&23));
    assert_eq!(values.next(), Some(&13));
    let n = 73;
    let reference = owned(vec![Vec::new(), vec![&n]]).unwrap();
    assert_eq!(reference as *const u32, &n as *const u32);
    println!("{} {} {}", selected, values.next().unwrap(), reference);
}

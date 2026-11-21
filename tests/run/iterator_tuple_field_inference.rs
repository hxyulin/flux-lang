trait Homogeneous {
    type Item;
    fn pair(item: Self::Item) -> Self;
}
impl<T: Copy> Homogeneous for (T, T) {
    type Item = T;
    fn pair(item: T) -> Self {
        (item, item)
    }
}
struct Windows<I, T>(I, std::marker::PhantomData<T>);
impl<I: Iterator, T: Homogeneous<Item = I::Item>> Iterator for Windows<I, T> {
    type Item = T;
    fn next(&mut self) -> Option<T> {
        self.0.next().map(T::pair)
    }
}
trait WindowsExt: Iterator + Sized {
    fn windows<T: Homogeneous<Item = Self::Item>>(self) -> Windows<Self, T> {
        Windows(self, std::marker::PhantomData)
    }
}
impl<I: Iterator> WindowsExt for I {}
fn run<P: Copy>(points: &[(P, P)]) -> Vec<(P, P, P, P)> {
    points
        .iter()
        .windows()
        .map(|(&j0, &j1)| (j0.0, j0.1, j1.0, j1.1))
        .collect()
}
fn main() {
    assert_eq!(run(&[(1, 2)]), vec![(1, 2, 1, 2)]);
}

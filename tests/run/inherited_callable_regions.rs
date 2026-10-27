trait Base {
    type Item;
}
trait Child: Base {
    fn run<B, F>(&self, initial: B, f: F) -> B
    where
        F: FnMut(B, Self::Item) -> B;
}
struct Pair<'a, 'b>(&'a u32, &'b u32);
impl<'a, 'b> Base for Pair<'a, 'b> {
    type Item = &'b u32;
}
impl Child for Pair<'_, '_> {
    fn run<B, F>(&self, initial: B, mut f: F) -> B
    where
        F: FnMut(B, Self::Item) -> B,
    {
        f(initial, self.1)
    }
}
struct One<'a>(Option<&'a u32>);
impl<'a> Iterator for One<'a> {
    type Item = &'a u32;
    fn next(&mut self) -> Option<Self::Item> {
        self.0.take()
    }
}
impl DoubleEndedIterator for One<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.0.take()
    }
    fn rfold<B, F>(self, initial: B, mut f: F) -> B
    where
        F: FnMut(B, Self::Item) -> B,
    {
        match self.0 {
            Some(value) => f(initial, value),
            None => initial,
        }
    }
}
fn invoke<'a, F: FnMut(&'a u32)>(mut f: F, x: &'a u32) {
    let mut inner = || f(x);
    inner();
}
fn main() {
    let longer = 29;
    let selected;
    {
        let shorter = 7;
        let pair = Pair(&shorter, &longer);
        selected = pair.run(None, |_, value| Some(value));
        assert_eq!(*pair.0, 7);
    }
    assert!(core::ptr::eq(selected.unwrap(), &longer));
    let sum = One(Some(&longer)).rfold(13, |sum, value| sum + *value);
    assert_eq!(sum, 42);
    let mut captured = 0;
    invoke(|x| captured += *x, &longer);
    assert_eq!(captured, 29);
    println!("inherited callbacks {} {}", sum, captured);
}

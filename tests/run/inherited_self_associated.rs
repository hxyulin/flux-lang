trait Base<T> { type Item; }
trait Mid<T>: Base<(T, u8)> {}
trait Top<T>: Mid<T> { fn item(&self) -> Self::Item; }
struct Concrete;
impl Base<(u32, u8)> for Concrete { type Item = u64; }
impl Mid<u32> for Concrete {}
impl Top<u32> for Concrete {
    fn item(&self) -> Self::Item {
        let value: Self::Item = 42;
        value
    }
}

struct Values<'a, T> { data: &'a [T] }
impl<'a, T> Iterator for Values<'a, T> {
    type Item = &'a T;
    fn next(&mut self) -> Option<Self::Item> {
        if self.data.is_empty() { return None; }
        let item = &self.data[0];
        self.data = &self.data[1..];
        Some(item)
    }
}
impl<T> DoubleEndedIterator for Values<'_, T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        let len = self.data.len();
        if len == 0 { return None; }
        let item = &self.data[len - 1];
        self.data = &self.data[..len - 1];
        Some(item)
    }
}

trait Left: Base<u16> {}
trait Right: Base<u16> {}
trait Diamond: Left + Right { fn value(&self) -> Self::Item; }
impl Base<u16> for Concrete { type Item = i32; }
impl Left for Concrete {}
impl Right for Concrete {}
impl Diamond for Concrete { fn value(&self) -> Self::Item { 19 } }

fn main() {
    println!("{} {}", Concrete.item(), Concrete.value());
    let input = [3, 5, 7, 11];
    let mut values = Values { data: &input };
    println!("{:?} {:?} {:?} {:?} {:?}", values.next_back(), values.next(), values.next_back(), values.next(), values.next_back());
}

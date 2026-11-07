struct Guard<T, D, F: FnMut(&D, &mut T)> { value: T, data: D, f: F }
impl<T, D, F: FnMut(&D, &mut T)> Drop for Guard<T, D, F> {
    fn drop(&mut self) { (self.f)(&self.data, &mut self.value); }
}
fn main() {
    let mut value = 0;
    {
        let _guard = Guard { data: 9usize, value: &mut value, f: |&n, target| **target = n as i32 };
    }
    println!("{}", value);
}

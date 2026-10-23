fn first<I: Iterator<Item = &'static i32>>(mut x: I) -> Option<&'static i32> { x.next() }
fn escape<'a>(x: &'a i32) -> &'static i32 {
    [[x].into_iter()].into_iter().find_map(first).unwrap()
}
fn main() {}

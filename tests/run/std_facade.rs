extern crate alloc;
extern crate std;

use std::fmt;
use std::iter::FusedIterator;
use std::mem;
use std::ops::Index;
use std::string::String;
use std::vec::Vec;

struct Number(i32);
impl fmt::Display for Number {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "number={}", self.0)
    }
}

fn next<I: FusedIterator<Item = i32>>(iter: &mut I) -> Option<i32> {
    iter.next()
}
fn index<T: Index<usize, Output = i32>>(values: &T) -> i32 {
    *values.index(1)
}
fn main() {
    let mut fused = Resuming(0).fuse();
    println!(
        "{:?} {:?} {:?}",
        next(&mut fused),
        next(&mut fused),
        next(&mut fused)
    );
    let mut values: Vec<i32> = vec![2, 4, 6];
    let original: alloc::vec::Vec<i32> = mem::take(&mut values);
    println!("{} {}", values.len(), index(&original));
    let mut iter = original.into_iter();
    println!("{:?} {:?}", next(&mut iter), next(&mut iter));
    println!(
        "{:?} {:?} {:?}",
        next(&mut iter),
        next(&mut iter),
        next(&mut iter)
    );
    let mut slice_iter = values.iter();
    fn slice_fused<I: FusedIterator>(_: &I) {}
    slice_fused(&slice_iter);
    println!("{:?} {:?}", slice_iter.next(), slice_iter.next());
    let text: String = std::fmt::format(format_args!("{}", Number(7)));
    let same: alloc::string::String = text;
    let boxed: std::boxed::Box<i32> = Box::new(9);
    let same_box: alloc::boxed::Box<i32> = boxed;
    println!("{} {} {}", same, same_box, std::thread::panicking());
    println!("{}", std::mem::size_of::<i32>());
}

// A source iterator may resume after None; Fuse must still stay exhausted.
struct Resuming(u32);
impl Iterator for Resuming {
    type Item = i32;
    fn next(&mut self) -> Option<i32> {
        self.0 += 1;
        if self.0 == 2 { None } else { Some(10) }
    }
}

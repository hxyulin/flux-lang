use std::cmp::Ordering;

fn compare<T: Ord>(left: T, right: T) -> Ordering { left.cmp(&right) }
fn main() {
    let values = [3u32, 5, 7];
    let first = &values[0] as *const u32;
    let second = &values[1] as *const u32;
    assert_eq!(compare(first, second), Ordering::Less);
    assert_eq!(compare(second, first), Ordering::Greater);
    assert_eq!(compare(first, first), Ordering::Equal);
    assert_eq!(compare(first as *mut u32, second as *mut u32), Ordering::Less);
    println!("pointer ordering passed");
}

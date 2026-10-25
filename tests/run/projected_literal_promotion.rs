fn whole() -> &'static [u32] { &[3u32, 5, 7][..] }
fn tail(start: usize) -> &'static [u32] { &[11u32, 13, 17][start..] }
fn element(index: usize) -> &'static u64 { &[19u64, 23, 29][index] }
fn empty<T>() -> &'static [T] { &[][..] }
fn main() {
    let values = Some(vec![31u32, 37]);
    let absent: Option<&Vec<u32>> = None;
    let fallback: &[u32] = absent.map_or(&[][..], |v| v);
    assert!(fallback.is_empty());
    let selected: &[u32] = values.as_ref().map_or(&[][..], |v| v);
    assert_eq!(selected, [31, 37]);
    assert_eq!(whole(), [3, 5, 7]);
    assert_eq!(tail(1), [13, 17]);
    assert_eq!(*element(2), 29);
    assert!(empty::<String>().is_empty());
    println!("promoted {:?} {:?} {} {}", whole(), tail(2), element(1), fallback.len());
}

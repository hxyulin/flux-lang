fn search<'a>(s: &'a [u32]) -> Option<&'a u32> {
    let mut seen = None;
    let _ = s.binary_search_by(|x| {
        seen = Some(x);
        x.cmp(&3)
    });
    seen
}
fn sorted<'a>(s: &'a [u32]) -> Option<&'a u32> {
    let mut seen = None;
    assert!(s.is_sorted_by(|a, b| {
        seen = Some(a);
        a <= b
    }));
    seen
}
fn main() {
    let values = [1u32, 2, 3, 4];
    assert!(search(&values).is_some());
    assert_eq!(sorted(&values), Some(&3));
    assert_eq!(values.binary_search_by_key(&&3, |x| x), Ok(2));
    assert!(values.is_sorted_by_key(|x| x));
    let mut visited = Vec::new();
    let _ = values.binary_search_by(|x| {
        visited.push(x);
        x.cmp(&2)
    });
    assert!(visited.iter().all(|x| values.contains(x)));
    println!(
        "slice callbacks {} {}",
        search(&values).unwrap(),
        sorted(&values).unwrap()
    );
}

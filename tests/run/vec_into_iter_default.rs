struct NoDefault;

fn main() {
    let mut iter = std::vec::IntoIter::<NoDefault>::default();
    assert_eq!(iter.len(), 0);
    assert!(iter.as_slice().is_empty());
    assert!(iter.next().is_none());
    assert!(iter.next_back().is_none());
}

fn main() {
    let values = [1, 2, 3, 4, 5];
    let mut windows = values.array_windows::<3>();
    assert_eq!(windows.len(), 3);
    assert_eq!(windows.next(), Some(&[1, 2, 3]));
    assert_eq!(windows.next_back(), Some(&[3, 4, 5]));
    assert_eq!(windows.clone().last(), Some(&[2, 3, 4]));
    assert_eq!(windows.next(), Some(&[2, 3, 4]));
    assert_eq!(windows.next_back(), None);
    assert_eq!(windows.next(), None);
    assert_eq!([1].array_windows::<2>().count(), 0);
    assert_eq!([0u8; 0].array_windows::<1>().count(), 0);
    assert_eq!([(); 4].array_windows::<2>().count(), 3);
    let collected: Vec<_> = values.array_windows::<2>().rev().copied().collect();
    assert_eq!(collected, [[4, 5], [3, 4], [2, 3], [1, 2]]);
    println!("array windows passed");
}

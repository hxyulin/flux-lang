use core::slice::GetDisjointMutError::{IndexOutOfBounds, OverlappingIndices};

fn main() {
    let mut values = [1, 2, 3, 4, 5];
    let [last, first] = values.get_disjoint_mut([4, 0]).unwrap();
    *last *= 10;
    *first *= 10;
    assert_eq!(values, [10, 2, 3, 4, 50]);

    let [tail, head] = values.get_disjoint_mut([3..5, 0..2]).unwrap();
    tail[0] = 40;
    head[1] = 20;
    let [middle, end] = values.get_disjoint_mut([2..=3, 4..=4]).unwrap();
    middle[0] = 30;
    end[0] = 51;
    assert_eq!(values, [10, 20, 30, 40, 51]);

    assert_eq!(values.get_disjoint_mut([1, 1]), Err(OverlappingIndices));
    assert_eq!(
        values.get_disjoint_mut([usize::MAX, 0]),
        Err(IndexOutOfBounds)
    );
    assert_eq!(values.get_disjoint_mut([1..4, 2..2]), Err(OverlappingIndices));
    let [empty, whole] = values.get_disjoint_mut([0..0, 0..5]).unwrap();
    assert!(empty.is_empty());
    assert_eq!(whole.len(), 5);

    let [one, two] = unsafe { values.get_disjoint_unchecked_mut([2, 3]) };
    *one += 1;
    *two += 2;
    let indices: [usize; 0] = [];
    let none = values.get_disjoint_mut(indices).unwrap();
    assert!(none.is_empty());
    assert!([(); 2].get_disjoint_mut([0, 1]).is_ok());
    println!("{values:?}");
}

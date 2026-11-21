use std::collections::VecDeque;

fn insert_wrapped() {
    let mut queue = VecDeque::with_capacity(4);
    queue.extend([1, 2, 3, 4]);
    assert_eq!(queue.pop_front(), Some(1));
    queue.push_back(5);
    queue.insert(1, 9);
    queue.insert(4, 8);
    queue.insert(0, 0);
    queue.insert(queue.len(), 6);
    assert_eq!(queue.into_iter().collect::<Vec<_>>(), [0, 2, 9, 3, 4, 8, 5, 6]);
}

fn main() {
    insert_wrapped();
    let mut queue = VecDeque::with_capacity(8);
    let capacity = queue.capacity();
    queue.extend(0..capacity);
    assert_eq!(queue.pop_front(), Some(0));
    assert_eq!(queue.pop_front(), Some(1));
    queue.push_back(capacity);
    queue.push_back(capacity + 1);

    assert_eq!(queue.binary_search_by_key(&2, |value| *value), Ok(0));
    assert_eq!(
        queue.binary_search_by_key(&capacity, |value| *value),
        Ok(capacity - 2)
    );
    assert_eq!(
        queue.binary_search_by_key(&(capacity + 1), |value| *value),
        Ok(capacity - 1)
    );
    assert_eq!(queue.binary_search_by_key(&0, |value| *value), Err(0));
    assert_eq!(
        queue.binary_search_by_key(&(capacity + 2), |value| *value),
        Err(capacity)
    );

    let mut range = queue.range(capacity - 3..=capacity - 1);
    assert_eq!(range.len(), 3);
    assert_eq!(range.next(), Some(&(capacity - 1)));
    assert_eq!(range.next_back(), Some(&(capacity + 1)));
    assert_eq!(range.next(), Some(&capacity));
    assert_eq!(range.next(), None);
    assert_eq!(queue.range(capacity..).next(), None);

    for value in queue.range_mut(capacity - 3..) {
        *value -= capacity - 2;
    }
    println!("{:?}", queue.range(capacity - 3..).collect::<Vec<_>>());
    assert_eq!(queue.remove(1), Some(3));
    assert_eq!(queue.remove(queue.len() - 2), Some(2));
    assert_eq!(queue.remove(queue.len()), None);
    println!("{:?}", queue.into_iter().collect::<Vec<_>>());
}

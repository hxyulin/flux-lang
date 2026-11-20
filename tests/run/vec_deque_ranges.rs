use std::collections::VecDeque;

fn main() {
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

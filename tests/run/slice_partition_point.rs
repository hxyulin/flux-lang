//! Slice partition boundaries and observable FnMut effects, without
//! depending on the search's predicate call count or order.

fn boundaries(values: &[i32]) {
    // Every prefix length, including empty, all and none, for both power
    // of two and non-power-of-two slice lengths.
    for len in 0..=values.len() {
        let slice = &values[..len];
        for end in 0..=len {
            let limit = end as i32;
            let point = slice.partition_point(|value| *value < limit);
            assert_eq!(point, end);
            println!("{} {} {}", len, end, point);
        }
    }
}

fn borrowed_predicate(values: &[i32], limit: &i32) {
    let mut calls = 0;
    let mut saw_match = false;
    let empty = values[..0].partition_point(|value| {
        calls += 1;
        *value < *limit
    });
    assert_eq!(calls, 0);
    let predicate = |value: &i32| {
        calls += 1;
        saw_match = true;
        *value < *limit
    };
    let first = values.partition_point(predicate);
    assert_eq!(empty, 0);
    assert_eq!(first, *limit as usize);
    // Only observe that a nonempty search calls and mutates the borrowed
    // predicate, since Rust does not specify how often it is called.
    println!("borrowed {} {} {} {}", empty, first, calls > 0, saw_match);
}

fn main() {
    let values = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
    boundaries(&values);
    borrowed_predicate(&values, &7);

    // A partition need not be sorted within either group.
    let grouped = [6, 2, 4, 9, 1, 7, 3];
    println!(
        "grouped {}",
        grouped.partition_point(|value| *value % 2 == 0)
    );

    // Vec deref and the tuple predicate used by IntervalSet::contains.
    let mut runs = Vec::new();
    runs.push((1, 3));
    runs.push((5, 8));
    runs.push((10, 12));
    for point in 0..14 {
        println!(
            "runs {} {}",
            point,
            runs.partition_point(|&(_, end)| end <= point)
        );
    }

    // Non-Copy elements are only borrowed by the predicate.
    let words = [String::from("a"), String::from("bb"), String::from("ccc")];
    println!(
        "words {} {}",
        words.partition_point(|word| word.len() < 3),
        words[2]
    );
}

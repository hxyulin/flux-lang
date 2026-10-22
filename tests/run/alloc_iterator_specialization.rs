//! Alloc owning iterators and drains retain trusted-length specialization
//! when Zip folds or compares iterators with observable closures.

fn collected<I: Iterator<Item = i64>>(iter: I) {
    let values: Vec<i64> = iter.collect();
    println!("{values:?} capacity {}", values.capacity());
}

fn main() {
    let count = vec![1, 2, 3]
        .into_iter()
        .map(|x| {
            println!("owned {x}");
            x
        })
        .zip(0..1)
        .count();
    println!("count {count}");

    let mut values = vec![4, 5, 6];
    let count = values
        .drain(..)
        .rev()
        .map(|x| {
            println!("drained {x}");
            x
        })
        .zip(0..1)
        .count();
    println!("count {count} remaining {values:?}");

    let equal = vec![7, 8, 9]
        .into_iter()
        .map(|x| {
            println!("compared {x}");
            x
        })
        .eq([7]);
    println!("equal {equal}");

    // Every source and adapter uses the same metadata for reservation.
    // Small collections distinguish exact reservation from minimum growth.
    collected([1i64].into_iter());
    collected([2i64, 3].iter().copied());
    collected([4i64, 5, 6].iter().skip(2).copied());
    collected((0i64..3).step_by(2));
    collected((0i128..3).step_by(2).map(|x| x as i64));
    collected(core::iter::once(7i64));
    collected(core::iter::repeat_n(8i64, 2));
    collected([[9i64], [10]].into_iter().flatten());
    collected((0..2).flat_map(|x| [x as i64]));
    collected([11i64, 12].into_iter().skip(1));
    collected(
        (0i64..1)
            .chain(2..3)
            .rev()
            .enumerate()
            .map(|(i, x)| x + i as i64),
    );
    collected((0i64..3).zip([1, 2]).map(|(x, y)| x + y).take(1));
    let mut source = [13i64].into_iter();
    let erased: &mut dyn Iterator<Item = i64> = &mut source;
    collected(erased);
}

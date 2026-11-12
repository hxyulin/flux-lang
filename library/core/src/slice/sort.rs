//! Sorting in place without allocating: a stable merge sort whose merges
//! rotate rather than copy into a buffer, with insertion sort for short
//! runs. Equal elements keep their order, so its results are those of
//! rustc's stable sort, and of its unstable sort where that keeps them
//! (20 elements or fewer, which it sorts by insertion too, comparing in
//! the same order).

use crate::ops::FnMut;

/// Runs this long or shorter are sorted by insertion, as rustc sorts
/// slices of up to 20 elements.
const INSERTION: usize = 20;

/// Sorts `v` stably by `is_less`.
pub(super) fn stable<T, F: FnMut(&T, &T) -> bool>(v: &mut [T], is_less: &mut F) {
    let len = v.len();
    if len < 2 {
        return;
    }
    if len <= INSERTION {
        insertion(v, is_less);
        return;
    }
    let mid = len / 2;
    stable(&mut v[..mid], is_less);
    stable(&mut v[mid..], is_less);
    merge(v, mid, is_less);
}

/// Inserts each element after the first into the sorted run before it,
/// as rustc's `insertion_sort_shift_left` does.
fn insertion<T, F: FnMut(&T, &T) -> bool>(v: &mut [T], is_less: &mut F) {
    let mut i = 1;
    while i < v.len() {
        let mut j = i;
        while j > 0 && (*is_less)(&v[j], &v[j - 1]) {
            v.swap(j, j - 1);
            j -= 1;
        }
        i += 1;
    }
}

/// Merges the sorted runs `v[..mid]` and `v[mid..]` stably: splits the
/// longer at its middle, finds where that element goes in the other,
/// rotates the parts between into place and merges both sides.
fn merge<T, F: FnMut(&T, &T) -> bool>(v: &mut [T], mid: usize, is_less: &mut F) {
    let len = v.len();
    if mid == 0 || mid == len || !(*is_less)(&v[mid], &v[mid - 1]) {
        return;
    }
    if len == 2 {
        v.swap(0, 1);
        return;
    }
    let (cut1, cut2);
    if mid > len - mid {
        cut1 = mid / 2;
        // The right run's elements less than the left's middle go
        // before it; equal ones stay after.
        let mut lo = mid;
        let mut hi = len;
        while lo < hi {
            let m = lo + (hi - lo) / 2;
            if (*is_less)(&v[m], &v[cut1]) {
                lo = m + 1;
            } else {
                hi = m;
            }
        }
        cut2 = lo;
    } else {
        cut2 = mid + (len - mid) / 2;
        // The left run's elements not greater than the right's middle
        // stay before it.
        let mut lo = 0;
        let mut hi = mid;
        while lo < hi {
            let m = lo + (hi - lo) / 2;
            if (*is_less)(&v[cut2], &v[m]) {
                hi = m;
            } else {
                lo = m + 1;
            }
        }
        cut1 = lo;
    }
    rotate(&mut v[cut1..cut2], mid - cut1);
    let new_mid = cut1 + (cut2 - mid);
    merge(&mut v[..new_mid], cut1, is_less);
    merge(&mut v[new_mid..], cut2 - new_mid, is_less);
}

/// Rotates `v` left by `mid`: three reversals.
pub(super) fn rotate<T>(v: &mut [T], mid: usize) {
    v[..mid].reverse();
    v[mid..].reverse();
    v.reverse();
}

/// Selects a rank with median-of-medians pivots and a three-way partition.
/// Moving the medians of groups of five to the front bounds each recursive
/// partition's size; equal values leave the active range in one step.
pub(super) fn select<T, F: FnMut(&T, &T) -> bool>(
    mut values: &mut [T], mut index: usize, is_less: &mut F,
) {
    loop {
        let len = values.len();
        if len <= 5 {
            insertion(values, is_less);
            return;
        }
        let groups = len / 5 + if len % 5 == 0 { 0 } else { 1 };
        for group in 0..groups {
            let start = group * 5;
            let end = (start + 5).min(len);
            insertion(&mut values[start..end], is_less);
            values.swap(group, start + (end - start) / 2);
        }
        let median = groups / 2;
        select(&mut values[..groups], median, is_less);
        values.swap(median, len - 1);
        let mut lower = 0;
        let mut scan = 0;
        let mut upper = len - 1;
        while scan < upper {
            if (*is_less)(&values[scan], &values[len - 1]) {
                values.swap(lower, scan);
                lower += 1;
                scan += 1;
            } else if (*is_less)(&values[len - 1], &values[scan]) {
                upper -= 1;
                values.swap(scan, upper);
            } else {
                scan += 1;
            }
        }
        values.swap(upper, len - 1);
        if index < lower {
            values = &mut values[..lower];
        } else if index > upper {
            index -= upper + 1;
            values = &mut values[upper + 1..];
        } else {
            return;
        }
    }
}

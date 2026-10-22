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

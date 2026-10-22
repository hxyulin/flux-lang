// Slice patterns' elements are places of their own: an array's moved
// element used again (E0382), an element moved out of a slice (E0508), a
// borrow of the array overlapping the rest's (E0502) or of the same
// element (E0499); disjoint elements borrowed mutably at once are fine.

struct S(u8);

fn take(_: S) {}

fn moved_twice(arr: [S; 3]) {
    let [x, ..] = arr;
    take(x);
    let [y, ..] = arr;
    take(y);
}

fn out_of_slice(v: &[S]) {
    match *v {
        [x, ..] => take(x),
        [] => {}
    }
}

fn overlapping(mut arr: [S; 3]) {
    let [first, rest @ ..] = &mut arr;
    let again = &arr[1];
    first.0 = 2;
    let _ = (rest, again);
}

fn same_element(s: &mut [S]) {
    if let [a, ..] = s {
        let b = &mut s[0];
        a.0 = 1;
        let _ = b;
    }
}

fn disjoint(s: &mut [S]) {
    if let [first, middle @ .., last] = s {
        first.0 = 1;
        last.0 = 2;
        let _ = middle;
    }
}

fn main() {}

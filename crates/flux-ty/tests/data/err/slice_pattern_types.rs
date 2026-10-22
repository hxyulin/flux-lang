// Slice patterns that cannot match their values' types: an array of
// another length (E0527, E0528), one of a generic length (E0730), and a
// value that is neither an array nor a slice (E0529).

fn f<const N: usize>(a: [u8; N]) {
    let [x, ..] = a;
    let _ = x;
}

fn main() {
    let [a, b] = [1, 2, 3];
    let [c, d, e, f, ..] = [1, 2, 3];
    let [g] = 5;
    let [h] = [1];
    let _ = (a, b, c, d, e, f, g, h);
}

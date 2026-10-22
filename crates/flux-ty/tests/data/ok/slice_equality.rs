// Arrays compare with slices and references to them, either way round,
// through the compiler's impls (rustc's `core` writes them with const
// generics); slices with slices through `core`'s.

fn main() {
    let a = [1, 2, 3];
    let s: &[i32] = &a;
    let m: &mut [i32] = &mut [1, 2, 3];
    let w: &[u8] = b"fn";
    let x = s == a;
    let y = a == s;
    let z = a == *s;
    let p = s[..] == a;
    let q = m != a;
    let r = a[..2] == s[..2];
    let t = &a[..] == s;
    let u = w == b"fn";
    let v = b"ab" == b"ab";
}

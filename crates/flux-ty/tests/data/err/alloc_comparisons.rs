// `alloc`'s comparisons of `String` with `str` and of `Vec` with slices
// and arrays give `str` and `[T]` more than one `PartialEq` impl, so a
// comparison's right side is not coerced, as in rustc.

fn strs(a: &str, b: &str) -> bool {
    a.eq(&b)
}

fn slices(a: &[i32], b: &[i32]) -> bool {
    a.eq(&b)
}

fn arrays(v: Vec<i32>) -> bool {
    [1, 2] == v
}

fn mut_arrays(v: Vec<i32>, a: &mut [i32; 2]) -> bool {
    v == a
}

fn main() {
    let s = String::from("a");
    let v = vec![1, 2];
    let mut m = [1, 2];
    let _ = s == "a" && "a" == s && s == *"a" && *"a" == s;
    let _ = v == [1, 2] && v == &[1, 2] && v == m[..] && m[..] == v;
    let _ = v == &m[..] && &m[..] == v && v == &mut m[..] && &mut m[..] == v;
    let _ = strs("a", "b") && slices(&[1], &[2]) && arrays(v.clone());
    let _ = mut_arrays(v, &mut m);
}

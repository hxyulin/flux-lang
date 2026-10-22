//! Array and slice patterns: fixed lengths, `..` with elements before
//! and after it, `rest @ ..` binding a slice's part or an array of the
//! rest, nested, by reference through default binding modes, in `match`
//! (with guards), `let` and `while let`.

fn sign(s: &[u8]) -> (bool, &[u8]) {
    match s {
        [b'-', rest @ ..] => (true, rest),
        all => (false, all),
    }
}

fn describe(s: &[i32]) -> i32 {
    match s {
        [] => 0,
        [x] => *x,
        [first, .., last] => first * 100 + last,
    }
}

fn middle(s: &[i32]) -> usize {
    match s {
        [_, mid @ .., _] => mid.len(),
        _ => 99,
    }
}

fn exact(a: [u8; 4]) -> u32 {
    let [a, b, c, d] = a;
    u32::from_be_bytes([a, b, c, d])
}

fn tail_sum(a: [i32; 5]) -> i32 {
    let [first, rest @ ..] = a;
    first * 1000 + rest.iter().sum::<i32>()
}

fn by_ref(a: &[String3; 3]) -> u8 {
    let [x, _, z] = a;
    x.0[0] + z.0[2]
}

struct String3([u8; 3]);

fn count_pairs(s: &[(u8, u8)]) -> u32 {
    let mut n = 0;
    let mut rest = s;
    while let [(a, b), tail @ ..] = rest {
        n += (*a + *b) as u32;
        rest = tail;
    }
    n
}

fn ends(s: &[i32]) -> i32 {
    match s {
        [.., x, y] if x < y => 1,
        [a, ..] if *a > 10 => 2,
        [_, _, ..] => 3,
        [_] => 4,
        [] => 5,
    }
}

fn mutate(s: &mut [i32]) {
    if let [first, ..] = s {
        *first += 100;
    }
}

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn show(label: i32, value: i64) {
    unsafe {
        printf(c"%d: %lld\n".as_ptr(), label, value);
    }
}

fn main() {
    let (neg, rest) = sign(b"-12");
    show(1, neg as i64 * 10 + rest.len() as i64);
    let (neg, rest) = sign(b"34");
    show(2, neg as i64 * 10 + rest.len() as i64);
    show(3, describe(&[]) as i64);
    show(4, describe(&[7]) as i64);
    show(5, describe(&[1, 2, 3]) as i64);
    show(6, middle(&[1, 2, 3, 4]) as i64);
    show(7, middle(&[1]) as i64);
    show(8, exact([1, 2, 3, 4]) as i64);
    show(9, tail_sum([1, 2, 3, 4, 5]) as i64);
    show(10, by_ref(&[String3([1, 2, 3]), String3([4, 5, 6]), String3([7, 8, 9])]) as i64);
    show(11, count_pairs(&[(1, 2), (3, 4), (5, 6)]) as i64);
    show(12, ends(&[1, 2]) as i64);
    show(13, ends(&[20, 1]) as i64);
    show(14, ends(&[5, 4, 3]) as i64);
    show(15, ends(&[5]) as i64);
    show(16, ends(&[]) as i64);
    let mut v = [1, 2, 3, 4];
    mutate(&mut v);
    show(17, v[0] as i64 * 10 + v[3] as i64);
    let [a, b, c, d] = [1, 2, 3, 4];
    show(18, (a + b + c + d) as i64);
    let arr = [[1, 2], [3, 4]];
    let [[p, _], [_, q]] = arr;
    show(19, (p * 10 + q) as i64);
}

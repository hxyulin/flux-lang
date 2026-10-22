//! Slices' `get` and `get_mut` with an index or any range (rustc's
//! `SliceIndex`), `str::get` with a range of byte offsets, and
//! `copy_within`, `chunks_exact_mut` (with `into_remainder`), `rchunks`
//! and `rchunks_exact`.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn show(label: i32, value: i64) {
    unsafe {
        printf(c"%d: %lld\n".as_ptr(), label, value);
    }
}

fn slice(label: i32, s: Option<&[i32]>) {
    match s {
        Some(s) => {
            show(label, s.len() as i64);
            for &x in s {
                show(label, x as i64);
            }
        }
        None => show(label, -1),
    }
}

fn nth(v: &[i32], i: usize) -> i32 {
    *v.get(i).unwrap_or(&-1)
}

fn main() {
    let v = [10, 20, 30, 40, 50];
    show(1, *v.get(0).unwrap_or(&0) as i64);
    show(2, v.get(9).is_none() as i64);
    show(3, nth(&v, 4) as i64);
    let n = 2;
    show(4, *v.get(n + 1).unwrap_or(&0) as i64);
    slice(5, v.get(1..3));
    slice(6, v.get(3..9));
    slice(7, v.get(..2));
    slice(8, v.get(4..));
    slice(9, v.get(6..));
    slice(10, v.get(..));
    slice(11, v.get(1..=2));
    slice(12, v.get(..=4));
    slice(13, v.get(..=5));
    slice(14, v.get(3..2));
    let mut w = [1, 2, 3, 4, 5, 6, 7];
    if let Some(part) = w.get_mut(2..4) {
        part[0] = 100;
        part[1] = 200;
    }
    if let Some(x) = w.get_mut(0) {
        *x = -1;
    }
    show(15, w.iter().sum::<i32>() as i64);
    let mut a = [1, 2, 3, 4];
    a.copy_within(1..4, 0);
    for x in a {
        show(16, x as i64);
    }
    let mut b = [1, 2, 3, 4, 5];
    b.copy_within(..3, 2);
    for x in b {
        show(17, x as i64);
    }
    let mut c = [1, 2, 3, 4, 5];
    c.copy_within(1..=2, 3);
    for x in c {
        show(18, x as i64);
    }
    let mut block = [0u8; 10];
    for (i, chunk) in block.chunks_exact_mut(4).enumerate() {
        chunk.copy_from_slice(&(i as u32 + 1).to_le_bytes());
    }
    for x in block {
        show(19, x as i64);
    }
    let mut it = block.chunks_exact_mut(3);
    show(20, it.len() as i64);
    if let Some(last) = it.next_back() {
        last[0] = 9;
    }
    let rem = it.into_remainder();
    show(21, rem.len() as i64);
    rem[0] = 7;
    show(22, block[6] as i64 * 100 + block[9] as i64);
    let r = [1, 2, 3, 4, 5];
    for chunk in r.rchunks(2) {
        show(23, chunk.len() as i64 * 10 + chunk[0] as i64);
    }
    for chunk in r.rchunks(2).rev() {
        show(24, chunk.len() as i64 * 10 + chunk[0] as i64);
    }
    let re = r.rchunks_exact(2);
    show(25, re.remainder().len() as i64);
    for chunk in re {
        show(26, chunk[0] as i64);
    }
    show(27, r.rchunks(3).len() as i64);
    let s = "héllo";
    show(28, s.get(0..1).map_or(-1, |p| p.len() as i64));
    show(29, s.get(1..2).is_none() as i64);
    show(30, s.get(1..3).map_or(-1, |p| p.len() as i64));
    show(31, s.get(3..).map_or(-1, |p| p.len() as i64));
    show(32, s.get(..=2).map_or(-1, |p| p.len() as i64));
    show(33, s.get(4..9).is_none() as i64);
    show(34, s.get(..).map_or(-1, |p| p.len() as i64));
}

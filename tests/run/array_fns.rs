//! Arrays' functions in `core`, written with const generics:
//! `array::from_fn`, `map`, `as_slice`, `as_mut_slice` and `each_ref`,
//! and the traits `core` implements for arrays of any length (`Clone`
//! of elements that are not `Copy`, `Default` up to 32 elements,
//! comparisons with arrays and slices, `IntoIterator` by value with
//! `array::IntoIter<T, N>`).

use core::array;

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

struct Noisy(i32);

impl Clone for Noisy {
    fn clone(&self) -> Noisy {
        unsafe { printf(c"clone %d\n".as_ptr(), self.0) };
        Noisy(self.0 + 10)
    }
}

impl Drop for Noisy {
    fn drop(&mut self) {
        unsafe { printf(c"drop %d\n".as_ptr(), self.0) };
    }
}

#[derive(Clone, Copy)]
struct Loud(i32);

fn total(values: &[usize]) -> usize {
    let mut sum = 0;
    for v in values {
        sum += *v;
    }
    sum
}

fn squares<const N: usize>() -> [u64; N] {
    array::from_fn(|i| (i * i) as u64)
}

fn main() {
    let doubled: [usize; 5] = array::from_fn(|i| i * 2);
    let lens = ["a", "bcd", "ef"].map(|s| s.len());
    let sq = squares::<4>();
    unsafe {
        printf(
            c"%d %d %d %d %lld\n".as_ptr(),
            total(&doubled) as i32,
            doubled.as_slice().len() as i32,
            total(lens.as_slice()) as i32,
            lens.each_ref()[1].clone() as i32,
            sq[3] as i64,
        )
    };

    let mut m = [1, 2, 3];
    let ms = m.as_mut_slice();
    ms[0] = 7;
    let refs = m.each_ref();
    unsafe { printf(c"%d %d\n".as_ptr(), *refs[0], *refs[2]) };

    let noisy = [Noisy(1), Noisy(2)];
    let copies = noisy.clone();
    let loud = [Loud(5); 3];
    let loud2 = loud.clone();
    unsafe { printf(c"%d %d\n".as_ptr(), copies[1].0, loud2[2].0) };

    let names = [Noisy(20), Noisy(21), Noisy(22)].map(|n| n.0 * 2);
    unsafe { printf(c"mapped %d %d\n".as_ptr(), names[0], names[2]) };

    let d: [i32; 32] = Default::default();
    let e: [Noisy; 0] = Default::default();
    let s: &[i32] = &[1, 2];
    unsafe {
        printf(
            c"%d %d %d %d %d %d\n".as_ptr(),
            d[31],
            e.len() as i32,
            (s == [1, 2]) as i32,
            ([1, 2] == s) as i32,
            ([1, 2] < [1, 3]) as i32,
            ([3u8, 4] == *b"\x03\x04") as i32,
        )
    };

    let mut it = [Noisy(30), Noisy(31), Noisy(32)].into_iter();
    let first = it.next();
    unsafe { printf(c"left %d\n".as_ptr(), it.as_slice().len() as i32) };
    drop(it);
    drop(first);
}

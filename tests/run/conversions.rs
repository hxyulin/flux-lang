//! rustc's prelude traits: `FromIterator` (`collect`, into the program's
//! types, `()`, and `Option` and `Result` of collections, stopping at the
//! first `None` or `Err`), `Extend` (with `unzip` and `partition`),
//! `Into`, `TryFrom` and `TryInto` (the integers' and `char`'s
//! conversions), `AsRef` and `AsMut`; and `Sum` and `Product` of
//! `Option`s and `Result`s.

use core::convert::TryFrom;

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn show(label: i32, value: i64) {
    unsafe {
        printf(c"%d: %lld\n".as_ptr(), label, value);
    }
}

/// Up to eight values, collected.
#[derive(Default)]
struct Bag {
    items: [i32; 8],
    len: usize,
}

impl Bag {
    fn total(&self) -> i64 {
        self.items[..self.len].iter().map(|&x| x as i64).sum()
    }
}

impl FromIterator<i32> for Bag {
    fn from_iter<I: IntoIterator<Item = i32>>(iter: I) -> Bag {
        let mut bag = Bag::default();
        bag.extend(iter);
        bag
    }
}

impl Extend<i32> for Bag {
    fn extend<I: IntoIterator<Item = i32>>(&mut self, iter: I) {
        for x in iter {
            show(900, x as i64);
            if self.len < 8 {
                self.items[self.len] = x;
                self.len += 1;
            }
        }
    }
}

struct Celsius(i32);

struct Fahrenheit(i32);

impl From<Celsius> for Fahrenheit {
    fn from(c: Celsius) -> Fahrenheit {
        Fahrenheit(c.0 * 9 / 5 + 32)
    }
}

fn to_f<T: Into<Fahrenheit>>(t: T) -> i32 {
    t.into().0
}

fn bytes_len<T: AsRef<[u8]>>(t: T) -> usize {
    t.as_ref().len()
}

fn text_len<S: AsRef<str>>(s: S) -> usize {
    s.as_ref().len()
}

fn zero_first<T: AsMut<[i32]>>(mut t: T) -> i32 {
    let s = t.as_mut();
    s[0] = 0;
    s.iter().sum()
}

fn conv<T: TryFrom<i64>>(x: i64) -> i64 {
    match T::try_from(x) {
        Ok(_) => 1,
        Err(_) => 0,
    }
}

fn digit(c: u8) -> Option<i32> {
    if c.is_ascii_digit() { Some((c - b'0') as i32) } else { None }
}

fn checked(x: i32) -> Result<i32, i32> {
    if x >= 0 { Ok(x) } else { Err(x) }
}

fn main() {
    let bag: Bag = (1..=4).collect();
    show(1, bag.total());
    let mut bag2 = Bag::default();
    bag2.extend([5, 6]);
    bag2.extend(bag.items[..2].iter().copied());
    show(2, bag2.total());
    let digits: Option<Bag> = "1234".bytes().map(digit).collect();
    show(3, digits.map_or(-1, |b| b.total()));
    let bad: Option<Bag> = "12x4".bytes().map(digit).collect();
    show(4, bad.is_none() as i64);
    let ok: Result<Bag, i32> = [1, 2, 3].iter().map(|&x| checked(x)).collect();
    show(5, ok.map_or(-1, |b| b.total()));
    let err: Result<Bag, i32> = [1, -2, 3, -4].iter().map(|&x| checked(x)).collect();
    show(6, err.err().unwrap_or(0) as i64);
    let () = [(), ()].into_iter().collect();
    let (evens, odds): (Bag, Bag) = (1..=6).partition(|x| x % 2 == 0);
    show(7, evens.total() * 100 + odds.total());
    let (left, right): (Bag, Bag) = [(1, 10), (2, 20)].into_iter().unzip();
    show(8, left.total() * 1000 + right.total());
    let sum: Option<i32> = [Some(1), Some(2), Some(3)].into_iter().sum();
    show(9, sum.unwrap_or(-1) as i64);
    let none: Option<i32> = [Some(1), None, Some(3)].into_iter().sum();
    show(10, none.is_none() as i64);
    let product: Result<i64, i32> = [Ok(2i64), Ok(5)].into_iter().product();
    show(11, product.unwrap_or(-1));
    let failed: Result<i64, i32> = [Ok(2i64), Err(7), Err(8)].into_iter().product();
    show(12, failed.err().unwrap_or(0) as i64);
    // Into.
    show(20, to_f(Celsius(100)) as i64);
    show(21, to_f(Fahrenheit(3)) as i64);
    let wide: i64 = 7i32.into();
    show(22, wide);
    let opt: Option<u8> = 4u8.into();
    show(23, opt.unwrap_or(0) as i64);
    let code: u32 = 'A'.into();
    show(24, code as i64);
    // TryFrom and TryInto.
    show(30, u8::try_from(300i32).is_err() as i64);
    show(31, u8::try_from(200i32).unwrap_or(0) as i64);
    show(32, i8::try_from(200u8).is_err() as i64);
    show(33, u32::try_from(-1i64).is_err() as i64);
    show(34, i64::try_from(u64::MAX).is_err() as i64);
    show(35, usize::try_from(5u32).unwrap_or(0) as i64);
    show(36, u16::try_from(7u8).unwrap_or(0) as i64);
    let t: Result<i16, _> = 40000u32.try_into();
    show(37, t.is_err() as i64);
    let t: Result<u8, _> = (-5i8).try_into();
    show(38, t.is_err() as i64);
    let t: Result<i128, _> = u128::MAX.try_into();
    show(39, t.is_err() as i64);
    show(40, conv::<i8>(-128) + conv::<i8>(-129) * 10 + conv::<u64>(5) * 100 + conv::<u16>(65536) * 1000);
    show(41, char::try_from(0x41u32).map_or(-1, |c| c as i64));
    show(42, char::try_from(0xD800u32).is_err() as i64);
    show(43, u8::try_from('é').unwrap_or(0) as i64);
    show(44, u8::try_from('ā').is_err() as i64);
    show(45, (u8::try_from(256u16) == u8::try_from(1000u32)) as i64);
    // AsRef and AsMut.
    show(50, bytes_len([0u8; 3]) as i64);
    show(51, bytes_len("four") as i64);
    show(52, bytes_len(&[1u8, 2][..]) as i64);
    show(53, text_len("hello") as i64);
    show(54, zero_first([5, 6, 7]) as i64);
    let mut arr = [9, 1];
    show(55, zero_first(&mut arr) as i64);
    show(56, arr[0] as i64);
}

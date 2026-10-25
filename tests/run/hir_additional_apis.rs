use std::cell::Cell;
use std::str::FromStr;
use std::sync::OnceLock;
struct Owner<'a>(&'a Cell<usize>, u32);
impl Drop for Owner<'_> { fn drop(&mut self) { self.0.set(self.0.get() + 1); } }
struct Parsed(usize);
impl FromStr for Parsed {
    type Err = std::num::ParseIntError;
    fn from_str(s: &str) -> Result<Parsed, Self::Err> { Ok(Parsed(s.parse::<usize>()? + 1)) }
}
fn main() {
    let mut option = Some(7u32);
    let mut iter = (&mut option).into_iter();
    assert_eq!(iter.len(), 1);
    *iter.next_back().unwrap() += 3;
    assert_eq!(iter.len(), 0); assert!(iter.next().is_none());
    assert_eq!((&option).into_iter().copied().sum::<u32>(), 10);
    let mut absent: Option<u32> = None;
    assert_eq!((&mut absent).into_iter().len(), 0);
    assert_eq!(Some(Ok::<u32,u32>(11)).transpose(), Ok(Some(11)));
    assert_eq!(Some(Err::<u32,u32>(13)).transpose(), Err(13));
    assert_eq!(None::<Result<u32,u32>>.transpose(), Ok(None));
    let drops = Cell::new(0);
    let owners: Box<[Owner<'_>]> = [Owner(&drops, 17), Owner(&drops, 19)].into();
    assert_eq!(owners[0].1 + owners[1].1, 36); drop(owners);
    assert_eq!(drops.get(), 2);
    let empty: Box<[u32]> = [].into(); assert!(empty.is_empty());
    let cell = OnceLock::<u32>::new();
    assert_eq!(format!("{cell:?}"), "OnceLock(<uninit>)");
    cell.get_or_init(|| 23);
    assert_eq!(format!("{cell:?}"), "OnceLock(23)");
    let pattern = String::from("é");
    assert!("héllo".contains(&pattern));
    assert_eq!("éé".replace(&pattern, "λ"), "λλ");
    assert_eq!("aaaaa".replace("aa", "b"), "bba");
    assert_eq!("aé".replace("", "|"), "|a|é|");
    assert_eq!("a1b2".replace(|c: char| c.is_ascii_digit(), "#"), "a#b#");
    assert_eq!("123".parse::<Parsed>().unwrap().0, 124);
    for text in ["", "+", "-", " 1", "1 ", "12x", "256", "-129", "+127", "-128", "0", "-0", "+0", "١"] {
        println!("parse {text:?} {:?} {:?}", text.parse::<i8>(), text.parse::<u8>());
    }
    assert_eq!("340282366920938463463374607431768211455".parse::<u128>().unwrap(), u128::MAX);
    assert_eq!("-170141183460469231731687303715884105728".parse::<i128>().unwrap(), i128::MIN);
    assert_eq!(u8::MIN.to_string().parse::<u8>().unwrap(), u8::MIN);
    assert_eq!(u8::MAX.to_string().parse::<u8>().unwrap(), u8::MAX);
    assert_eq!(u16::MIN.to_string().parse::<u16>().unwrap(), u16::MIN);
    assert_eq!(u16::MAX.to_string().parse::<u16>().unwrap(), u16::MAX);
    assert_eq!(u32::MIN.to_string().parse::<u32>().unwrap(), u32::MIN);
    assert_eq!(u32::MAX.to_string().parse::<u32>().unwrap(), u32::MAX);
    assert_eq!(u64::MIN.to_string().parse::<u64>().unwrap(), u64::MIN);
    assert_eq!(u64::MAX.to_string().parse::<u64>().unwrap(), u64::MAX);
    assert_eq!(u128::MIN.to_string().parse::<u128>().unwrap(), u128::MIN);
    assert_eq!(u128::MAX.to_string().parse::<u128>().unwrap(), u128::MAX);
    assert_eq!(usize::MIN.to_string().parse::<usize>().unwrap(), usize::MIN);
    assert_eq!(usize::MAX.to_string().parse::<usize>().unwrap(), usize::MAX);
    assert_eq!(i8::MIN.to_string().parse::<i8>().unwrap(), i8::MIN);
    assert_eq!(i8::MAX.to_string().parse::<i8>().unwrap(), i8::MAX);
    assert_eq!(i16::MIN.to_string().parse::<i16>().unwrap(), i16::MIN);
    assert_eq!(i16::MAX.to_string().parse::<i16>().unwrap(), i16::MAX);
    assert_eq!(i32::MIN.to_string().parse::<i32>().unwrap(), i32::MIN);
    assert_eq!(i32::MAX.to_string().parse::<i32>().unwrap(), i32::MAX);
    assert_eq!(i64::MIN.to_string().parse::<i64>().unwrap(), i64::MIN);
    assert_eq!(i64::MAX.to_string().parse::<i64>().unwrap(), i64::MAX);
    assert_eq!(i128::MIN.to_string().parse::<i128>().unwrap(), i128::MIN);
    assert_eq!(i128::MAX.to_string().parse::<i128>().unwrap(), i128::MAX);
    assert_eq!(isize::MIN.to_string().parse::<isize>().unwrap(), isize::MIN);
    assert_eq!(isize::MAX.to_string().parse::<isize>().unwrap(), isize::MAX);
    let sliced = "u64";
    assert_eq!(sliced[1..].parse::<u32>().unwrap(), 64);
    println!("additional {} {} {}", option.unwrap(), drops.get(), "31".parse::<Parsed>().unwrap().0);
}

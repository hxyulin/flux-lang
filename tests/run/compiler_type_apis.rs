use std::borrow::Cow;
use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};

trait FloatInfo {
    const MANTISSA_DIGITS: u32;
    const MIN_10_EXP: i32;
    const MAX_10_EXP: i32;
}
impl FloatInfo for f32 {
    const MANTISSA_DIGITS: u32 = Self::MANTISSA_DIGITS;
    const MIN_10_EXP: i32 = Self::MIN_10_EXP;
    const MAX_10_EXP: i32 = Self::MAX_10_EXP;
}
impl FloatInfo for f64 {
    const MANTISSA_DIGITS: u32 = Self::MANTISSA_DIGITS;
    const MIN_10_EXP: i32 = Self::MIN_10_EXP;
    const MAX_10_EXP: i32 = Self::MAX_10_EXP;
}
const WIDTH: usize = (<f64 as FloatInfo>::MANTISSA_DIGITS - 1) as usize;

fn main() {
    let iter: Box<dyn Iterator<Item = u32>> = Box::new(vec![1u32, 2, 3].into_iter());
    let collected: Vec<_> = iter.collect();
    assert_eq!(collected, vec![1, 2, 3]);
    let mut iter: Box<dyn DoubleEndedIterator<Item = u32>> =
        Box::new(vec![1u32, 2, 3, 4].into_iter());
    assert_eq!(iter.next_back(), Some(4));
    assert_eq!(iter.nth(1), Some(2));
    assert_eq!(iter.next(), Some(3));
    assert_eq!(iter.next_back(), None);
    let iter: Box<dyn ExactSizeIterator<Item = u32>> = Box::new(vec![5u32, 6].into_iter());
    assert_eq!(iter.len(), 2);
    let empty: &OsStr = Default::default();
    assert!(empty.is_empty());
    let text = String::from("héllo");
    let os = OsString::from(&text);
    assert!(os == "héllo");
    assert!("héllo" == os);
    assert!(os.as_os_str() == "héllo");
    assert!("héllo" == os.as_os_str());
    assert!(os == *os.as_os_str());
    assert!(*os.as_os_str() == os);
    let bad = OsString::from_vec(vec![b'h', 255]);
    assert!(bad != "h");
    assert_eq!(bad.as_os_str(), OsStr::from_bytes(&[b'h', 255]));
    let boxed = os.into_boxed_os_str();
    let clone = boxed.clone();
    assert_eq!(&*boxed, &*clone);
    assert_ne!(
        boxed.as_encoded_bytes().as_ptr(),
        clone.as_encoded_bytes().as_ptr()
    );
    let path = PathBuf::from("a//b");
    assert!(path == *Path::new("a/b"));
    assert!(*Path::new("a/b") == path);
    let path = path.into_boxed_path();
    let clone = path.clone();
    assert_eq!(&*path, &*clone);
    let boxed = String::from("owned 🦀").into_boxed_str();
    let address = boxed.as_ptr();
    let string = String::from(boxed);
    assert_eq!(string.as_ptr(), address);
    let borrowed: Cow<'_, str> = Cow::from(string.as_str());
    assert_eq!(borrowed.as_ptr(), string.as_ptr());
    let borrowed: Cow<'_, str> = Cow::from(&string);
    assert_eq!(borrowed.as_ptr(), string.as_ptr());
    let own: Cow<'static, str> = Cow::from(string);
    assert_eq!(own.as_ptr(), address);
    assert_eq!(f32::RADIX, 2);
    assert_eq!(f32::DIGITS, 6);
    assert_eq!(f64::DIGITS, 15);
    assert_eq!(f32::MIN_EXP, -125);
    assert_eq!(f32::MAX_EXP, 128);
    assert_eq!(f64::MIN_EXP, -1021);
    assert_eq!(f64::MAX_EXP, 1024);
    let mut chars = "aé🦀z".chars();
    chars.next();
    chars.next_back();
    println!("{:?} {:#?}", chars, chars);
    let mut indices = "aé🦀z".char_indices();
    indices.next();
    indices.next_back();
    println!("{:?} {:#?}", indices, indices);
    println!(
        "{} {} {} {} {} {}",
        own,
        [0u8; WIDTH].len(),
        <f32 as FloatInfo>::MANTISSA_DIGITS,
        <f32 as FloatInfo>::MIN_10_EXP,
        <f32 as FloatInfo>::MAX_10_EXP,
        <f64 as FloatInfo>::MAX_10_EXP
    );
}

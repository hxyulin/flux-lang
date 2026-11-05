// Arc and Rc raw-pointer round trips and conversions, Cow conversions and
// slice joins.
use std::borrow::Cow;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

#[repr(align(64))]
#[derive(Debug, PartialEq)]
struct Wide(u8);

fn main() {
    let shared = Arc::new(String::from("arc"));
    let raw = Arc::into_raw(shared.clone());
    println!("{} {}", Arc::strong_count(&shared), unsafe { &*raw });
    let back = unsafe { Arc::from_raw(raw) };
    println!("{} {}", back, Arc::ptr_eq(&back, &shared));
    unsafe { Arc::increment_strong_count(Arc::as_ptr(&shared)) };
    println!("{}", Arc::strong_count(&shared));
    unsafe { Arc::decrement_strong_count(Arc::as_ptr(&shared)) };
    drop(back);
    println!("{} {}", Arc::strong_count(&shared), Arc::weak_count(&shared));
    let weak = Arc::downgrade(&shared);
    println!("{} {:?}", Arc::weak_count(&shared), weak);

    let wide = Arc::new(Wide(9));
    let wide = unsafe { Arc::from_raw(Arc::into_raw(wide)) };
    println!("{:?}", wide);
    let slice: Arc<[u32]> = Arc::from(vec![1, 2, 3]);
    let slice = unsafe { Arc::from_raw(Arc::into_raw(slice)) };
    println!("{:?}", slice);

    let text: Arc<str> = Arc::from("text");
    let owned: Arc<str> = String::from("owned").into();
    let cow: Arc<str> = Arc::from(Cow::Borrowed("cow"));
    let bytes: Arc<[u8]> = Arc::from(text.clone());
    println!("{} {} {} {:?} {}", text, owned, cow, bytes, text < owned);
    let default: Arc<str> = Arc::default();
    println!("{:?} {}", default, Arc::<i32>::from(5));
    let os: Arc<OsStr> = Arc::from(OsStr::new("os"));
    let path: Arc<Path> = Arc::from(Path::new("a/b"));
    let path_buf: Arc<Path> = Arc::from(PathBuf::from("c/d"));
    println!("{:?} {:?} {:?}", os, path, path_buf);

    let rc = Rc::new(vec![1u8, 2]);
    let raw = Rc::into_raw(rc.clone());
    let back = unsafe { Rc::from_raw(raw) };
    println!("{:?} {} {}", back, Rc::strong_count(&rc), Rc::as_ptr(&rc) == Rc::as_ptr(&back));
    let rc_text: Rc<str> = Rc::from("rc");
    let rc_string: Rc<str> = Rc::from(String::from("string"));
    let rc_box: Rc<i32> = Rc::from(Box::new(4));
    println!("{} {:?} {} {}", rc_text, rc_string, rc_box, rc_text == Rc::from("rc"));
    let rc_os: Rc<OsStr> = Rc::from(OsString::from("rcos"));
    let rc_path: Rc<Path> = Rc::from(Path::new("rc/path"));
    let collected: Rc<[i32]> = (1..4).collect();
    println!("{:?} {:?} {:?} {:?}", rc_os, rc_path, collected, Rc::<Vec<u8>>::default());

    let borrowed: Cow<[u8]> = Cow::from(&b"xy"[..]);
    let vector: Vec<u8> = borrowed.clone().into();
    let string: String = Cow::Borrowed("cow string").into();
    let boxed: Box<str> = Cow::Borrowed("boxed").into();
    println!("{:?} {:?} {} {}", borrowed, vector, string, boxed);
    let mut sum: Cow<str> = Cow::Borrowed("");
    sum += "a";
    sum += "b";
    let sum = sum + "c";
    let empty: Cow<str> = Cow::default();
    println!("{} {} {:?} {}", sum, matches!(sum, Cow::Owned(_)), empty, Cow::Borrowed("a") < Cow::Borrowed("b"));
    let joined = [Cow::Borrowed(&b"ab"[..]), Cow::Owned(b"cd".to_vec())].join(&b' ');
    let joined_slice = [vec![1, 2], vec![3]].join(&[0, 0][..]);
    let words = ["x", "y", "z"].join("-");
    let strings = vec![String::from("p"), String::from("q")].join(", ");
    println!("{:?} {:?} {} {}", joined, joined_slice, words, strings);
    let collected: Cow<str> = "abc".chars().rev().collect();
    println!("{}", collected);

    let mut pinned = Box::pin(5);
    *pinned.as_mut() += 1;
    let from: std::pin::Pin<Box<str>> = Box::<str>::from("pinned text").into();
    println!("{} {:?} {} {}", pinned, from, pinned == Box::pin(6), format!("{:p}", Box::new(1)).starts_with("0x"));
}

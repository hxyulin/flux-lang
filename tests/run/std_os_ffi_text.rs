// Descriptor ownership, C types and strings, backtrace status, parsing
// bool and char, and String and Vec editing methods.
use std::backtrace::{Backtrace, BacktraceStatus};
use std::ffi::{CStr, CString};
use std::fs::File;
use std::io::Write;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, IntoRawFd, OwnedFd};
use std::os::raw::{c_char, c_int};

fn main() {
    let path = std::env::temp_dir().join(format!("flux-os-ffi-{}", std::process::id()));
    let file = File::create(&path).unwrap();
    let owned: OwnedFd = file.into();
    let borrowed: BorrowedFd<'_> = owned.as_fd();
    let duplicate = borrowed.try_clone_to_owned().unwrap();
    println!("{} {}", borrowed.as_raw_fd() >= 0, duplicate.as_raw_fd() != owned.as_raw_fd());
    let mut back = File::from(duplicate);
    back.write_all(b"via fd").unwrap();
    let raw = owned.into_raw_fd();
    drop(unsafe { OwnedFd::from_raw_fd(raw) });
    println!("{}", std::fs::read_to_string(&path).unwrap());
    std::fs::remove_file(&path).unwrap();
    let width: c_int = 7;
    let byte: c_char = 65;
    println!("{} {}", width, byte);

    let owned = CString::new("text").unwrap();
    let pointer = owned.as_ptr();
    let view = unsafe { CStr::from_ptr(pointer) };
    let unchecked = unsafe { CStr::from_bytes_with_nul_unchecked(b"raw\0") };
    let lossy = CString::new(vec![b'o', 0xff, b'k']).unwrap();
    println!("{:?} {:?} {} {}", view, unchecked, lossy.to_string_lossy(), unchecked.to_string_lossy());

    let trace = Backtrace::disabled();
    println!("{} {:?} {}", trace, trace, trace.status() == BacktraceStatus::Disabled);

    println!("{:?} {:?} {:?}", "true".parse::<bool>(), "false".parse::<bool>(), "yes".parse::<bool>());
    if let Err(error) = "maybe".parse::<bool>() { println!("{error}"); }
    println!("{:?} {:?} {:?}", "x".parse::<char>(), "".parse::<char>(), "xy".parse::<char>());
    if let Err(error) = "".parse::<char>() { println!("{error}"); }
    println!("{:?}", "owned".parse::<String>());

    let mut text = String::from("hello, wörld");
    let drained: String = text.drain(5..7).collect();
    println!("{:?} {:?}", drained, text);
    text.retain(|c| c != 'l');
    text.extend_from_within(..2);
    println!("{:?}", text);
    let mut tail = String::from("abc");
    let mut drain = tail.drain(..);
    println!("{:?} {:?} {:?}", drain.next_back(), drain.as_str(), drain);
    drop(drain);
    println!("{:?}", tail);
    println!("{:?} {:?}", String::from_utf16(&[104, 105, 0xD83D, 0xDE00]), String::from_utf16(&[0xD800]).is_err());
    if let Err(error) = String::from_utf16(&[0xDC00]) { println!("{error}"); }
    println!("{}", String::from_utf16_lossy(&[97, 0xD800, 98]));
    let joined: String = vec![std::borrow::Cow::Borrowed("a"), std::borrow::Cow::Owned("b".to_string())].into_iter().collect();
    println!("{}", joined);

    let mut values = vec![1, 1, 2, 3, 3, 4, 4, 4];
    values.dedup_by(|a, b| a == b);
    println!("{:?}", values);
    let mut words = vec!["apple", "avocado", "banana", "blueberry", "cherry"];
    words.dedup_by_key(|word| word.chars().next());
    println!("{:?}", words);
    let mut buffer: Vec<u8> = Vec::with_capacity(8);
    let spare = buffer.spare_capacity_mut();
    spare[0].write(9);
    spare[1].write(8);
    unsafe { buffer.set_len(2) };
    let spare = buffer.spare_capacity_mut().len();
    println!("{:?} {}", buffer, spare >= 6);
    let leaked: &'static mut [i32] = vec![5, 6].leak();
    leaked[0] += 1;
    println!("{:?}", leaked);
}

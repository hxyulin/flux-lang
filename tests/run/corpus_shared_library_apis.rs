use std::borrow::ToOwned;
use std::collections::HashMap;
use std::error::Error;
use std::ffi::CStr;
use std::process::Command;

fn describe(error: &dyn Error) { println!("{error}"); assert!(error.source().is_none()); }
fn main() {
    let mut owned = String::from("old");
    "updated".clone_into(&mut owned);
    assert_eq!(owned, "updated");
    let mut values = vec![1, 2, 3];
    [4, 5].as_slice().clone_into(&mut values);
    assert_eq!(values, vec![4, 5]);
    let source = vec![String::from("a"), String::from("b")];
    let mut target = vec![String::from("c")];
    source.clone_into(&mut target);
    assert_eq!(source, target);
    let map = HashMap::from([(1u32, String::from("first")), (1, String::from("last")), (2, String::from("other"))]);
    assert_eq!(map.len(), 2);
    assert_eq!(map[&1], "last");
    let empty: HashMap<u8, u8> = HashMap::from([]);
    assert!(empty.is_empty());
    describe(&CStr::from_bytes_with_nul(b"missing").unwrap_err());
    describe(&CStr::from_bytes_with_nul(b"a\0b\0").unwrap_err());
    describe(&CStr::from_bytes_until_nul(b"missing").unwrap_err());
    let mut command = Command::new("example program");
    command.arg("one").arg("two three");
    println!("{command:?}");
    println!("{command:#?}");
    command.env("FLUX_B", "two").env_remove("FLUX_A").current_dir("/tmp/example");
    println!("{command:?}");
    println!("{command:#?}");
    command.env_clear().env("FLUX_C", "three");
    println!("{command:?}");
}

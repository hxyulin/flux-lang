use std::convert::Infallible;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

fn main() {
    let name = std::ffi::OsStr::from_bytes(b"name\t\xff");
    let path = Path::new(name);
    assert_eq!(format!("{:?}", path.display()), format!("{path:?}"));

    for spelling in ["", "/é//file", "with\0nul"] {
        let parsed: Result<PathBuf, Infallible> = PathBuf::from_str(spelling);
        assert_eq!(parsed.unwrap().as_os_str().as_bytes(), spelling.as_bytes());
    }

    let root = std::env::temp_dir().join(format!("flux-remove-dir-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let child = root.join("child");
    fs::create_dir(&child).unwrap();
    assert!(fs::remove_dir(&root).is_err());
    assert!(child.is_dir());
    fs::remove_dir(&child).unwrap();
    fs::remove_dir(&root).unwrap();
    assert!(!root.exists());
    println!("{:?}", path.display());
}

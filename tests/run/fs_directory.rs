use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;
use std::os::unix::fs::{MetadataExt, symlink};
use std::time::{Duration, Instant};

fn main() {
    let root = std::env::temp_dir().join(format!("flux-directory-{}", std::process::id()));
    std::fs::create_dir_all(root.join("nested")).unwrap();
    std::fs::write(root.join("file"), b"payload").unwrap();
    let raw = OsString::from_vec(vec![b'n', 0xc3, 0xa9]);
    std::fs::write(root.join(&raw), b"raw").unwrap();
    symlink("file", root.join("link")).unwrap();
    symlink("missing", root.join("dangling")).unwrap();
    let metadata = std::fs::metadata(root.join("link")).unwrap();
    let link = std::fs::symlink_metadata(root.join("link")).unwrap();
    assert!(metadata.is_file());
    assert_eq!(metadata.len(), 7);
    assert!(metadata.dev() > 0);
    assert!(metadata.ino() > 0);
    assert_ne!(metadata.ino(), link.ino());
    assert!(metadata.mtime() > 0);
    assert!(metadata.ctime() > 0);
    unsafe extern "C" { fn stat(path: *const u8, buffer: *mut u64) -> i32; }
    let mut native_path = root.join("file").as_os_str().as_encoded_bytes().to_vec();
    native_path.push(0);
    let mut native = [0u64; 32];
    // SAFETY: the path is terminated; this aligned buffer exceeds Darwin's
    // inode64 stat record. The native call provides an independent ABI oracle.
    assert_eq!(unsafe { stat(native_path.as_ptr(), native.as_mut_ptr()) }, 0);
    assert_eq!(metadata.dev(), native[0] as u32 as u64);
    assert_eq!(metadata.ino(), native[1]);
    assert_eq!(metadata.mtime(), native[6] as i64);
    assert_eq!(metadata.mtime_nsec(), native[7] as i64);
    assert_eq!(metadata.ctime(), native[8] as i64);
    assert_eq!(metadata.ctime_nsec(), native[9] as i64);
    assert!((0..1_000_000_000).contains(&metadata.mtime_nsec()));
    assert!((0..1_000_000_000).contains(&metadata.ctime_nsec()));
    assert!(link.file_type().is_symlink());
    assert!(std::fs::symlink_metadata(root.join("dangling")).unwrap().file_type().is_symlink());
    assert!(std::fs::metadata(root.join("dangling")).is_err());
    let mut entries: Vec<_> = std::fs::read_dir(&root).unwrap().map(|entry| entry.unwrap()).collect();
    entries.sort_by_key(|entry| entry.file_name());
    assert_eq!(entries.len(), 5);
    for entry in entries {
        let kind = entry.file_type().unwrap();
        assert_eq!(entry.metadata().unwrap().file_type(), kind);
        assert_eq!(entry.path(), root.join(entry.file_name()));
        println!("{:?} {} {} {}", entry.file_name().into_vec(), kind.is_file(), kind.is_dir(), kind.is_symlink());
    }
    assert!(std::fs::read_dir(root.join("file")).is_err());
    assert_eq!(std::fs::read_dir(root.join("missing")).unwrap_err().kind(), std::io::ErrorKind::NotFound);
    let exe = std::env::current_exe().unwrap();
    assert!(exe.is_absolute());
    assert!(exe.is_file());
    assert_eq!(std::env::consts::DLL_EXTENSION, "dylib");
    let start = Instant::now();
    std::thread::sleep(Duration::from_millis(1));
    assert!(start.elapsed() >= Duration::from_millis(1));
    // Removing a symlink to another directory must preserve its destination.
    let outside = root.with_extension("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("keep"), b"safe").unwrap();
    symlink(&outside, root.join("external")).unwrap();
    std::fs::remove_dir_all(&root).unwrap();
    assert_eq!(std::fs::read(outside.join("keep")).unwrap(), b"safe");
    std::fs::remove_dir_all(&outside).unwrap();
    println!("directory runtime complete");
}

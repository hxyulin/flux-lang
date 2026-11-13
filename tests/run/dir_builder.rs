use std::fs::{self, DirBuilder};
use std::io::ErrorKind;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt, symlink};
unsafe extern "C" { fn umask(mode: u32) -> u32; }
struct Mask(u32);
impl Drop for Mask {
    fn drop(&mut self) {
        // SAFETY: this single-threaded test restores its process's previous mask.
        unsafe { umask(self.0); }
    }
}
fn main() {
    // SAFETY: this test owns its process and creates no other threads.
    let _mask = Mask(unsafe { umask(0o027) });
    let root = std::env::temp_dir().join(format!("flux-dir-builder-{}", std::process::id()));
    let plain = DirBuilder::new();
    assert_eq!(plain.create(root.join("missing/leaf")).unwrap_err().kind(), ErrorKind::NotFound);
    plain.create(&root).unwrap();
    assert_eq!(fs::metadata(&root).unwrap().permissions().mode() & 0o777, 0o750);
    assert_eq!(plain.create(&root).unwrap_err().kind(), ErrorKind::AlreadyExists);
    let mut builder = DirBuilder::new();
    builder.recursive(true).mode(0o711);
    builder.create(root.join("a/b/c")).unwrap();
    for path in [root.join("a"), root.join("a/b"), root.join("a/b/c")] {
        assert_eq!(fs::metadata(path).unwrap().permissions().mode() & 0o777, 0o710);
    }
    builder.create(root.join("a/b/c")).unwrap();
    builder.create(&root).unwrap();
    assert_eq!(fs::metadata(&root).unwrap().permissions().mode() & 0o777, 0o750);
    fs::write(root.join("file"), b"x").unwrap();
    assert_eq!(builder.create(root.join("file")).unwrap_err().kind(), ErrorKind::AlreadyExists);
    assert!(builder.create(root.join("file/child")).is_err());
    symlink("a", root.join("link")).unwrap();
    builder.create(root.join("link/b/new")).unwrap();
    assert!(root.join("a/b/new").is_dir());
    builder.create("").unwrap();
    builder.create("/").unwrap();
    assert!(plain.create("").is_err());
    assert!(builder.create("bad\0path").is_err());
    builder.recursive(false);
    assert_eq!(builder.create(&root).unwrap_err().kind(), ErrorKind::AlreadyExists);
    fs::remove_dir_all(root).unwrap();
    println!("directory builder complete");
}

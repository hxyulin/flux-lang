use std::fs;
use std::os::unix::ffi::OsStringExt;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::Path;

fn main() {
    let root = std::env::temp_dir().join(format!("flux-remove-tree-{}", std::process::id()));
    fs::create_dir_all(root.join("tree/deep/nested")).unwrap();
    fs::create_dir_all(root.join("outside/keep")).unwrap();
    fs::write(root.join("outside/keep/file"), b"outside").unwrap();
    fs::write(root.join("tree/deep/nested/file"), b"inside").unwrap();
    let raw = std::ffi::OsString::from_vec(b"raw\xff".to_vec());
    fs::write(root.join("tree").join(&raw), b"raw").unwrap();
    fs::set_permissions(root.join("tree").join(&raw), fs::Permissions::from_mode(0)).unwrap();
    symlink("../outside", root.join("tree/link")).unwrap();
    symlink("missing", root.join("tree/broken")).unwrap();
    symlink("outside", root.join("root-link")).unwrap();
    fs::remove_dir_all(root.join("root-link")).unwrap();
    assert!(root.join("outside/keep/file").exists());
    assert!(fs::symlink_metadata(root.join("root-link")).is_err());
    fs::remove_dir_all(root.join("tree")).unwrap();
    assert!(!root.join("tree").exists());
    assert_eq!(
        fs::read(root.join("outside/keep/file")).unwrap(),
        b"outside"
    );
    fs::write(root.join("plain"), b"file").unwrap();
    println!(
        "file {:?}",
        fs::remove_dir_all(root.join("plain"))
            .unwrap_err()
            .raw_os_error()
    );
    println!(
        "missing {:?}",
        fs::remove_dir_all(root.join("missing"))
            .unwrap_err()
            .raw_os_error()
    );
    let nul = root.join(std::ffi::OsString::from_vec(b"nul\0path".to_vec()));
    assert!(fs::remove_dir_all(nul).is_err());
    #[cfg(target_os = "linux")]
    let descriptors = fs::read_dir("/proc/self/fd").unwrap().count();
    for i in 0..100 {
        let path = root.join(format!("empty-{i}"));
        fs::create_dir_all(&path).unwrap();
        fs::remove_dir_all(&path).unwrap();
    }
    #[cfg(target_os = "linux")]
    assert_eq!(fs::read_dir("/proc/self/fd").unwrap().count(), descriptors);
    // Move a real directory aside and replace its old name with a symlink.
    // Removing the old name must leave both the moved tree and target intact.
    fs::create_dir_all(root.join("old/child")).unwrap();
    fs::rename(root.join("old"), root.join("moved")).unwrap();
    symlink("outside", root.join("old")).unwrap();
    fs::remove_dir_all(root.join("old")).unwrap();
    assert!(root.join("moved/child").is_dir());
    assert!(root.join("outside/keep/file").exists());
    let racing = root.join("racing");
    fs::create_dir_all(&racing).unwrap();
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let worker_stop = stop.clone();
    let child = racing.join("child");
    let worker = std::thread::spawn(move || {
        while !worker_stop.load(std::sync::atomic::Ordering::Relaxed) {
            let _ = fs::remove_dir_all(&child);
            let _ = symlink("../outside", &child);
            let _ = fs::remove_file(&child);
            let _ = fs::create_dir_all(&child);
        }
    });
    for _ in 0..64 {
        let _ = fs::remove_dir_all(&racing);
        let _ = fs::create_dir_all(&racing);
    }
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    worker.join().unwrap();
    assert_eq!(
        fs::read(root.join("outside/keep/file")).unwrap(),
        b"outside"
    );
    fs::remove_dir_all(&root).unwrap();
    assert!(!Path::new(&root).exists());
    println!("recursive removal kept symlink targets");
}

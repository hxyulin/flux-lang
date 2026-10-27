use std::fs::{self, File, OpenOptions, Permissions};
use std::io::{ErrorKind, Read, Seek, SeekFrom, Write};
use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::fs::PermissionsExt;
unsafe extern "C" {
    fn fcntl(fd: i32, command: i32, ...) -> i32;
}

fn main() {
    let directory = match std::env::var("FLUX_FS_SURFACE") {
        Ok(path) => std::path::PathBuf::from(path),
        Err(_) => std::env::current_dir()
            .unwrap()
            .join(format!("out/filesystem-surface-{}", std::process::id())),
    };
    let nested = directory.join("a/b/c");
    fs::create_dir_all(&nested).unwrap();
    fs::create_dir_all(&nested).unwrap();
    fs::create_dir_all("").unwrap();
    let path = nested.join("aé-output");
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap();
    assert_eq!(
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap_err()
            .kind(),
        ErrorKind::AlreadyExists
    );
    assert_eq!(fs::read(&path).unwrap().len(), 0);
    // SAFETY: the file owns this live descriptor; F_GETFD inspects its flags.
    assert_eq!(unsafe { fcntl(file.as_raw_fd(), 1) } & 1, 1);

    file.write_all(b"0123456789").unwrap();
    assert_eq!(file.stream_position().unwrap(), 10);
    assert_eq!(file.metadata().unwrap().len(), 10);
    file.seek(SeekFrom::Start(3)).unwrap();
    file.write_all(b"ABC").unwrap();
    file.flush().unwrap();
    file.seek(SeekFrom::End(-2)).unwrap();
    let mut tail = [0u8; 2];
    file.read_exact(&mut tail).unwrap();
    assert_eq!(&tail, b"89");
    let mut duplicate = file.try_clone().unwrap();
    // SAFETY: the clone owns a separate live close-on-exec descriptor.
    assert_eq!(unsafe { fcntl(duplicate.as_raw_fd(), 1) } & 1, 1);

    duplicate.rewind().unwrap();
    let mut first = [0u8; 2];
    duplicate.read_exact(&mut first).unwrap();
    assert_eq!(&first, b"01");
    assert_eq!(file.stream_position().unwrap(), 2);
    drop(duplicate);
    file.set_len(13).unwrap();
    file.rewind().unwrap();
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).unwrap();
    assert_eq!(&bytes, b"012ABC6789\0\0\0");
    file.set_permissions(Permissions::from_mode(0o640)).unwrap();
    assert_eq!(file.metadata().unwrap().permissions().mode() & 0o777, 0o640);
    let mut permissions = file.metadata().unwrap().permissions();
    permissions.set_readonly(true);
    assert!(permissions.readonly());
    file.set_permissions(permissions).unwrap();
    fs::set_permissions(&path, Permissions::from_mode(0o600)).unwrap();
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(
        file.seek(SeekFrom::Current(-99)).unwrap_err().kind(),
        ErrorKind::InvalidInput
    );
    assert_eq!(
        file.seek(SeekFrom::Start(u64::MAX)).unwrap_err().kind(),
        ErrorKind::InvalidInput
    );
    file.sync_all().unwrap();
    let raw = file.into_raw_fd();
    let file = unsafe { File::from_raw_fd(raw) };
    assert_eq!(file.as_raw_fd(), raw);
    drop(file);
    let mut append = OpenOptions::new().append(true).open(&path).unwrap();
    append.seek(SeekFrom::Start(0)).unwrap();
    append.write_all(b"+").unwrap();
    drop(append);
    assert_eq!(fs::metadata(&path).unwrap().len(), 14);
    let replacement = nested.join("replacement");
    fs::write(&replacement, b"new bytes").unwrap();
    fs::rename(&replacement, &path).unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"new bytes");
    assert!(!replacement.is_file());
    let snapshot = File::open(&path).unwrap();
    fs::remove_file(&path).unwrap();
    let mut old = String::new();
    (&snapshot).read_to_string(&mut old).unwrap();
    assert_eq!(old, "new bytes");
    let odd = std::ffi::OsString::from_vec(vec![b'n', 255, b'x']);
    let odd_path = nested.join(&odd);
    let odd_created = match fs::write(&odd_path, b"binary path") {
        Ok(()) => {
            assert_eq!(fs::read(&odd_path).unwrap(), b"binary path");
            true
        }
        Err(error) => {
            println!("odd path {:?} {:?}", error.kind(), error.raw_os_error());
            false
        }
    };
    assert_eq!(odd.as_bytes(), [b'n', 255, b'x']);
    assert_eq!(
        std::ffi::OsStr::from_bytes(odd.as_bytes()).as_bytes(),
        odd.as_bytes()
    );
    if odd_created {
        fs::remove_file(odd_path).unwrap();
    }
    let barrier = nested.join("not-directory");
    fs::write(&barrier, b"x").unwrap();
    assert!(fs::create_dir_all(barrier.join("child")).is_err());
    fs::remove_file(barrier).unwrap();
    assert_eq!(
        OpenOptions::new()
            .open(directory.join("invalid"))
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidInput
    );
    assert_eq!(
        OpenOptions::new()
            .read(true)
            .create(true)
            .open(directory.join("invalid"))
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidInput
    );
    assert_eq!(
        OpenOptions::new()
            .append(true)
            .truncate(true)
            .open(directory.join("invalid"))
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidInput
    );
    assert_eq!(
        File::open("bad\0path").unwrap_err().kind(),
        ErrorKind::InvalidInput
    );
    let winner = nested.join("one-winner");
    let wins = std::sync::atomic::AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..8 {
            let path = &winner;
            let count = &wins;
            scope.spawn(
                move || match OpenOptions::new().write(true).create_new(true).open(path) {
                    Ok(mut file) => {
                        file.write_all(b"winner").unwrap();
                        count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                    Err(error) => assert_eq!(error.kind(), ErrorKind::AlreadyExists),
                },
            );
        }
    });
    assert_eq!(wins.load(std::sync::atomic::Ordering::Relaxed), 1);
    assert_eq!(fs::read(&winner).unwrap(), b"winner");
    fs::remove_file(winner).unwrap();
    let stress = nested.join("descriptor-stress");
    for _ in 0..256 {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&stress)
            .unwrap();
        file.write_all(b"abc").unwrap();
        let mut cloned = file.try_clone().unwrap();
        drop(file);
        cloned.rewind().unwrap();
        let mut data = [0u8; 3];
        cloned.read_exact(&mut data).unwrap();
        assert_eq!(&data, b"abc");
    }
    fs::remove_file(stress).unwrap();
    println!(
        "filesystem output {} {} {}",
        bytes.len(),
        old,
        wins.load(std::sync::atomic::Ordering::Relaxed)
    );
}

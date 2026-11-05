// Advisory whole-file locks between two handles of one file.
use std::fs::{File, TryLockError};

fn describe(result: Result<(), TryLockError>) -> String {
    match result {
        Ok(()) => "locked".into(),
        Err(error) => format!("{error} / {error:?}"),
    }
}

fn main() {
    let path = std::env::temp_dir().join(format!("flux-file-locks-{}", std::process::id()));
    let first = File::create(&path).unwrap();
    let second = File::open(&path).unwrap();
    first.lock().unwrap();
    println!("exclusive held: {}", describe(second.try_lock()));
    println!("shared blocked: {}", describe(second.try_lock_shared()));
    let error = std::io::Error::from(second.try_lock().unwrap_err());
    println!("{:?} {}", error.kind(), error);
    first.unlock().unwrap();
    println!("after unlock: {}", describe(second.try_lock()));
    second.unlock().unwrap();
    first.lock_shared().unwrap();
    println!("shared with shared: {}", describe(second.try_lock_shared()));
    second.unlock().unwrap();
    println!("exclusive with shared: {}", describe(second.try_lock()));
    drop(first);
    println!("after close: {}", describe(second.try_lock()));
    second.unlock().unwrap();
    drop(second);
    std::fs::remove_file(&path).unwrap();
}

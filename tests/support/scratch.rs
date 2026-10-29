//! Per-test scratch directories: removed when the test passes, kept for
//! inspection when it panics.
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct Scratch {
    path: PathBuf,
}

impl Scratch {
    /// A new, empty directory under `parent`. Its name joins the process id
    /// with the creation time and a counter, so a reused process id never
    /// finds an earlier run's files.
    pub fn new(parent: impl AsRef<Path>) -> Scratch {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let parent = parent.as_ref();
        std::fs::create_dir_all(parent)
            .unwrap_or_else(|error| panic!("cannot create {}: {error}", parent.display()));
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |time| time.as_nanos());
        loop {
            let path = parent.join(format!(
                "{}-{nanos}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match std::fs::create_dir(&path) {
                Ok(()) => return Scratch { path },
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("cannot create {}: {error}", path.display()),
            }
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("kept scratch directory {}", self.path.display());
        } else {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

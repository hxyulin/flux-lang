use std::hash::{BuildHasherDefault, Hash, Hasher};
use std::path::{Path, PathBuf};

#[derive(Default)]
struct Tape(u64);
impl Hasher for Tape {
    fn write(&mut self, bytes: &[u8]) { for &byte in bytes { self.0 = self.0.wrapping_mul(31).wrapping_add(byte as u64); } }
    fn finish(&self) -> u64 { self.0 }
}
fn main() {
    let names = ["", ".", "./", "..", "../", "/", "//", "///a", "a", "a/", "a/.", "./a", "a/./b", "a//b/", "a/../b", "/a//b/.", "β/💡", "././a//."];
    for name in names {
        let path = Path::new(name);
        let mut hash = Tape::default(); path.hash(&mut hash);
        println!("path {path:?} parent {:?} absolute {} hash {}", path.parent(), path.is_absolute(), hash.finish());
        for base in names {
            let prefix = path.strip_prefix(base);
            println!("base {base:?} {:?} {:?} {:?}", prefix, path.cmp(Path::new(base)), path.join(base));
        }
    }
    let mut map = std::collections::HashMap::<PathBuf, usize, BuildHasherDefault<Tape>>::default();
    map.insert(PathBuf::from("a//b/./"), 7);
    assert_eq!(map.get(Path::new("a/b")), Some(&7));
    assert_eq!(map.insert(PathBuf::from("a/b"), 9), Some(7));
    assert_eq!(map.len(), 1);
    // SAFETY: on Unix arbitrary non-NUL bytes are valid OS-string data.
    let raw = unsafe { std::ffi::OsStr::from_encoded_bytes_unchecked(b"x\xff/\xce\xb2") };
    let path = Path::new(raw);
    let owned = path.to_path_buf();
    assert_eq!(owned.as_path(), path);
    assert!(path.to_str().is_none());
    println!("raw {:?} {} {:?}", path, path.display(), path.parent());
    if let Ok(root) = std::env::var("FLUX_PATH_FIXTURE") {
        let root = Path::new(&root);
        let long = "a".repeat(256);
        for name in ["text", "binary", "directory", "link", "loop", long.as_str(), "missing", "nul\0name"] {
            let path = root.join(name);
            println!("file {name:?} {}", path.is_file());
            match std::fs::read(&path) {
                Ok(bytes) => println!("bytes {bytes:?}"),
                Err(error) => println!("read {error} {error:?}"),
            }
            match std::fs::read_to_string(&path) {
                Ok(text) => println!("text {text:?}"),
                Err(error) => println!("utf8 {error} {error:?}"),
            }
            match std::fs::canonicalize(&path) {
                Ok(canonical) => println!("canonical {:?}", canonical.strip_prefix(root).unwrap()),
                Err(error) => println!("canonical {error} {error:?}"),
            }
        }
        for code in 0..150 { println!("errno {code} {:?}", std::io::Error::from_raw_os_error(code)); }
    }
}

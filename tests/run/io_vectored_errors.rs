use std::error::Error;
use std::io::{self, IoSlice, Write};
use std::sync::atomic::{AtomicUsize, Ordering};
static DROPS: AtomicUsize = AtomicUsize::new(0);
#[derive(Debug)]
struct Detail(u32);
impl std::fmt::Display for Detail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "detail {}", self.0)
    }
}
impl Error for Detail {
    fn description(&self) -> &str {
        "custom description"
    }
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&ROOT)
    }
}
impl Drop for Detail {
    fn drop(&mut self) {
        DROPS.fetch_add(1, Ordering::SeqCst);
    }
}
#[derive(Debug)]
struct Root;
static ROOT: Root = Root;
impl std::fmt::Display for Root {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("root")
    }
}
impl Error for Root {}
struct Short {
    bytes: Vec<u8>,
    calls: usize,
}
impl Write for Short {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.calls += 1;
        let n = bytes.len().min(2);
        self.bytes.extend_from_slice(&bytes[..n]);
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[repr(C)]
struct Iovec {
    base: *const u8,
    len: usize,
}
fn main() {
    let bytes = [1u8, 2, 3, 4];
    let original = IoSlice::new(&bytes);
    let mut slice = original;
    slice.advance(1);
    assert_eq!(&*slice, &[2, 3, 4]);
    assert_eq!(&*original, &bytes);
    // SAFETY: on Unix IoSlice is ABI-compatible with the native iovec record.
    let raw: Iovec = unsafe { std::mem::transmute(original) };
    assert_eq!(raw.base, bytes.as_ptr());
    assert_eq!(raw.len, bytes.len());
    let mut parts = [
        IoSlice::new(&[]),
        IoSlice::new(b"ab"),
        IoSlice::new(b""),
        IoSlice::new(b"cde"),
    ];
    let mut remaining = &mut parts[..];
    IoSlice::advance_slices(&mut remaining, 3);
    assert_eq!(remaining.len(), 1);
    assert_eq!(&*remaining[0], b"de");
    IoSlice::advance_slices(&mut remaining, 2);
    assert!(remaining.is_empty());
    let parts = [IoSlice::new(b""), IoSlice::new(b"ab"), IoSlice::new(b"cde")];
    let mut output = Vec::new();
    assert_eq!(output.write_vectored(&parts).unwrap(), 5);
    assert_eq!(output, b"abcde");
    let mut short = Short {
        bytes: Vec::new(),
        calls: 0,
    };
    assert_eq!(short.write_vectored(&parts).unwrap(), 2);
    assert_eq!(short.write_vectored(&[]).unwrap(), 0);
    assert_eq!(short.calls, 2);
    let mut output: Box<dyn Write> = Box::new(Vec::<u8>::new());
    assert_eq!(output.write_vectored(&parts).unwrap(), 5);
    let mut custom = io::Error::new(io::ErrorKind::InvalidData, Detail(7));
    assert_eq!(custom.raw_os_error(), None);
    assert_eq!(custom.kind(), io::ErrorKind::InvalidData);
    assert_eq!(custom.get_ref().unwrap().to_string(), "detail 7");
    assert_eq!(custom.get_mut().unwrap().to_string(), "detail 7");
    assert_eq!(custom.source().unwrap().to_string(), "root");
    println!("{} {:?} {:?}", custom, custom, custom.kind());
    let inner = custom.into_inner().unwrap();
    assert_eq!(DROPS.load(Ordering::SeqCst), 0);
    drop(inner);
    assert_eq!(DROPS.load(Ordering::SeqCst), 1);
    let message = io::Error::new(io::ErrorKind::Other, "message");
    println!("{} {:?}", message, message);
    let owned: Box<dyn Error> = String::from("owned").into();
    let borrowed: Box<dyn Error + Send + Sync> = "borrowed".into();
    println!("{:?} {} {:?} {}", owned, owned, borrowed, borrowed);
    let boxed = Box::new(Detail(9));
    assert_eq!(
        Error::description(&boxed),
        "description() is deprecated; use Display"
    );
    assert_eq!(Error::source(&boxed).unwrap().to_string(), "root");
    drop(boxed);
    assert_eq!(DROPS.load(Ordering::SeqCst), 2);
    println!(
        "{:?} {:?} {}",
        original,
        short.bytes,
        DROPS.load(Ordering::SeqCst)
    );
}

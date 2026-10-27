use std::io::{self, ErrorKind, Write};

struct Writer {
    bytes: Vec<u8>,
    interrupted: bool,
    zero: bool,
    fail_at: usize,
    calls: usize,
    flushes: usize,
}
impl Write for Writer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.calls += 1;
        if self.interrupted {
            self.interrupted = false;
            return Err(ErrorKind::Interrupted.into());
        }
        if self.zero {
            return Ok(0);
        }
        if self.bytes.len() >= self.fail_at {
            return Err(ErrorKind::PermissionDenied.into());
        }
        let count = bytes.len().min(2).min(self.fail_at - self.bytes.len());
        self.bytes.extend_from_slice(&bytes[..count]);
        Ok(count)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.flushes += 1;
        Ok(())
    }
}
fn writer() -> Writer {
    Writer {
        bytes: Vec::new(),
        interrupted: true,
        zero: false,
        fail_at: usize::MAX,
        calls: 0,
        flushes: 0,
    }
}
fn emit<W: Write>(mut out: W) -> io::Result<()> {
    out.write_all(b"object")?;
    out.flush()
}
fn main() {
    let mut out = writer();
    out.write_all(b"").unwrap();
    assert_eq!(out.calls, 0);
    out.write_all(b"abcde").unwrap();
    assert_eq!(out.bytes, b"abcde");
    assert_eq!(out.calls, 4);
    assert_eq!(out.flushes, 0);
    emit(out.by_ref()).unwrap();
    assert_eq!(out.bytes, b"abcdeobject");
    assert_eq!(out.flushes, 1);
    write!(&mut out, " {}:{:04} λ", -7, 12).unwrap();
    assert_eq!(out.bytes, "abcdeobject -7:0012 λ".as_bytes());

    let mut failing = writer();
    failing.fail_at = 3;
    let error = write!(&mut failing, "abcdef {}", 8).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::PermissionDenied);
    assert_eq!(failing.bytes, b"abc");
    assert_eq!(failing.flushes, 0);
    let mut zero = writer();
    zero.zero = true;
    let error = zero.write_all(b"x").unwrap_err();
    assert_eq!(error.kind(), ErrorKind::WriteZero);
    println!("{} {:?}", error, error);

    let mut bytes = Vec::new();
    emit(&mut &mut bytes).unwrap();
    assert_eq!(bytes.write(b"!").unwrap(), 1);
    write!(&mut bytes, " {}", 42).unwrap();
    assert_eq!(bytes, b"object! 42");
    let mut boxed = Box::new(Vec::new());
    emit(&mut boxed).unwrap();
    assert_eq!(*boxed, b"object");
    let mut dynamic: Box<dyn Write> = Box::new(writer());
    dynamic.write_all(b"hello").unwrap();
    write!(&mut dynamic, " {}", 27).unwrap();
    dynamic.flush().unwrap();

    let mut storage = [0; 5];
    let mut remaining = &mut storage[..];
    assert_eq!(remaining.write(b"ab").unwrap(), 2);
    remaining.write_all(b"cdef").unwrap_err();
    assert!(remaining.is_empty());
    remaining.write_all(b"").unwrap();
    remaining.flush().unwrap();
    assert_eq!(&storage, b"abcde");
    println!("write {:?} {:?} {}", out.bytes, failing.bytes, out.calls);
}

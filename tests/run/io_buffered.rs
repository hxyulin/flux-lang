use std::cell::Cell;
use std::io::{self, BufRead, BufReader, Cursor, Read, Write};
use std::os::fd::FromRawFd;
unsafe extern "C" {
    fn pipe(fds: *mut i32) -> i32;
}
use std::rc::Rc;

struct Chunked {
    bytes: Vec<u8>,
    position: usize,
    calls: Rc<Cell<usize>>,
    interrupt: bool,
    fail_once: Option<usize>,
}
impl Read for Chunked {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        self.calls.set(self.calls.get() + 1);
        if self.interrupt {
            self.interrupt = false;
            return Err(io::ErrorKind::Interrupted.into());
        }
        if self.fail_once == Some(self.position) {
            self.fail_once = None;
            return Err(io::ErrorKind::Other.into());
        }
        let count = out.len().min(3).min(self.bytes.len() - self.position);
        out[..count].copy_from_slice(&self.bytes[self.position..self.position + count]);
        self.position += count;
        Ok(count)
    }
}
fn chunked(bytes: &[u8]) -> Chunked {
    Chunked {
        bytes: bytes.to_vec(),
        position: 0,
        calls: Rc::new(Cell::new(0)),
        interrupt: true,
        fail_once: None,
    }
}
struct Delayed(Rc<Cell<bool>>);
impl Read for Delayed {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if !self.0.get() || output.is_empty() {
            return Ok(0);
        }
        self.0.set(false);
        output[0] = 42;
        Ok(1)
    }
}
fn main() {
    let mut reader = BufReader::with_capacity(4, chunked(b"abc|defgh|last"));
    assert_eq!(reader.capacity(), 4);
    let mut bytes = b"pre".to_vec();
    assert_eq!(reader.read_until(b'|', &mut bytes).unwrap(), 4);
    assert_eq!(bytes, b"preabc|");
    bytes.clear();
    assert_eq!(reader.read_until(b'|', &mut bytes).unwrap(), 6);
    assert_eq!(bytes, b"defgh|");
    bytes.clear();
    assert_eq!(reader.read_until(b'|', &mut bytes).unwrap(), 4);
    assert_eq!(bytes, b"last");
    assert_eq!(reader.read_until(b'|', &mut bytes).unwrap(), 0);

    let mut source = chunked(b"abcdef");
    source.fail_once = Some(3);
    let mut reader = BufReader::with_capacity(3, source);
    let mut partial = Vec::new();
    assert_eq!(
        reader.read_until(b'|', &mut partial).unwrap_err().kind(),
        io::ErrorKind::Other
    );
    assert_eq!(partial, b"abc");
    partial.clear();
    reader.read_until(b'|', &mut partial).unwrap();
    assert_eq!(partial, b"def");

    let mut reader = BufReader::with_capacity(8, Cursor::new(b"abc\nXYZ"));
    let mut limited = reader.by_ref().take(3);
    let mut first = Vec::new();
    assert_eq!(limited.read_until(b'\n', &mut first).unwrap(), 3);
    assert_eq!(first, b"abc");
    assert_eq!(limited.limit(), 0);
    assert!(limited.fill_buf().unwrap().is_empty());
    limited.set_limit(2);
    assert_eq!(limited.read_to_end(&mut first).unwrap(), 2);
    assert_eq!(first, b"abc\nX");
    let reader = limited.into_inner();
    assert_eq!(reader.fill_buf().unwrap(), b"YZ");
    reader.consume(1);
    assert_eq!(reader.fill_buf().unwrap(), b"Z");

    let zero_reader = chunked(b"not consumed");
    let calls = zero_reader.calls.clone();
    let mut zero = BufReader::new(zero_reader).take(0);
    assert_eq!(zero.read(&mut [0; 1]).unwrap(), 0);
    assert!(zero.fill_buf().unwrap().is_empty());
    assert_eq!(calls.get(), 0);

    let mut long = vec![b'x'; 25003];
    long[9000] = b'\n';
    long[17000] = 0;
    let mut reader = BufReader::new(Cursor::new(long.clone()));
    let mut all = Vec::new();
    reader.read_to_end(&mut all).unwrap();
    assert_eq!(all, long);
    let path = std::env::temp_dir().join(format!("flux-buffered-file-{}", std::process::id()));
    std::fs::write(&path, &long).unwrap();
    let mut file = BufReader::with_capacity(17, std::fs::File::open(&path).unwrap());
    let mut prefix = Vec::new();
    assert_eq!(file.read_until(b'\n', &mut prefix).unwrap(), 9001);
    assert_eq!(prefix, &long[..9001]);
    let mut bounded = file.by_ref().take(8000);
    let mut raw = Vec::new();
    assert_eq!(bounded.read_until(0, &mut raw).unwrap(), 8000);
    assert_eq!(raw.last(), Some(&0));
    let mut suffix = Vec::new();
    bounded.into_inner().read_to_end(&mut suffix).unwrap();
    assert_eq!(suffix, &long[17001..]);
    std::fs::remove_file(path).unwrap();
    let mut fds = [0; 2];
    unsafe {
        assert_eq!(pipe(fds.as_mut_ptr()), 0);
    }
    let read = unsafe { std::fs::File::from_raw_fd(fds[0]) };
    let mut write = unsafe { std::fs::File::from_raw_fd(fds[1]) };
    let send = long.clone();
    std::thread::scope(|scope| {
        scope.spawn(move || {
            write.write_all(&send).unwrap();
        });
        let mut pipe = BufReader::with_capacity(31, read);
        let mut received = Vec::new();
        pipe.read_to_end(&mut received).unwrap();
        assert_eq!(received, long);
    });
    let mut direct = BufReader::with_capacity(0, Cursor::new(b"zero capacity"));
    let mut copied = Vec::new();
    direct.read_to_end(&mut copied).unwrap();
    assert_eq!(copied, b"zero capacity");

    let mut bytes_reader = chunked(&[0, 255, 42]);
    bytes_reader.fail_once = Some(3);
    let mut values = bytes_reader.bytes();
    assert_eq!(values.next().unwrap().unwrap(), 0);
    assert_eq!(values.next().unwrap().unwrap(), 255);
    assert_eq!(values.next().unwrap().unwrap(), 42);
    assert_eq!(
        values.next().unwrap().unwrap_err().kind(),
        io::ErrorKind::Other
    );
    assert!(values.next().is_none());
    let ready = Rc::new(Cell::new(false));
    let mut delayed = Delayed(ready.clone()).bytes();
    assert!(delayed.next().is_none());
    ready.set(true);
    assert_eq!(delayed.next().unwrap().unwrap(), 42);

    let mut lines = BufReader::with_capacity(2, Cursor::new("é中\nlast".as_bytes()));
    let mut text = String::from("prefix:");
    assert_eq!(lines.read_line(&mut text).unwrap(), 6);
    assert_eq!(text, "prefix:é中\n");
    let mut invalid = Cursor::new(&[0xff, b'\n']);
    assert_eq!(
        invalid.read_line(&mut text).unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    assert_eq!(text, "prefix:é中\n");
    let mut slice = &b"a,b"[..];
    let mut out = Vec::new();
    slice.read_until(b',', &mut out).unwrap();
    assert_eq!(out, b"a,");
    assert_eq!(slice, b"b");
    let mut boxed: Box<dyn BufRead> = Box::new(BufReader::new(Cursor::new(b"boxed")));
    let mut output = Vec::new();
    boxed.read_until(b'!', &mut output).unwrap();
    assert_eq!(output, b"boxed");
    println!("buffered bytes {} limit {}", long.len(), first.len());
}

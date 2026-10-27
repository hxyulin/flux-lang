use std::io::{self, ErrorKind, Read, Seek, SeekFrom};

struct Stream {
    data: Vec<u8>,
    position: u64,
    interrupt: bool,
    fail: bool,
}
impl Read for Stream {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if out.is_empty() {
            return Ok(0);
        }
        if self.interrupt {
            self.interrupt = false;
            return Err(ErrorKind::Interrupted.into());
        }
        if self.fail && self.position >= 2 {
            return Err(ErrorKind::PermissionDenied.into());
        }
        if self.position >= self.data.len() as u64 {
            return Ok(0);
        }
        let count = out
            .len()
            .min(2)
            .min(self.data.len() - self.position as usize);
        out[..count]
            .copy_from_slice(&self.data[self.position as usize..self.position as usize + count]);
        self.position += count as u64;
        Ok(count)
    }
}
impl Seek for Stream {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        let (base, offset) = match from {
            SeekFrom::Start(position) => {
                self.position = position;
                return Ok(position);
            }
            SeekFrom::End(offset) => (self.data.len() as u64, offset),
            SeekFrom::Current(offset) => (self.position, offset),
        };
        let next = if offset >= 0 {
            base.checked_add(offset as u64)
        } else {
            base.checked_sub(offset.unsigned_abs())
        };
        self.position = next.ok_or(io::Error::from(ErrorKind::InvalidInput))?;
        Ok(self.position)
    }
}
fn exact<R: Read + Seek>(reader: &mut R) -> [u8; 3] {
    reader.seek(SeekFrom::Start(1)).unwrap();
    let mut bytes = [0; 3];
    reader.read_exact(&mut bytes).unwrap();
    bytes
}
fn main() {
    let mut stream = Stream {
        data: b"abcdef".to_vec(),
        position: 0,
        interrupt: true,
        fail: false,
    };
    let mut bytes = [0; 4];
    stream.read_exact(&mut []).unwrap();
    assert!(stream.interrupt);
    stream.read_exact(&mut bytes).unwrap();
    assert_eq!(&bytes, b"abcd");
    assert_eq!(stream.stream_position().unwrap(), 4);
    assert_eq!(exact(&mut &mut stream), *b"bcd");
    assert_eq!(stream.seek(SeekFrom::End(-1)).unwrap(), 5);
    let mut tail = vec![0];
    assert_eq!(stream.read_to_end(&mut tail).unwrap(), 1);
    assert_eq!(tail, vec![0, b'f']);
    assert_eq!(
        stream.read_exact(&mut [0]).unwrap_err().kind(),
        ErrorKind::UnexpectedEof
    );
    assert_eq!(
        stream.seek(SeekFrom::Current(-20)).unwrap_err().kind(),
        ErrorKind::InvalidInput
    );
    assert_eq!(stream.stream_position().unwrap(), 6);
    stream.seek(SeekFrom::Start(u64::MAX)).unwrap();
    assert_eq!(
        stream.seek(SeekFrom::Current(1)).unwrap_err().kind(),
        ErrorKind::InvalidInput
    );
    stream.rewind().unwrap();
    stream.fail = true;
    let mut partial = vec![99];
    assert_eq!(
        stream.read_to_end(&mut partial).unwrap_err().kind(),
        ErrorKind::PermissionDenied
    );
    assert_eq!(partial, vec![99, b'a', b'b']);

    let mut slice = &b"hello"[..];
    let mut text = String::from("prefix:");
    assert_eq!(slice.by_ref().read_to_string(&mut text).unwrap(), 5);
    assert_eq!(text, "prefix:hello");
    assert!(slice.is_empty());
    let mut invalid = &b"ok\xff"[..];
    assert_eq!(
        invalid.read_to_string(&mut text).unwrap_err().kind(),
        ErrorKind::InvalidData
    );
    assert_eq!(text, "prefix:hello");
    let mut empty = &b""[..];
    assert_eq!(empty.read(&mut bytes).unwrap(), 0);
    assert_eq!(
        io::Error::from_raw_os_error(4).kind(),
        ErrorKind::Interrupted
    );
    assert_eq!(io::Error::from_raw_os_error(2).kind(), ErrorKind::NotFound);
    for kind in [
        ErrorKind::Other,
        ErrorKind::NotFound,
        ErrorKind::WouldBlock,
        ErrorKind::UnexpectedEof,
        ErrorKind::Interrupted,
    ] {
        let error = io::Error::from(kind);
        assert_eq!(error.raw_os_error(), None);
        println!("{} {:?}", error, error);
    }
    let mut short = &b"a"[..];
    let error = short.read_exact(&mut [0; 2]).unwrap_err();
    println!("{} {:?}", error, error);
    let mut valid_partial = Stream {
        data: b"abcd".to_vec(),
        position: 0,
        interrupt: false,
        fail: true,
    };
    let mut destination = String::from("start:");
    assert_eq!(
        valid_partial
            .read_to_string(&mut destination)
            .unwrap_err()
            .kind(),
        ErrorKind::PermissionDenied
    );
    assert_eq!(destination, "start:ab");
    let mut invalid_partial = Stream {
        data: vec![255, 255, 2],
        position: 0,
        interrupt: false,
        fail: true,
    };
    assert_eq!(
        invalid_partial
            .read_to_string(&mut destination)
            .unwrap_err()
            .kind(),
        ErrorKind::PermissionDenied
    );
    assert_eq!(destination, "start:ab");
    println!("io {:?} {:?} {}", bytes, partial, text);
}

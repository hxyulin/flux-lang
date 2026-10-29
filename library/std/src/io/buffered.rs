//! Buffered input and bounded/byte-at-a-time reader adapters.
use super::{Error, ErrorKind, Read, Result};
use crate::alloc_crate::boxed::Box;
use crate::alloc_crate::string::String;
use crate::alloc_crate::vec::Vec;

/// A byte source that exposes its unread buffer without copying it.
pub trait BufRead: Read {
    fn fill_buf(&mut self) -> Result<&[u8]>;
    fn consume(&mut self, amount: usize);

    fn read_until(&mut self, delimiter: u8, output: &mut Vec<u8>) -> Result<usize> {
        let mut total = 0;
        loop {
            let buffer = match self.fill_buf() {
                Ok(buffer) => buffer,
                Err(error) => {
                    if error.kind() == ErrorKind::Interrupted {
                        continue;
                    }
                    return Err(error);
                }
            };
            let end = buffer.iter().position(|&byte| byte == delimiter);
            let count = end.map_or(buffer.len(), |index| index + 1);
            output.extend_from_slice(&buffer[..count]);
            self.consume(count);
            total += count;
            if end.is_some() || count == 0 {
                return Ok(total);
            }
        }
    }

    fn read_line(&mut self, output: &mut String) -> Result<usize> {
        let mut bytes = Vec::new();
        let result = self.read_until(b'\n', &mut bytes);
        match String::from_utf8(bytes) {
            Ok(text) => {
                output.push_str(&text);
                result
            }
            Err(_) => match result {
                Err(error) => Err(error),
                Ok(_) => Err(Error::invalid_utf8()),
            },
        }
    }
}

impl<B: BufRead + ?Sized> BufRead for &mut B {
    fn fill_buf(&mut self) -> Result<&[u8]> {
        (**self).fill_buf()
    }
    fn consume(&mut self, amount: usize) {
        (**self).consume(amount);
    }
    fn read_until(&mut self, delimiter: u8, output: &mut Vec<u8>) -> Result<usize> {
        (**self).read_until(delimiter, output)
    }
    fn read_line(&mut self, output: &mut String) -> Result<usize> {
        (**self).read_line(output)
    }
}
impl<R: Read + ?Sized> Read for Box<R> {
    fn read(&mut self, bytes: &mut [u8]) -> Result<usize> {
        (**self).read(bytes)
    }
    fn read_exact(&mut self, bytes: &mut [u8]) -> Result<()> {
        (**self).read_exact(bytes)
    }
    fn read_to_end(&mut self, bytes: &mut Vec<u8>) -> Result<usize> {
        (**self).read_to_end(bytes)
    }
    fn read_to_string(&mut self, text: &mut String) -> Result<usize> {
        (**self).read_to_string(text)
    }
}
impl<B: BufRead + ?Sized> BufRead for Box<B> {
    fn fill_buf(&mut self) -> Result<&[u8]> {
        (**self).fill_buf()
    }
    fn consume(&mut self, amount: usize) {
        (**self).consume(amount);
    }
    fn read_until(&mut self, delimiter: u8, output: &mut Vec<u8>) -> Result<usize> {
        (**self).read_until(delimiter, output)
    }
    fn read_line(&mut self, output: &mut String) -> Result<usize> {
        (**self).read_line(output)
    }
}
impl BufRead for &[u8] {
    fn fill_buf(&mut self) -> Result<&[u8]> {
        Ok(*self)
    }
    fn consume(&mut self, amount: usize) {
        *self = &self[amount..];
    }
}
impl<T: AsRef<[u8]>> BufRead for super::Cursor<T> {
    fn fill_buf(&mut self) -> Result<&[u8]> {
        let bytes = self.inner.as_ref();
        let start = core::cmp::min(self.pos, bytes.len() as u64) as usize;
        Ok(&bytes[start..])
    }
    fn consume(&mut self, amount: usize) {
        self.pos += amount as u64;
    }
}

/// A reader with reusable initialized storage for unread input.
pub struct BufReader<R: ?Sized> {
    buffer: Vec<u8>,
    position: usize,
    filled: usize,
    inner: R,
}
impl<R: core::fmt::Debug + ?Sized> core::fmt::Debug for BufReader<R> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("BufReader")
            .field("reader", &&self.inner)
            .field("buffer", &self.buffer())
            .finish()
    }
}
impl<R: Read> BufReader<R> {
    pub fn new(inner: R) -> BufReader<R> {
        Self::with_capacity(8192, inner)
    }
    pub fn with_capacity(capacity: usize, inner: R) -> BufReader<R> {
        let mut buffer = Vec::new();
        buffer.resize(capacity, 0);
        BufReader {
            buffer,
            position: 0,
            filled: 0,
            inner,
        }
    }
    pub fn into_inner(self) -> R {
        self.inner
    }
}
impl<R: ?Sized> BufReader<R> {
    pub fn get_ref(&self) -> &R {
        &self.inner
    }
    pub fn get_mut(&mut self) -> &mut R {
        &mut self.inner
    }
    pub fn buffer(&self) -> &[u8] {
        &self.buffer[self.position..self.filled]
    }
    pub fn capacity(&self) -> usize {
        self.buffer.len()
    }
}
impl<R: Read + ?Sized> Read for BufReader<R> {
    fn read(&mut self, output: &mut [u8]) -> Result<usize> {
        if self.position == self.filled && output.len() >= self.buffer.len() {
            self.position = 0;
            self.filled = 0;
            return self.inner.read(output);
        }
        let available = self.fill_buf()?;
        let count = core::cmp::min(available.len(), output.len());
        output[..count].copy_from_slice(&available[..count]);
        self.consume(count);
        Ok(count)
    }
}
impl<R: Read + ?Sized> BufRead for BufReader<R> {
    fn fill_buf(&mut self) -> Result<&[u8]> {
        if self.position == self.filled {
            // Invalidate consumed bytes before a fallible refill, so an error
            // cannot make old input visible again on the following call.
            self.position = 0;
            self.filled = 0;
            self.filled = self.inner.read(&mut self.buffer)?;
        }
        Ok(&self.buffer[self.position..self.filled])
    }
    fn consume(&mut self, amount: usize) {
        self.position = core::cmp::min(self.position.saturating_add(amount), self.filled);
    }
}

/// A reader that permits at most `limit` bytes to be consumed.
#[derive(Debug)]
pub struct Take<R> {
    pub(super) inner: R,
    pub(super) limit: u64,
}
impl<R> Take<R> {
    pub fn limit(&self) -> u64 {
        self.limit
    }
    pub fn set_limit(&mut self, limit: u64) {
        self.limit = limit;
    }
    pub fn get_ref(&self) -> &R {
        &self.inner
    }
    pub fn get_mut(&mut self) -> &mut R {
        &mut self.inner
    }
    pub fn into_inner(self) -> R {
        self.inner
    }
}
impl<R: Read> Read for Take<R> {
    fn read(&mut self, output: &mut [u8]) -> Result<usize> {
        if self.limit == 0 {
            return Ok(0);
        }
        let count = core::cmp::min(output.len() as u64, self.limit) as usize;
        let read = self.inner.read(&mut output[..count])?;
        self.limit -= read as u64;
        Ok(read)
    }
}
impl<R: BufRead> BufRead for Take<R> {
    fn fill_buf(&mut self) -> Result<&[u8]> {
        if self.limit == 0 {
            return Ok(&[]);
        }
        let bytes = self.inner.fill_buf()?;
        let count = core::cmp::min(bytes.len() as u64, self.limit) as usize;
        Ok(&bytes[..count])
    }
    fn consume(&mut self, amount: usize) {
        let count = core::cmp::min(amount as u64, self.limit) as usize;
        self.inner.consume(count);
        self.limit -= count as u64;
    }
}

/// An owning iterator that reads one byte per item and retries interruptions.
#[derive(Debug)]
pub struct Bytes<R> {
    pub(super) inner: R,
}
impl<R: Read> Iterator for Bytes<R> {
    type Item = Result<u8>;
    fn next(&mut self) -> Option<Result<u8>> {
        let mut byte = [0];
        loop {
            match self.inner.read(&mut byte) {
                Ok(0) => return None,
                Ok(_) => return Some(Ok(byte[0])),
                Err(error) => {
                    if error.kind() != ErrorKind::Interrupted {
                        return Some(Err(error));
                    }
                }
            }
        }
    }
}

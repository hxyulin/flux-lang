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

    /// Discards bytes through the next delimiter, returning how many were read.
    fn skip_until(&mut self, delimiter: u8) -> Result<usize> {
        let mut total = 0;
        loop {
            let (done, used) = {
                let buffer = match self.fill_buf() {
                    Ok(buffer) => buffer,
                    Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                    Err(error) => return Err(error),
                };
                match buffer.iter().position(|&byte| byte == delimiter) {
                    Some(index) => (true, index + 1),
                    None => (buffer.is_empty(), buffer.len()),
                }
            };
            self.consume(used);
            total += used;
            if done { return Ok(total); }
        }
    }

    /// An iterator over the input split at `delimiter`, without it.
    fn split(self, delimiter: u8) -> Split<Self> where Self: Sized { Split { buf: self, delim: delimiter } }
    /// An iterator over the input's lines, without `\n` or `\r\n`.
    fn lines(self) -> Lines<Self> where Self: Sized { Lines { buf: self } }
}

/// An iterator over the parts of a reader between delimiters.
#[derive(Debug)]
pub struct Split<B> { buf: B, delim: u8 }
impl<B: BufRead> Iterator for Split<B> {
    type Item = Result<Vec<u8>>;
    fn next(&mut self) -> Option<Result<Vec<u8>>> {
        let mut buf = Vec::new();
        match self.buf.read_until(self.delim, &mut buf) {
            Ok(0) => None,
            Ok(_n) => {
                if buf[buf.len() - 1] == self.delim { buf.pop(); }
                Some(Ok(buf))
            }
            Err(e) => Some(Err(e)),
        }
    }
}

/// An iterator over the lines of a reader.
#[derive(Debug)]
pub struct Lines<B> { buf: B }
impl<B: BufRead> Iterator for Lines<B> {
    type Item = Result<String>;
    fn next(&mut self) -> Option<Result<String>> {
        let mut buf = String::new();
        match self.buf.read_line(&mut buf) {
            Ok(0) => None,
            Ok(_n) => {
                if buf.ends_with('\n') {
                    buf.pop();
                    if buf.ends_with('\r') { buf.pop(); }
                }
                Some(Ok(buf))
            }
            Err(e) => Some(Err(e)),
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

use super::Write;

/// Buffers writes to an inner writer, writing the buffer out when it fills,
/// on flush, and when dropped.
pub struct BufWriter<W: ?Sized + Write> {
    buf: Vec<u8>,
    panicked: bool,
    inner: W,
}
impl<W: Write> BufWriter<W> {
    pub fn new(inner: W) -> BufWriter<W> { BufWriter::with_capacity(8192, inner) }
    pub fn with_capacity(capacity: usize, inner: W) -> BufWriter<W> {
        BufWriter { buf: Vec::with_capacity(capacity), panicked: false, inner }
    }
    /// Writes the buffer out and returns the writer, or the error with the
    /// unchanged BufWriter.
    pub fn into_inner(mut self) -> core::result::Result<W, IntoInnerError<BufWriter<W>>> {
        match self.flush_buf() {
            Err(e) => Err(IntoInnerError(self, e)),
            Ok(()) => Ok(self.into_parts().0),
        }
    }
    /// Returns the writer and the unwritten buffer, without writing it.
    pub fn into_parts(self) -> (W, core::result::Result<Vec<u8>, WriterPanicked>) {
        let mut this = core::mem::ManuallyDrop::new(self);
        let buf = core::mem::take(&mut this.buf);
        let buf = if !this.panicked { Ok(buf) } else { Err(WriterPanicked { buf }) };
        // SAFETY: the value is never used again, and its destructor does not run.
        let inner = unsafe { core::ptr::read(&this.inner) };
        (inner, buf)
    }
}
impl<W: ?Sized + Write> BufWriter<W> {
    fn flush_buf(&mut self) -> Result<()> {
        let mut written = 0;
        let len = self.buf.len();
        let mut ret = Ok(());
        while written < len {
            self.panicked = true;
            let r = self.inner.write(&self.buf[written..]);
            self.panicked = false;
            match r {
                Ok(0) => {
                    ret = Err(Error::write_zero("failed to write the buffered data"));
                    break;
                }
                Ok(n) => written += n,
                Err(ref e) if e.kind() == ErrorKind::Interrupted => {}
                Err(e) => { ret = Err(e); break; }
            }
        }
        if written > 0 { self.buf.drain(..written); }
        ret
    }
    pub fn get_ref(&self) -> &W { &self.inner }
    pub fn get_mut(&mut self) -> &mut W { &mut self.inner }
    pub fn buffer(&self) -> &[u8] { &self.buf }
    pub fn capacity(&self) -> usize { self.buf.capacity() }
}
impl<W: ?Sized + Write> Write for BufWriter<W> {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        if buf.len() > self.buf.capacity() - self.buf.len() { self.flush_buf()?; }
        if buf.len() >= self.buf.capacity() {
            self.panicked = true;
            let r = self.get_mut().write(buf);
            self.panicked = false;
            r
        } else {
            self.buf.extend_from_slice(buf);
            Ok(buf.len())
        }
    }
    fn write_all(&mut self, buf: &[u8]) -> Result<()> {
        if buf.len() > self.buf.capacity() - self.buf.len() { self.flush_buf()?; }
        if buf.len() >= self.buf.capacity() {
            self.panicked = true;
            let r = self.get_mut().write_all(buf);
            self.panicked = false;
            r
        } else {
            self.buf.extend_from_slice(buf);
            Ok(())
        }
    }
    fn flush(&mut self) -> Result<()> { self.flush_buf().and_then(|()| self.get_mut().flush()) }
}
impl<W: ?Sized + Write + super::Seek> super::Seek for BufWriter<W> {
    /// Writes the buffer out before seeking.
    fn seek(&mut self, pos: super::SeekFrom) -> Result<u64> {
        self.flush_buf()?;
        self.get_mut().seek(pos)
    }
}
impl<W: ?Sized + Write> Drop for BufWriter<W> {
    fn drop(&mut self) {
        if !self.panicked {
            // Errors are ignored, as std's destructor ignores them.
            let _r = self.flush_buf();
        }
    }
}
impl<W: ?Sized + Write + core::fmt::Debug> core::fmt::Debug for BufWriter<W> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("BufWriter")
            .field("writer", &&self.inner)
            .field("buffer", &format_args!("{}/{}", self.buf.len(), self.buf.capacity()))
            .finish()
    }
}

/// The buffered data of a BufWriter whose writer panicked during a write.
pub struct WriterPanicked { buf: Vec<u8> }
impl WriterPanicked {
    pub fn into_inner(self) -> Vec<u8> { self.buf }
}
impl core::fmt::Display for WriterPanicked {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("BufWriter inner writer panicked, what data remains unwritten is not known")
    }
}
impl core::fmt::Debug for WriterPanicked {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("WriterPanicked").field("buffer", &format_args!("{}/{}", self.buf.len(), self.buf.capacity())).finish()
    }
}
impl core::error::Error for WriterPanicked {}

/// A writer returned by into_inner together with the error that kept it.
#[derive(Debug)]
pub struct IntoInnerError<W>(W, Error);
impl<W> IntoInnerError<W> {
    pub fn error(&self) -> &Error { &self.1 }
    pub fn into_inner(self) -> W { self.0 }
    pub fn into_error(self) -> Error { self.1 }
    pub fn into_parts(self) -> (Error, W) { (self.1, self.0) }
}
impl<W> From<IntoInnerError<W>> for Error {
    fn from(iie: IntoInnerError<W>) -> Error { iie.1 }
}
impl<W> core::fmt::Display for IntoInnerError<W> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result { core::fmt::Display::fmt(&self.1, f) }
}
impl<W: Send + core::fmt::Debug> core::error::Error for IntoInnerError<W> {}

/// Buffers writes and writes them out at each newline.
pub struct LineWriter<W: ?Sized + Write> { inner: BufWriter<W> }
impl<W: Write> LineWriter<W> {
    pub fn new(inner: W) -> LineWriter<W> { LineWriter::with_capacity(1024, inner) }
    pub fn with_capacity(capacity: usize, inner: W) -> LineWriter<W> { LineWriter { inner: BufWriter::with_capacity(capacity, inner) } }
    pub fn into_inner(self) -> core::result::Result<W, IntoInnerError<LineWriter<W>>> {
        self.inner.into_inner().map_err(|err| {
            let IntoInnerError(inner, error) = err;
            IntoInnerError(LineWriter { inner }, error)
        })
    }
}
impl<W: ?Sized + Write> LineWriter<W> {
    pub fn get_ref(&self) -> &W { self.inner.get_ref() }
    pub fn get_mut(&mut self) -> &mut W { self.inner.get_mut() }
}
impl<W: ?Sized + Write> Write for LineWriter<W> {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        let newline = match buf.iter().rposition(|&byte| byte == b'\n') {
            Some(index) => index + 1,
            None => {
                // Complete lines buffered by an earlier call go out first.
                if self.inner.buffer().last() == Some(&b'\n') { self.inner.flush_buf()?; }
                return self.inner.write(buf);
            }
        };
        self.inner.flush_buf()?;
        let flushed = self.inner.get_mut().write(&buf[..newline])?;
        if flushed == 0 { return Ok(0); }
        if flushed < newline { return Ok(flushed); }
        let tail = &buf[newline..];
        let buffered = core::cmp::min(tail.len(), self.inner.capacity() - self.inner.buffer().len());
        self.inner.buf.extend_from_slice(&tail[..buffered]);
        Ok(flushed + buffered)
    }
    fn flush(&mut self) -> Result<()> { self.inner.flush() }
}
impl<W: ?Sized + Write + core::fmt::Debug> core::fmt::Debug for LineWriter<W> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("LineWriter")
            .field("writer", &self.get_ref())
            .field("buffer", &format_args!("{}/{}", self.inner.buffer().len(), self.inner.capacity()))
            .finish_non_exhaustive()
    }
}

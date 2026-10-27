use std::mem;
use std::ops::Range;
use std::sync::atomic::{AtomicUsize, Ordering};

type Result<T> = std::result::Result<T, ()>;
unsafe trait Pod: Copy + 'static {}
unsafe impl Pod for u8 {}
unsafe impl Pod for u32 {}
fn from_bytes<T: Pod>(bytes: &[u8]) -> Result<(&T, &[u8])> {
    let size = mem::size_of::<T>();
    if bytes.len() < size || bytes.as_ptr() as usize % mem::align_of::<T>() != 0 {
        return Err(());
    }
    Ok((unsafe { &*bytes.as_ptr().cast::<T>() }, &bytes[size..]))
}
fn slice_from_bytes<T: Pod>(bytes: &[u8], count: usize) -> Result<(&[T], &[u8])> {
    let size = mem::size_of::<T>().checked_mul(count).ok_or(())?;
    if bytes.len() < size || bytes.as_ptr() as usize % mem::align_of::<T>() != 0 {
        return Err(());
    }
    Ok((
        unsafe { std::slice::from_raw_parts(bytes.as_ptr().cast::<T>(), count) },
        &bytes[size..],
    ))
}

// The production ReadRef shape: a Copy value represents an independent data loan.
trait ReadRef<'data>: Clone + Copy {
    fn len(self) -> Result<u64>;
    fn read_bytes_at(self, offset: u64, size: u64) -> Result<&'data [u8]>;
    fn read_bytes_at_until(self, range: Range<u64>, delimiter: u8) -> Result<&'data [u8]>;
    fn read_bytes(self, offset: &mut u64, size: u64) -> Result<&'data [u8]> {
        let bytes = self.read_bytes_at(*offset, size)?;
        *offset = offset.wrapping_add(size);
        Ok(bytes)
    }
    fn read<T: Pod>(self, offset: &mut u64) -> Result<&'data T> {
        let size = mem::size_of::<T>() as u64;
        let bytes = self.read_bytes(offset, size)?;
        let (value, _) = from_bytes(bytes)?;
        Ok(value)
    }
    fn read_at<T: Pod>(self, mut offset: u64) -> Result<&'data T> {
        self.read(&mut offset)
    }
    fn read_slice<T: Pod>(self, offset: &mut u64, count: usize) -> Result<&'data [T]> {
        let size = count.checked_mul(mem::size_of::<T>()).ok_or(())? as u64;
        let bytes = self.read_bytes(offset, size)?;
        let (values, _) = slice_from_bytes(bytes, count)?;
        Ok(values)
    }
    fn read_slice_at<T: Pod>(self, mut offset: u64, count: usize) -> Result<&'data [T]> {
        self.read_slice(&mut offset, count)
    }
}
impl<'data> ReadRef<'data> for &'data [u8] {
    fn len(self) -> Result<u64> {
        Ok(self.len() as u64)
    }
    fn read_bytes_at(self, offset: u64, size: u64) -> Result<&'data [u8]> {
        self.get(offset as usize..)
            .ok_or(())?
            .get(..size as usize)
            .ok_or(())
    }
    fn read_bytes_at_until(self, range: Range<u64>, delimiter: u8) -> Result<&'data [u8]> {
        let bytes = self
            .get(range.start as usize..range.end as usize)
            .ok_or(())?;
        for (index, &byte) in bytes.iter().enumerate() {
            if byte == delimiter {
                return Ok(&bytes[..index]);
            }
        }
        Err(())
    }
}
#[derive(Clone, Copy)]
struct View<'data>(&'data [u8]);
impl<'data> ReadRef<'data> for View<'data> {
    fn len(self) -> Result<u64> {
        ReadRef::len(self.0)
    }
    fn read_bytes_at(self, offset: u64, size: u64) -> Result<&'data [u8]> {
        self.0.read_bytes_at(offset, size)
    }
    fn read_bytes_at_until(self, range: Range<u64>, delimiter: u8) -> Result<&'data [u8]> {
        self.0.read_bytes_at_until(range, delimiter)
    }
}
fn read_word<'data, R: ReadRef<'data>>(reader: &R) -> &'data u32 {
    let mut offset = 0;
    (*reader).read(&mut offset).unwrap()
}
trait Sealed {}
trait ObjectSymbol<'data>: Sealed {
    fn name(&self) -> &'data str;
    fn section(&self) -> Option<usize>;
    fn name_default(&self) -> &'data str {
        self.name()
    }
    fn section_index(&self) -> Option<usize> {
        self.section()
    }
}
static DROPS: AtomicUsize = AtomicUsize::new(0);
struct Symbol<'data> {
    name: &'data str,
    owned: String,
}
impl Sealed for Symbol<'_> {}
impl<'data> ObjectSymbol<'data> for Symbol<'data> {
    fn name(&self) -> &'data str {
        self.name
    }
    fn section(&self) -> Option<usize> {
        Some(self.owned.len())
    }
}
impl Drop for Symbol<'_> {
    fn drop(&mut self) {
        assert_eq!(self.owned, "owner");
        DROPS.fetch_add(1, Ordering::SeqCst);
    }
}
fn symbol_name<'data, S: ObjectSymbol<'data>>(symbol: &S) -> &'data str {
    symbol.name_default()
}
fn main() {
    let words = [0x04030201u32, 0x08070605, 0x0c0b0a09];
    let bytes = unsafe {
        std::slice::from_raw_parts(words.as_ptr().cast::<u8>(), mem::size_of_val(&words))
    };
    let word = {
        let reader = View(bytes);
        read_word(&reader)
    };
    assert_eq!(*word, words[0]);
    assert_eq!(ReadRef::len(bytes).unwrap(), 12);
    assert_eq!(bytes.read_at::<u8>(3).unwrap(), &4);
    assert_eq!(bytes.read_slice_at::<u32>(4, 2).unwrap(), &words[1..]);
    assert_eq!(bytes.read_bytes_at_until(0..12, 5).unwrap(), &[1, 2, 3, 4]);
    let mut offset = 4;
    let slice = bytes.read_slice::<u32>(&mut offset, 2).unwrap();
    assert_eq!(offset, 12);
    assert_eq!(slice, &words[1..]);
    assert!(bytes.read_at::<u32>(12).is_err());
    let text = String::from("symbol-data");
    let name = {
        let symbol = Symbol {
            name: &text,
            owned: String::from("owner"),
        };
        assert_eq!(symbol.section_index(), Some(5));
        symbol_name(&symbol)
    };
    assert_eq!(name, "symbol-data");
    assert_eq!(DROPS.load(Ordering::SeqCst), 1);
    println!(
        "defaults {} {} {} {}",
        *word,
        slice.len(),
        name,
        DROPS.load(Ordering::SeqCst)
    );
}

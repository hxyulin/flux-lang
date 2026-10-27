use std::io::{Cursor, ErrorKind, Read, Seek, SeekFrom, Write};

fn write_image<W: Write + Seek>(out: &mut W) {
    out.write_all(b"header").unwrap();
    out.seek(SeekFrom::Start(2)).unwrap();
    out.write_all(b"XY").unwrap();
    out.seek(SeekFrom::End(3)).unwrap();
    out.write_all(b"tail").unwrap();
}

fn main() {
    let mut image = Cursor::new(Vec::new());
    image.get_mut().reserve(128);
    write_image(&mut image);
    assert_eq!(image.position(), 13);
    assert_eq!(image.get_ref().as_slice(), b"heXYer\0\0\0tail");
    image.rewind().unwrap();
    let mut read = Vec::new();
    assert_eq!(image.read_to_end(&mut read).unwrap(), 13);
    assert_eq!(image.into_inner(), read);

    let mut c = Cursor::new(vec![1, 2, 3]);
    c.write_all(&[7]).unwrap();
    assert_eq!(c.get_ref(), &[7, 2, 3]);
    c.set_position(5);
    c.write_all(&[]).unwrap();
    assert_eq!(c.get_ref(), &[7, 2, 3, 0, 0]);
    assert_eq!(c.position(), 5);
    let before = c.position();
    assert_eq!(
        c.seek(SeekFrom::Current(-6)).unwrap_err().kind(),
        ErrorKind::InvalidInput
    );
    assert_eq!(c.position(), before);
    assert!(c.seek(SeekFrom::Current(i64::MIN)).is_err());
    c.set_position(u64::MAX);
    assert!(c.seek(SeekFrom::Current(1)).is_err());
    let mut one = [99];
    assert_eq!(c.read(&mut one).unwrap(), 0);
    assert_eq!(c.position(), u64::MAX);
    assert_eq!(one, [99]);

    let mut bytes = [0; 4];
    {
        let mut fixed = Cursor::new(&mut bytes[..]);
        fixed.seek(SeekFrom::End(-2)).unwrap();
        assert_eq!(fixed.write(b"abcd").unwrap(), 2);
        assert_eq!(fixed.write(b"z").unwrap(), 0);
        assert_eq!(
            fixed.write_all(b"z").unwrap_err().kind(),
            ErrorKind::WriteZero
        );
        fixed.set_position(u64::MAX);
        assert_eq!(fixed.write(b"z").unwrap(), 0);
        assert_eq!(fixed.position(), u64::MAX);
    }
    assert_eq!(bytes, [0, 0, b'a', b'b']);
    let mut array = Cursor::new([1, 2]);
    array.write_all(&[3, 4]).unwrap();
    assert_eq!(array.into_inner(), [3, 4]);
    let mut boxed = Cursor::new(vec![1, 2].into_boxed_slice());
    boxed.write_all(&[5]).unwrap();
    assert_eq!(&*boxed.into_inner(), &[5, 2]);
    let mut v = vec![1, 2];
    {
        let mut borrowed = Cursor::new(&mut v);
        borrowed.set_position(4);
        borrowed.write_all(b"z").unwrap();
        borrowed.set_position(1);
        assert_eq!(borrowed.read(&mut one).unwrap(), 1);
        assert_eq!(one, [2]);
    }
    assert_eq!(v, [1, 2, 0, 0, b'z']);
    let mut input = Cursor::new(b"abc".as_slice());
    input.seek(SeekFrom::End(-1)).unwrap();
    assert_eq!(input.read(&mut one).unwrap(), 1);
    assert_eq!(one, [b'c']);
    println!("cursor {} {} {}", read.len(), bytes[3], v.len());
}

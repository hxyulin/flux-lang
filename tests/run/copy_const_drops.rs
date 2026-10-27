use core::marker::PhantomData;
use std::cell::Cell;

trait Header {
    type Endian: Copy;
}
#[derive(Clone, Copy)]
struct File;
impl Header for File {
    type Endian = u32;
}
#[derive(Clone, Copy)]
struct Table<H: Header, R: Copy> {
    endian: H::Endian,
    marker: PhantomData<R>,
}
enum Any<R: Copy> {
    One((Table<File, R>, PhantomData<R>)),
}
const fn sum(value: (Table<File, u8>, Table<File, u8>)) -> u32 {
    let copied = value;
    copied.0.endian + value.1.endian
}
const VALUE: u32 = sum((
    Table {
        endian: 17,
        marker: PhantomData,
    },
    Table {
        endian: 25,
        marker: PhantomData,
    },
));
struct Owned<'a>(&'a Cell<u32>);
impl Drop for Owned<'_> {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}
fn main() {
    assert_eq!(VALUE, 42);
    let item = Any::One((
        Table::<File, u8> {
            endian: VALUE,
            marker: PhantomData,
        },
        PhantomData,
    ));
    let Any::One((table, _)) = item;
    assert_eq!(table.endian, 42);
    let drops = Cell::new(0);
    {
        let _items = [
            (Owned(&drops), PhantomData::<u8>),
            (Owned(&drops), PhantomData),
        ];
    }
    assert_eq!(drops.get(), 2);
    println!("copy const {} drops {}", table.endian, drops.get());
}

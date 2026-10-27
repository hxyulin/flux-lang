use std::cell::Cell;

trait Lend {
    type Item<'a>
    where
        Self: 'a;

    fn lend<'a>(&'a self) -> Self::Item<'a>;
}

trait LendMut {
    type Item<'a>
    where
        Self: 'a;

    fn lend_mut<'a>(&'a mut self) -> Self::Item<'a>;
}

struct Value(u32);

trait DynValue {
    type Hidden<T>
    where
        Self: Sized;

    fn number(&self) -> u32;
}

impl DynValue for Value {
    type Hidden<T> = T;

    fn number(&self) -> u32 {
        self.0
    }
}

impl Lend for Value {
    type Item<'a> = &'a u32;

    fn lend<'a>(&'a self) -> Self::Item<'a> {
        &self.0
    }
}

impl LendMut for Value {
    type Item<'a> = &'a mut u32;

    fn lend_mut<'a>(&'a mut self) -> Self::Item<'a> {
        &mut self.0
    }
}

fn shared<'a, T: Lend>(value: &'a T) -> T::Item<'a> {
    value.lend()
}

fn exclusive<'a, T: LendMut>(value: &'a mut T) -> T::Item<'a> {
    value.lend_mut()
}

trait View<'data> {
    fn bytes(&self) -> &'data [u8];
}

struct Part<'data, 'file> {
    data: &'data [u8],
    marker: &'file (),
    drops: &'data Cell<u32>,
}

impl<'data, 'file> View<'data> for Part<'data, 'file> {
    fn bytes(&self) -> &'data [u8] {
        self.data
    }
}

impl Drop for Part<'_, '_> {
    fn drop(&mut self) {
        let _ = *self.marker;
        self.drops.set(self.drops.get() + 1);
    }
}

struct Pieces<'data, 'file> {
    data: &'data [u8],
    marker: &'file (),
    drops: &'data Cell<u32>,
    next: usize,
}

impl<'data, 'file> Iterator for Pieces<'data, 'file> {
    type Item = Part<'data, 'file>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.next == self.data.len() {
            return None;
        }
        let index = self.next;
        self.next += 1;
        Some(Part {
            data: &self.data[index..index + 1],
            marker: self.marker,
            drops: self.drops,
        })
    }
}

trait Reader<'data> {
    type Section<'file>: View<'data>
    where
        Self: 'file,
        'data: 'file;

    type Sections<'file>: Iterator<Item = Self::Section<'file>>
    where
        Self: 'file,
        'data: 'file;

    type Table<'file>: SymbolSource<'data, Symbol = Self::Section<'file>>
    where
        Self: 'file,
        'data: 'file;

    fn section<'file>(&'file self) -> Self::Section<'file>
    where
        'data: 'file;

    fn sections<'file>(&'file self) -> Self::Sections<'file>
    where
        'data: 'file;

    fn table<'file>(&'file self) -> Self::Table<'file>
    where
        'data: 'file;
}

trait SymbolSource<'data> {
    type Symbol: View<'data>;
    fn symbol(&self) -> Self::Symbol;
}

struct Symbol<'data>(&'data [u8]);

struct SectionTable<'data, 'file> {
    data: &'data [u8],
    marker: &'file (),
    drops: &'data Cell<u32>,
}

impl<'data, 'file> SymbolSource<'data> for SectionTable<'data, 'file> {
    type Symbol = Part<'data, 'file>;

    fn symbol(&self) -> Self::Symbol {
        Part {
            data: self.data,
            marker: self.marker,
            drops: self.drops,
        }
    }
}

impl<'data> View<'data> for Symbol<'data> {
    fn bytes(&self) -> &'data [u8] {
        self.0
    }
}

struct Image<'data> {
    data: &'data [u8],
    marker: (),
    drops: &'data Cell<u32>,
}

impl<'data> Reader<'data> for Image<'data> {
    type Section<'file>
        = Part<'data, 'file>
    where
        Self: 'file,
        'data: 'file;

    type Sections<'file>
        = Pieces<'data, 'file>
    where
        Self: 'file,
        'data: 'file;

    type Table<'file>
        = SectionTable<'data, 'file>
    where
        Self: 'file,
        'data: 'file;

    fn section<'file>(&'file self) -> Self::Section<'file>
    where
        'data: 'file,
    {
        Part {
            data: self.data,
            marker: &self.marker,
            drops: self.drops,
        }
    }

    fn sections<'file>(&'file self) -> Self::Sections<'file>
    where
        'data: 'file,
    {
        Pieces {
            data: self.data,
            marker: &self.marker,
            drops: self.drops,
            next: 0,
        }
    }

    fn table<'file>(&'file self) -> Self::Table<'file>
    where
        'data: 'file,
    {
        SectionTable {
            data: self.data,
            marker: &self.marker,
            drops: self.drops,
        }
    }
}

impl<'data> SymbolSource<'data> for Image<'data> {
    type Symbol = Symbol<'data>;
    fn symbol(&self) -> Self::Symbol {
        Symbol(self.data)
    }
}

fn sum<'data, 'file, R: Reader<'data>>(reader: &'file R) -> u32
where
    'data: 'file,
{
    let mut result = 0;
    for section in reader.sections() {
        result += u32::from(section.bytes()[0]);
    }
    result
}

fn extract_data<'data, 'file, R: Reader<'data>>(reader: &'file R) -> &'data [u8]
where
    'data: 'file,
{
    reader.section().bytes()
}

fn symbol_data<'data, S: SymbolSource<'data>>(source: &S) -> &'data [u8] {
    source.symbol().bytes()
}

fn table_data<'data, 'file, R: Reader<'data>>(reader: &'file R) -> &'data [u8]
where
    'data: 'file,
{
    reader.table().symbol().bytes()
}

fn main() {
    let mut value = Value(11);
    assert_eq!(*shared(&value), 11);
    *exclusive(&mut value) += 6;
    assert_eq!(*shared(&value), 17);
    let dynamic: &dyn DynValue = &value;
    assert_eq!(dynamic.number(), 17);

    let data = [3u8, 1, 4, 1, 5];
    let drops = Cell::new(0);
    let (escaped, symbol, table) = {
        let image = Image {
            data: &data,
            marker: (),
            drops: &drops,
        };
        assert_eq!(sum(&image), 14);
        (
            extract_data(&image),
            symbol_data(&image),
            table_data(&image),
        )
    };
    assert_eq!(escaped, &data);
    assert_eq!(symbol, &data);
    assert_eq!(table, &data);
    assert_eq!(escaped.as_ptr(), data.as_ptr());
    assert_eq!(drops.get(), 7);
    println!("gat {} {} {}", value.0, escaped.len(), drops.get());
}

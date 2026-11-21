use std::marker::PhantomData;

struct Layout {
    size: usize,
}

struct Table {
    layout: &'static Layout,
    size: fn() -> usize,
}

struct Task<T>(PhantomData<T>);

impl<T> Task<T> {
    const LAYOUT: Layout = Layout {
        size: std::mem::size_of::<T>(),
    };

    fn size() -> usize {
        Self::LAYOUT.size
    }

    fn table() -> &'static Table {
        &Table {
            layout: &Self::LAYOUT,
            size: Self::size,
        }
    }
}

fn main() {
    for table in [Task::<u8>::table(), Task::<u64>::table()] {
        assert_eq!(table.layout.size, (table.size)());
        println!("{}", table.layout.size);
    }
}

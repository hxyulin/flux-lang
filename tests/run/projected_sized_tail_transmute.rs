trait Storage {
    type Tail;
}

impl Storage for u64 {
    type Tail = u64;
}

#[repr(C)]
struct Record<T: Storage> {
    first: u64,
    last: T::Tail,
}

fn array_view(value: &Record<u64>) -> &[u64; 2] {
    // SAFETY: this C-layout record holds two consecutive initialized u64s.
    unsafe { core::mem::transmute(value) }
}

fn main() {
    let record = Record::<u64> {
        first: 21,
        last: 42,
    };
    println!("{:?}", array_view(&record));
}

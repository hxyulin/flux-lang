use std::cell::Cell;

struct Storage {
    prefix: u8,
    cell: Cell<u32>,
    array: [u16; 3],
}
// SAFETY: this executable accesses STORAGE only from its single thread.
unsafe impl Sync for Storage {}
static STORAGE: Storage = Storage {
    prefix: 9,
    cell: Cell::new(17),
    array: [11, 22, 33],
};
const CELL: &Cell<u32> = &STORAGE.cell;
const SECOND: &u16 = &STORAGE.array[1];
const WHOLE: &[u16] = &STORAGE.array;
const fn forward(c: &Cell<u32>) -> &Cell<u32> {
    c
}
const FORWARDED: &Cell<u32> = forward(&STORAGE.cell);
static POINTERS: [&u16; 2] = [&STORAGE.array[0], &STORAGE.array[2]];
static NUMBERS: [u32; 3] = [4, 5, 6];
const SLICE: &[u32] = &NUMBERS;
const REBORROW: &[u32] = &*SLICE;
const ELEMENT: &u32 = &SLICE[1];
const REST: &[u32] = match &NUMBERS {
    [_, rest @ ..] => rest,
};
static REST_TABLE: [&[u32]; 2] = [
    REST,
    match REST {
        [first @ .., _] => first,
        _ => &[],
    },
];
static ENUM: Option<u8> = Some(31);
const MATCHED: u8 = match &ENUM {
    Some(x) => *x,
    None => 0,
};
const OWNED: Cell<u8> = Cell::new(23);
const TRANSIENT: u8 = {
    let _r = &Cell::new(1);
    42
};
const EMPTY: &[Cell<u8>; 0] = &[];
const NONE: &Option<Cell<u8>> = &None;
const VECTOR: &Vec<Cell<u8>> = &Vec::new();

struct SharedCells([Cell<u8>; 2]);
// SAFETY: only this executable's single thread accesses these cells.
unsafe impl Sync for SharedCells {}
static CELLS: SharedCells = SharedCells([Cell::new(8), Cell::new(9)]);
const CELL_SLICE: &[Cell<u8>] = &CELLS.0;
const CELL_LENGTH: usize = CELL_SLICE.len();
const CELL_SECOND: &Cell<u8> = &CELL_SLICE[1];
const CELL_REST: &[Cell<u8>] = match CELL_SLICE {
    [_, rest @ ..] => rest,
    _ => &[],
};
const CELL_REST_LENGTH: usize = CELL_REST.len();

fn main() {
    println!("{} {} {}", CELL_LENGTH, CELL_REST_LENGTH, CELL_SECOND.get());
    CELL_SECOND.set(10);
    println!("{} {}", CELLS.0[1].get(), CELL_REST[0].get());
    println!(
        "{} {} {} {}",
        REST.len(),
        REST[0],
        REST_TABLE[1].len(),
        REST_TABLE[1][0]
    );
    println!("{}", REST.as_ptr() == &NUMBERS[1] as *const u32);
    let owned = OWNED;
    println!("{} {} {}", STORAGE.prefix, CELL.get(), owned.get());
    println!(
        "{} {}",
        CELL as *const Cell<u32> == &STORAGE.cell as *const Cell<u32>,
        FORWARDED as *const Cell<u32> == CELL as *const Cell<u32>
    );
    CELL.set(1234);
    println!("{} {}", STORAGE.cell.get(), FORWARDED.get());
    FORWARDED.set(5678);
    println!("{} {}", STORAGE.cell.get(), CELL.get());
    println!("{} {} {} {}", SECOND, WHOLE.len(), WHOLE[2], *POINTERS[1]);
    println!(
        "{} {}",
        POINTERS[0] as *const u16 == &STORAGE.array[0] as *const u16,
        SECOND as *const u16 == &STORAGE.array[1] as *const u16
    );
    println!(
        "{} {} {}",
        SLICE.len(),
        SLICE[1],
        SLICE.as_ptr() == NUMBERS.as_ptr()
    );
    println!("{} {} {} {}", REBORROW.len(), REBORROW[2], ELEMENT, MATCHED);
    println!(
        "{} {} {} {}",
        TRANSIENT,
        EMPTY.len(),
        NONE.is_none(),
        VECTOR.len()
    );
}

struct Table {
    data: [u8; if Self::ENABLE { 2048 } else { 0 }],
}

impl Table {
    const ENABLE: bool = true;

    const fn new() -> Self {
        let mut data = [0u8; if Self::ENABLE { 2048 } else { 0 }];
        let mut i = 0;
        while i < data.len() && Self::ENABLE {
            data[i] = ((i * 17 + 3) % 251) as u8;
            i += 1;
        }
        Self { data }
    }
}

const fn count(values: &[u16]) -> usize {
    values.len()
}

const fn metadata_after_mutation() -> usize {
    let mut values = [1u16; 7];
    let pointer = (&raw const values) as *const [u16];
    values = [9; 7];
    pointer.len() + count(&values)
}

static TABLE: Table = Table::new();
const LENGTH: usize = metadata_after_mutation();
const METADATA_ONLY: usize = {
    let mut bytes = [2u8; 3];
    bytes[1] = 4;
    let pointer = (&raw const bytes) as *const [u8] as *const [bool];
    pointer.len()
};

fn main() {
    let mut sum = 0u64;
    for i in 0..TABLE.data.len() {
        assert_eq!(TABLE.data[i], ((i * 17 + 3) % 251) as u8);
        sum += u64::from(TABLE.data[i]);
    }
    assert_eq!(LENGTH, 14);
    assert_eq!(METADATA_ONLY, 3);
    println!("const table {} {sum} {LENGTH}", TABLE.data.len());
}

const NAME: (u8, u8) = (3, 8);
const PC: [(u8, u8); 2] = [(17, 1), (18, 6)];
const TABLE: &[(u8, bool, &[(u8, u8)])] = &[
    (17, true, &[NAME, PC[0], PC[1]]),
    (46, false, &[NAME, PC[1]]),
];
static STATIC_TABLE: &[(u8, bool, &[(u8, u8)])] = &[
    (17, true, &[NAME, PC[0], PC[1]]),
    (46, false, &[NAME, PC[1]]),
];

#[repr(C, align(128))]
struct Aligned {
    prefix: u16,
    pair: (u16, u16),
    array: [u16; 2],
}
const ROOT: Aligned = Aligned {
    prefix: 1,
    pair: (7, 513),
    array: [11, 1027],
};
const REFERENCES: &[&u16] = &[&ROOT.pair.1, &ROOT.array[1]];
const FLOATS: (f32, f64) = (-1.5, 2.25);
fn returned() -> &'static [(u8, u8)] {
    &[NAME, PC[0], PC[1]]
}
fn floating() -> &'static [f64] {
    &[FLOATS.1]
}
const fn direct() -> &'static [u16; 2] {
    &ROOT.array
}
static DIRECT: &[u16; 2] = direct();

fn main() {
    assert_eq!(TABLE, STATIC_TABLE);
    assert_eq!(returned(), TABLE[0].2);
    assert_eq!(*REFERENCES[0], 513);
    assert_eq!(*REFERENCES[1], 1027);
    // Preserve the root's alignment and each original projection offset.
    assert_eq!(REFERENCES[0] as *const u16 as usize % 128, 4);
    assert_eq!(REFERENCES[1] as *const u16 as usize % 128, 8);
    assert_eq!(floating()[0], 2.25);
    assert_eq!(direct().as_ptr() as usize % 128, 6);
    assert_eq!(DIRECT.as_ptr() as usize % 128, 6);
    assert_eq!(DIRECT[1], 1027);
    let mut sum = 0usize;
    for &(tag, children, attributes) in TABLE {
        sum += tag as usize + children as usize;
        for &(attribute, form) in attributes {
            sum += attribute as usize + form as usize;
        }
    }
    println!("projected const table {} {}", sum, REFERENCES.len());
}

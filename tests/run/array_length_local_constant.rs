const COUNT: usize = 9;

macro_rules! check {
    ($valid:expr) => {
        const _: [(); 0 - !{
            const VALID: bool = $valid;
            VALID
        } as usize] = [];
    };
}

check!(core::mem::size_of::<u32>() == 4);

const DATA: [u8; {
    const COUNT: usize = 2;
    COUNT
}] = [1, 2];

fn main() {
    assert_eq!(DATA.len(), 2);
    assert_eq!(COUNT, 9);
    println!("{}", DATA[1]);
}

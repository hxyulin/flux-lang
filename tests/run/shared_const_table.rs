const TABLE: [u8; 256] = [7; 256];
const OTHER: [u8; 256] = [9; 256];

#[inline(never)]
fn lookup(index: usize) -> u8 {
    TABLE[index]
}

#[inline(never)]
fn update(index: usize) -> u8 {
    let mut table = TABLE;
    table[index] = 13;
    table[index]
}

#[inline(never)]
fn borrow_update(index: usize) -> u8 {
    let mut table = TABLE;
    let value = &mut table[index];
    *value = 29;
    table[index]
}

#[inline(never)]
fn merged(index: usize, choose: bool) -> u8 {
    let table = if choose { TABLE } else { OTHER };
    table[index]
}

fn main() {
    let index = std::hint::black_box(199);
    println!(
        "{} {} {} {} {} {}",
        lookup(index),
        update(index),
        borrow_update(index),
        merged(index, true),
        merged(index, false),
        lookup(index),
    );
    #[cfg(panic = "unwind")]
    {
        std::panic::set_hook(Box::new(|_| {}));
        assert!(std::panic::catch_unwind(|| lookup(256)).is_err());
    }
}

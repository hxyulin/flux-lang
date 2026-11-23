#[derive(Copy, Clone)]
#[repr(i8)]
enum State {
    Low = -13,
    Middle = 7,
    High = 80,
}

const fn states() -> [State; 256] {
    let mut table = [State::Low; 256];
    table[13] = State::Middle;
    table[92] = State::High;
    table
}

const fn numbers() -> [u8; 256] {
    let mut table = [0; 256];
    let mut i = 0;
    while i < table.len() {
        table[i] = i as u8;
        i += 1;
    }
    table
}

const STATES: [State; 256] = states();
const NUMBERS: [u8; 256] = numbers();

fn main() {
    let index = std::hint::black_box(13);
    let mut changed = STATES;
    let original = STATES;
    changed[index] = State::Low;
    println!(
        "{} {} {} {}",
        std::hint::black_box(changed)[index] as i8,
        std::hint::black_box(original)[index] as i8,
        STATES[std::hint::black_box(92)] as i8,
        NUMBERS[index],
    );
}

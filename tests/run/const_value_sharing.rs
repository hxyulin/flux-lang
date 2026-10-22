//! Cached constants, repeated aggregates and call frames preserve value copies.
#[derive(Clone, Copy, Debug)]
enum Payload {
    Data([[i32; 3]; 2]),
    Empty,
}

const BASE: [[i32; 3]; 2] = [[1, 2, 3], [4, 5, 6]];
const BORROWED: &[[i32; 3]; 2] = &BASE;
const ENUM_BASE: Payload = Payload::Data(BASE);

const fn change(index: usize, value: i32) -> [[i32; 3]; 2] {
    let mut first = BASE;
    let second = first;
    first[1][index] = value;
    first[0] = second[1];
    first
}

const fn change_enum() -> Payload {
    match ENUM_BASE {
        Payload::Data(mut fields) => {
            fields[0][1] = 91;
            Payload::Data(fields)
        }
        Payload::Empty => Payload::Empty,
    }
}

const fn repeated() -> [[[i32; 3]; 2]; 3] {
    let mut arrays = [BASE; 3];
    arrays[1][0][2] = 81;
    arrays
}

const fn recurse(n: i32) -> i32 {
    if n == 0 {
        BASE[0][0]
    } else {
        recurse(n - 1) + change(2, n)[1][2]
    }
}

const FIRST: [[i32; 3]; 2] = change(0, 71);
const SECOND: [[i32; 3]; 2] = change(2, 72);
const CHANGED_ENUM: Payload = change_enum();
const REPEATED: [[[i32; 3]; 2]; 3] = repeated();
const CALLS: i32 = recurse(7) + recurse(3);

fn main() {
    println!("{:?} {:?}", FIRST, SECOND);
    println!("{:?} {:?}", BASE, BORROWED);
    println!("{:?} {:?}", ENUM_BASE, CHANGED_ENUM);
    println!("{:?} {}", REPEATED, CALLS);
    println!("{:?}", Payload::Empty);
    let mut runtime = BASE;
    let copy = runtime;
    runtime[1][1] = 99;
    println!("{:?} {:?} {:?}", runtime, copy, BORROWED);
}

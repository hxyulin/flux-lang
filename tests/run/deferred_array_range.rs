const LENGTH_CODES: usize = 286;
const DISTANCE_CODES: usize = 30;
const MAX_LENGTHS: usize = LENGTH_CODES + DISTANCE_CODES + 4;

fn fill(start: usize, len: usize) -> Option<u8> {
    let mut values: [u8; MAX_LENGTHS] = [0; MAX_LENGTHS];
    let part = values.get_mut(start..start + len)?;
    part.iter_mut().for_each(|byte| *byte = 42);
    Some(values[start])
}

fn main() {
    println!("{:?} {:?}", fill(10, 4), fill(MAX_LENGTHS - 1, 2));
}

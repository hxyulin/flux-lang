const fn next(x: usize) -> usize { x + 1 }
const BASE: usize = 2;
const HEIGHT: usize = next(BASE);
const WIDTH: usize = HEIGHT + 1;
struct Rows { data: [[u8; WIDTH]; HEIGHT] }
const TABLE: [[u8; WIDTH]; HEIGHT] = [[1; WIDTH]; HEIGHT];
const BYTES: usize = TABLE.len();
struct Padding([u8; BYTES]);
fn make(value: u8) -> Rows { Rows { data: [[value; WIDTH]; HEIGHT] } }
fn main() {
    let rows = make(7);
    let padding = Padding([3; BYTES]);
    println!("{} {} {} {} {}", rows.data.len(), rows.data[0].len(),
        rows.data[HEIGHT - 1][WIDTH - 1], padding.0.len(), padding.0[BYTES - 1]);
}

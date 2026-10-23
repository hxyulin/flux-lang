struct Owned(i32);
fn main() {
    let value = Owned(7);
    let moved = 'take: { break 'take value; };
    let _x = value.0;
}

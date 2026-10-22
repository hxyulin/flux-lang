// An array iterates by value: its items are elements, not references.

fn main() {
    let a = [1, 2, 3];
    for x in a {
        let y: &i32 = x;
    }
    let mut iter = [1u8, 2].into_iter();
    let z: Option<&u8> = iter.next();
}

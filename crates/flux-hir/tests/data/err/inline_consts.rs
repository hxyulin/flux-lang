// An inline constant (`const { .. }`) may not name the locals around it
// (E0435), but may its own.

fn main() {
    let x = 5;
    let a = const { x + 1 };
    let b = const {
        let y = 2;
        y * 3
    };
    let _ = (a, b);
}

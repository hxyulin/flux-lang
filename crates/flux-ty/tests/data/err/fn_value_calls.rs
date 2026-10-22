unsafe fn danger() -> i32 {
    1
}

fn main() {
    let g = danger;
    let p: unsafe fn() -> i32 = danger;
    let a = g() + p() + danger();
}

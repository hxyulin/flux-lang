const START: f32 = -1.25;
const ARR: [f64; 2] = [1.0, -0.0];
fn band(x: f32) -> i32 {
    match x {
        ..START => -2,
        START..0.0 => -1,
        -0.0..=1.25 => 1,
        1.25.. => 2,
        _ => 3,
    }
}
fn main() {
    assert!(matches!([1.0, 0.0], ARR));
    println!("{} {} {} {} {}",band(-2.0),band(-1.0),band(0.0),band(f32::INFINITY),band(f32::NAN));
}

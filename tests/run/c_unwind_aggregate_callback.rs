#[repr(C)]
struct Triple { integer: i64, float: f64, other: i64 }

struct Guard(i32);
impl Drop for Guard {
    fn drop(&mut self) { println!("drop {}", self.0); }
}

extern "C-unwind" fn transform(value: Triple, scale: f64) -> Triple {
    let _guard = Guard(value.integer as i32);
    Triple {
        integer: value.integer * 2,
        float: value.float * scale,
        other: value.other + 1,
    }
}

fn invoke(callback: extern "C-unwind" fn(Triple, f64) -> Triple) -> Triple {
    let _guard = Guard(17);
    callback(Triple { integer: 3, float: 1.25, other: 9 }, 2.5)
}

fn main() {
    let value = invoke(transform);
    println!("{} {} {}", value.integer, value.float, value.other);
    let value = transform(value, 2.0);
    println!("{} {} {}", value.integer, value.float, value.other);
}

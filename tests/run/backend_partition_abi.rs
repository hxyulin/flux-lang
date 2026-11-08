trait Value {
    fn value(&self) -> i64;
}

struct Pair(i64, i64);

impl Value for Pair {
    fn value(&self) -> i64 {
        self.0 + self.1
    }
}

fn aggregate(x: i64) -> (i64, i64, i64, i64, i64, i64, i64, i64) {
    (x, x + 1, x + 2, x + 3, x + 4, x + 5, x + 6, x + 7)
}

fn sum(x: i64) -> i64 {
    let (a, b, c, d, e, f, g, h) = aggregate(x);
    a + b + c + d + e + f + g + h
}

fn narrow(a: i8, b: u8) -> i64 {
    a as i64 + b as i64
}

fn generic<T: Value>(value: &T) -> i64 {
    value.value()
}

fn dynamic(value: &dyn Value) -> i64 {
    value.value()
}

static CALLBACK: fn(i64) -> i64 = sum;
static TEXT: &str = "partitioned";

struct Guard;

impl Drop for Guard {
    fn drop(&mut self) {
        println!("dropped");
    }
}

fn main() {
    let _guard = Guard;
    let pair = Pair(13, 29);
    println!("{} {} {} {} {}", TEXT, CALLBACK(10), narrow(-7, 250), generic(&pair), dynamic(&pair));
}

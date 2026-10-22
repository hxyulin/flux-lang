// Edition 2024's deny-by-default `static_mut_refs` lint.

struct Point {
    x: i32,
}

impl Point {
    fn get(&self) -> i32 {
        self.x
    }
    fn set(&mut self, x: i32) {
        self.x = x;
    }
}

static mut COUNT: i32 = 0;
static mut POS: Point = Point { x: 0 };

fn take(r: &i32) -> i32 {
    *r
}

fn main() {
    unsafe {
        let a = &COUNT;
        let b = &mut COUNT;
        let c = take(&COUNT);
        let d = &POS.x;
        POS.set(1);
        let e = POS.get();
        COUNT += 1;
        let f = COUNT;
    }
}

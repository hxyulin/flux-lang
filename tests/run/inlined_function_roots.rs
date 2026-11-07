#[inline(always)]
fn inline_only(x: i64) -> i64 {
    x * 3 + 1
}

#[inline(always)]
fn address_taken(x: i64) -> i64 {
    x * 3 + 1
}

static HANDLER: fn(i64) -> i64 = address_taken;

#[inline(never)]
fn noinline_root(x: i64) -> i64 {
    x - 7
}

trait Apply {
    fn apply(&self, x: i64) -> i64;
}
struct Adder(i64);
impl Apply for Adder {
    #[inline(always)]
    fn apply(&self, x: i64) -> i64 {
        x + self.0
    }
}

fn main() {
    let object: &dyn Apply = &Adder(3);
    println!(
        "{} {} {} {}",
        inline_only(5),
        HANDLER(7),
        noinline_root(12),
        object.apply(6)
    );
}

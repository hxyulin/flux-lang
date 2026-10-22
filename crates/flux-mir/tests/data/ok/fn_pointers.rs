// A function item is a value of no size, called directly; coerced to a
// function pointer, its address, called indirectly.

fn double(x: i32) -> i32 {
    x * 2
}

fn negate(x: i32) -> i32 {
    -x
}

fn id<T>(x: T) -> T {
    x
}

fn apply(f: fn(i32) -> i32, x: i32) -> i32 {
    f(x)
}

fn pick(flag: bool) -> fn(i32) -> i32 {
    if flag { double } else { negate }
}

fn main() {
    let d = double;
    let a = d(1);
    let b = apply(id::<i32>, a);
    let c = pick(true)(b);
}

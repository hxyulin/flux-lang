struct Context {
    value: *mut u32,
}

fn update(ctx: &Context) {
    let bump = || unsafe { *ctx.value += 1 };
    unsafe { *ctx.value *= 2 };
    bump();
}

fn main() {
    let mut value = 20;
    update(&Context { value: &raw mut value });
    println!("{value}");
}

fn f(a: i32) -> i32 {
    {
        let inner = 1;
    }
    inner + missing + a
}

fn g() {
    break;
    continue;
}

fn f() {}

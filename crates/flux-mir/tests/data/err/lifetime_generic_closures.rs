fn call_it<F: Fn() -> R, R>(f: F) -> R {
    f()
}

fn through_generic() {
    let r;
    {
        let x = 1;
        r = call_it(|| &x);
    }
    let _ = *r;
}

fn option_map() {
    let r;
    {
        let v = [1, 2];
        let o = Some(0usize);
        r = o.map(|i| &v[i]);
    }
    let _ = r;
}

fn main() {}

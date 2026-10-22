// What a `Fn(&T) -> &U` bound's call returns borrows what it is given
// (E0515, E0597), and a closure meeting it may not return what it
// borrows from its creator (E0597, as rustc's placeholder lifetimes
// make it need `'static`).

fn call<F: Fn(&u8) -> &u8>(f: F) -> u8 {
    let x = 5;
    *f(&x)
}

fn escapes<F: Fn(&u8) -> &u8>(f: F) -> &'static u8 {
    let x = 5;
    f(&x)
}

fn dangling<F: Fn(&u8) -> &u8>(f: F) -> u8 {
    let r;
    {
        let y = 7;
        r = f(&y);
    }
    *r
}



fn main() {
    let outside = 9u8;
    let a = call(|_| &outside);
    let b = call(|v| v);
    let _ = (a, b);
}

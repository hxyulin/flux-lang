// Moves, initialization and borrows through enums and patterns.

struct S;

struct D(S);

impl Drop for D {
    fn drop(&mut self) {}
}

enum E {
    A(S),
    B(S, S),
    C,
}

fn take(_: S) {}

fn moved_then_matched(e: E) {
    if let E::A(s) = e {
        take(s);
    }
    match e {
        E::C => {}
        _ => {}
    }
}

fn moved_then_borrowed(o: Option<S>) {
    if let Some(s) = o {
        take(s);
    }
    match o {
        None => {}
        Some(ref r) => {}
    }
}

fn both_bindings(e: E) {
    match e {
        E::B(a, b) => {
            take(a);
            take(b);
        }
        _ => {}
    }
    let E::B(x, _) = e else {
        return;
    };
}

fn out_of_reference(r: &E) {
    match *r {
        E::A(s) => {}
        _ => {}
    }
}

fn out_of_drop(d: D) {
    match d {
        D(s) => {}
    }
}

fn not_mutable(e: E) {
    match e {
        E::A(ref mut s) => {}
        _ => {}
    }
}

fn borrowed_then_moved(mut e: E) {
    let r = match &e {
        E::A(s) => s,
        _ => return,
    };
    let f = e;
    let _ = r;
}

fn guards(mut x: Option<i32>, mut o: Option<i32>, p: Option<S>) {
    match x {
        Some(_) if {
            x = None;
            false
        } =>
        {
            let _ = x;
        }
        _ => {}
    }
    match o {
        Some(ref mut n) if {
            *n += 1;
            true
        } => {}
        _ => {}
    }
    match p {
        Some(s) if take_bool(s) => {}
        _ => {}
    }
}

fn take_bool(_: S) -> bool {
    true
}

fn bindings(o: Option<S>, mut p: Option<S>) {
    match o {
        whole @ Some(inner) => {}
        _ => {}
    }
    loop {
        match p {
            Some(d) => take(d),
            None => break,
        }
    }
}

fn main() {}

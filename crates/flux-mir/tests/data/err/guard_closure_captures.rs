// Capturing a guard binding must not bypass the guard's shared access.
struct S(u8);
struct Pair(S, u8);

fn consume(_: S) -> bool {
    true
}

fn move_binding(s: S) {
    match s {
        b if (move || { let r = &b; r.0 == 7 })() => {},
        _ => {},
    }
}

fn move_noncopy_field(s: Pair) {
    match s {
        b if (|| consume(b.0))() => {},
        _ => {},
    }
}

fn mutate_copy_field(mut s: S) {
    match s {
        mut b if (|| { b.0 += 1; true })() => {},
        _ => {},
    }
}

fn borrow_binding(mut s: S) {
    match s {
        mut b if (|| { let r = &mut b; r.0 == 7 })() => {},
        _ => {},
    }
}

fn mutate_copy_binding(s: u8) {
    match s {
        mut b if (|| { b += 1; true })() => {},
        _ => {},
    }
}

fn move_ref_mut(mut s: S) {
    match s {
        ref mut b if (move || { let r = b; r.0 == 7 })() => {},
        _ => {},
    }
}

fn mutate_ref_mut(mut s: S) {
    match s {
        ref mut b if (|| { b.0 += 1; true })() => {},
        _ => {},
    }
}

fn main() {}

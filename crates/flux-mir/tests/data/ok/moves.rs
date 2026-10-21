struct Inner {
    x: i32,
}

struct S {
    inner: Inner,
    n: i32,
}

fn take(_s: S) {}

fn take_inner(_i: Inner) {}

fn partial_move_then_copy_field(s: S) -> i32 {
    let inner = s.inner;
    take_inner(inner);
    s.n
}

fn partial_move_by_pattern(s: S) -> i32 {
    let S { inner, .. } = s;
    take_inner(inner);
    s.n
}

fn reassign_moved_field(mut s: S) {
    take_inner(s.inner);
    s.inner = Inner { x: 1 };
    take(s);
}

fn let_underscore_after_move(s: S) {
    take(s);
    let _ = s.n;
    let _ = s;
}

fn move_in_branch_then_reassign(flag: bool, mut s: S) -> i32 {
    if flag {
        take(s);
        s = S { inner: Inner { x: 2 }, n: 3 };
    }
    s.inner.x
}

fn move_then_reassign_whole(mut s: S) -> i32 {
    take(s);
    s = S { inner: Inner { x: 4 }, n: 5 };
    s.n
}

fn copy_tuples(t: (i32, (bool, u8))) -> u8 {
    let a = t;
    let b = t;
    let c = a.1;
    let d = c;
    if c.0 && d.0 { a.1.1 + b.1.1 } else { t.1.1 }
}

fn functional_update_copy_only(s: S) -> i32 {
    let t = S { inner: Inner { x: 6 }, ..s };
    take(t);
    take(s);
    0
}

fn functional_update_moves_inner(s: S) -> i32 {
    let t = S { n: 7, ..s };
    take(t);
    s.n
}

fn deferred_init(flag: bool) -> i32 {
    let s: S;
    if flag {
        s = S { inner: Inner { x: 8 }, n: 9 };
    } else {
        s = S { inner: Inner { x: 10 }, n: 11 };
    }
    let t: (i32, i32);
    t = (s.n, s.inner.x);
    t.0 + t.1
}

fn moved_into_tuple_field(s: S) -> i32 {
    let t = (s, 1);
    let (inner_s, n) = t;
    take(inner_s);
    n + t.1
}

fn main() {
    let _ = partial_move_then_copy_field(S { inner: Inner { x: 1 }, n: 2 });
    let _ = partial_move_by_pattern(S { inner: Inner { x: 1 }, n: 2 });
    reassign_moved_field(S { inner: Inner { x: 1 }, n: 2 });
    let_underscore_after_move(S { inner: Inner { x: 1 }, n: 2 });
    let _ = move_in_branch_then_reassign(true, S { inner: Inner { x: 1 }, n: 2 });
    let _ = move_then_reassign_whole(S { inner: Inner { x: 1 }, n: 2 });
    let _ = copy_tuples((1, (true, 2)));
    let _ = functional_update_copy_only(S { inner: Inner { x: 1 }, n: 2 });
    let _ = functional_update_moves_inner(S { inner: Inner { x: 1 }, n: 2 });
    let _ = deferred_init(false);
    let _ = moved_into_tuple_field(S { inner: Inner { x: 1 }, n: 2 });
}

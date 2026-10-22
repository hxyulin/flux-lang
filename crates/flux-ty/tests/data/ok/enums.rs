// The types of enums, their variants and the bindings of patterns: a
// pattern that sees through a reference binds by reference.

enum E {
    A,
    B(i32, bool),
    C { x: i64, y: (u8, bool) },
}

enum Tree<T> {
    Leaf(T),
    Node(T, T),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Small {
    One = 1,
    Two,
}

fn by_value(e: E) -> i64 {
    match e {
        E::A => 0,
        E::B(n, flag) => {
            let _: bool = flag;
            n as i64
        }
        E::C { x, y: (b, _) } => x + b as i64,
    }
}

fn by_reference(e: &E, m: &mut E) -> i64 {
    let first = match e {
        E::B(n, _) => *n as i64,
        E::C { x, .. } => *x,
        E::A => 0,
    };
    if let E::B(n, flag) = m {
        *n += 1;
        *flag = !*flag;
    }
    first
}

fn explicit(mut e: E) -> i32 {
    match e {
        E::B(ref n, _) if *n > 0 => *n,
        E::B(ref mut n, _) => {
            *n = 1;
            *n
        }
        whole @ E::A => {
            let _: E = whole;
            0
        }
        _ => 2,
    }
}

fn generic<T: Copy>(t: Tree<T>) -> T {
    match t {
        Tree::Leaf(v) | Tree::Node(v, _) => v,
    }
}

fn inferred() -> Option<u8> {
    let mut found = None;
    let values = (Some(3), None::<u8>);
    if let (Some(v), _) = values {
        found = Some(v);
    }
    found
}

fn results(r: Result<i32, bool>) -> i32 {
    let Ok(v) = r else {
        return -1;
    };
    v
}

fn main() {
    let _ = by_value(E::C { x: 1, y: (2, true) });
    let mut m = E::B(1, false);
    let _ = by_reference(&E::A, &mut m);
    let _ = explicit(E::A);
    let _ = generic(Tree::Node(1u16, 2));
    let _ = inferred();
    let _ = results(Ok(1));
    let _ = Small::Two as u8;
}

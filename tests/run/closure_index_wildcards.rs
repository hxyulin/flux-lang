//! Closures matching places with patterns that may not read them. A
//! place matched only by wildcards (`let _ = x`, `match x { _ => .. }`,
//! `(_, _)`, `[_, _]` of an array, `[..]` of a slice) is not captured,
//! but an element of an array or slice is reached by indexing, which
//! borrows the array whether the element is read or not, after which
//! the index is evaluated (and checked: the last case panics); `ref mut`
//! borrows it mutably. With and without `move`, seen in drop order.

struct D(i32);

impl Drop for D {
    fn drop(&mut self) {
        println!("drop {}", self.0);
    }
}

fn index(i: usize) -> usize {
    i
}

fn indexed(p: [u8; 1]) -> u8 {
    let c = || {
        match p[0] {
            _ => {}
        }
        1
    };
    c()
}

fn field(p: (u8, u8)) -> u8 {
    let c = || {
        match p.1 {
            _ => {}
        }
        2
    };
    c()
}

fn let_wildcard_indexed(p: [D; 2], i: usize) -> i32 {
    let c = || {
        let _ = p[i];
        let _ = p[index(i)].0;
        3
    };
    c()
}

fn if_let_wildcard(p: [i32; 3]) -> i32 {
    let c = || if let _ = p[2] { 4 } else { 0 };
    c()
}

fn nested_indices(p: [[D; 2]; 2], i: usize, j: usize) -> i32 {
    let c = || {
        match p[i][j] {
            _ => 5,
        }
    };
    c()
}

// A wildcard does not read a whole binding: it stays with its owner and
// the closure moves nothing.
fn wildcard_moves_nothing(d: D) {
    let c = move || {
        let _ = d;
        6
    };
    println!("wildcard {}", c());
    drop(c);
    println!("closure dropped, d still here");
}

fn wildcard_field(p: (D, D)) {
    let c = move || {
        let _ = p.0;
        match p.1 {
            _ => 7,
        }
    };
    println!("wildcard field {}", c());
    drop(c);
    println!("closure dropped");
}

// With `move`, the array is moved even if only matched by wildcards.
fn move_indexed(p: [D; 2]) {
    let c = move || {
        match p[1] {
            _ => 8,
        }
    };
    println!("move indexed {}", c());
    drop(c);
    println!("closure dropped");
}

fn tuple_wildcards(t: (D, D)) {
    let c = move || match t {
        (_, _) => 9,
    };
    println!("tuple wildcards {}", c());
    drop(c);
    println!("closure dropped");
}

fn array_wildcards(a: [D; 2]) {
    let c = move || match a {
        [_, _] => 10,
    };
    println!("array wildcards {}", c());
    drop(c);
    println!("closure dropped");
}

// `[..]` matches a slice of any length, without reading it.
fn slice_rest(s: &[D]) -> i32 {
    let c = || match *s {
        [..] => 11,
    };
    c()
}

fn slice_len(s: &[D]) -> i32 {
    let c = || match *s {
        [_, ..] => 12,
        [] => 0,
    };
    c()
}

// `ref mut` borrows the array mutably.
fn ref_mut_element(mut p: [i32; 2]) -> [i32; 2] {
    let mut c = || match p[1] {
        ref mut x => *x += 13,
    };
    c();
    c();
    p
}

fn let_ref_mut_element(mut p: [(i32, i32); 2]) -> [(i32, i32); 2] {
    let mut c = || {
        let (ref mut a, _) = p[0];
        *a = 14;
    };
    c();
    p
}

fn binding_in_arm(p: [i32; 3]) -> i32 {
    let c = || match p[1] {
        0 => 0,
        n => n,
    };
    c()
}

// The index is evaluated, and checked, even if the element is not read.
fn out_of_bounds(p: [u8; 2]) {
    let i = index(5);
    let c = || {
        let _ = p[i];
    };
    c();
}

fn main() {
    println!("{} {}", indexed([7]), field((1, 2)));
    println!("{}", let_wildcard_indexed([D(1), D(2)], 1));
    println!("{}", if_let_wildcard([1, 2, 3]));
    println!("{}", nested_indices([[D(3), D(4)], [D(5), D(6)]], 1, 0));
    let d = D(7);
    wildcard_moves_nothing(d);
    wildcard_field((D(8), D(9)));
    move_indexed([D(10), D(11)]);
    tuple_wildcards((D(12), D(13)));
    array_wildcards([D(14), D(15)]);
    let s = [D(16), D(17)];
    println!("{} {}", slice_rest(&s), slice_len(&s));
    println!("{:?}", ref_mut_element([1, 2]));
    println!("{:?}", let_ref_mut_element([(1, 2), (3, 4)]));
    println!("{}", binding_in_arm([1, 2, 3]));
    out_of_bounds([1, 2]);
}

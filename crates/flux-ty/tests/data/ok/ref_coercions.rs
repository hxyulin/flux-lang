// References to references where a reference is expected are
// dereferenced (rustc's deref coercion), and references are pointers
// to a C variadic function.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

struct Figure {
    sides: i32,
}

fn sides(f: &Figure) -> i32 {
    f.sides
}

fn bump(f: &mut Figure) {
    f.sides += 1;
}

fn id<T>(t: &T) -> &T {
    t
}

struct Holder<'a> {
    f: &'a Figure,
}

fn pick<'a>(r: &'a &'a Figure) -> &'a Figure {
    r
}

fn first(f: &Figure) -> &i32 {
    &f.sides
}

fn main() {
    let a = Figure { sides: 3 };
    let b = Figure { sides: 4 };
    let figures = [&a, &b];
    let mut t = 0;
    for f in &figures {
        t += sides(f);
    }
    let r = &a;
    let rr = &r;
    let x: &Figure = &rr;
    let mut c = Figure { sides: 5 };
    {
        let mut m = &mut c;
        let mm = &mut m;
        bump(mm);
        let s: &Figure = mm;
        t += s.sides;
    }
    let mut d = Figure { sides: 6 };
    let dm = &mut d;
    let h = Holder { f: &dm };
    let q = &&mut c;
    let by_closure = |f: &Figure| f.sides * 10;
    let captured = || sides(&rr);
    let y: &Figure = id(&&b);
    let bytes = [104u8, 105, 0];
    let mut more = [33u8, 0];
    unsafe {
        printf(c"%s%s\n".as_ptr(), &bytes, &mut more);
        printf(c"%d\n".as_ptr(), *first(&&a));
    }
    let _ = (t, x.sides, h.f.sides, by_closure(q), captured(), pick(&&b), y);
}

//! `Drop`: bindings are dropped at the end of their block in reverse
//! order, parameters after the function's own bindings, temporaries at
//! the end of their statement (a block's tail before its bindings, as in
//! edition 2024) unless a `let` borrows them; a value is dropped before
//! its fields, which drop in order; values moved out are not dropped,
//! values moved out on some paths only are dropped where they were not,
//! and a struct with some fields moved out drops the others; assigning
//! drops the old value; `break`, `continue` and `return` drop what they
//! leave, including what earlier arguments of a call made; generic code
//! drops what its type parameters turn out to be.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn say(text: i32) {
    unsafe {
        printf(c"%d ".as_ptr(), text);
    }
}

fn line() {
    unsafe {
        printf(c"\n".as_ptr());
    }
}

struct Noisy(i32);

impl Drop for Noisy {
    fn drop(&mut self) {
        say(self.0);
    }
}

impl Noisy {
    fn id(&self) -> i32 {
        self.0
    }

    fn tenfold(&self) -> Noisy {
        Noisy(self.0 * 10)
    }

    fn plus(self, other: Noisy) -> i32 {
        self.0 + other.0
    }

    fn positive(&self) -> bool {
        self.0 > 0
    }
}

// Its own `drop` first, then its fields', in order.
struct Outer {
    first: Noisy,
    second: Noisy,
    tag: i32,
}

impl Drop for Outer {
    fn drop(&mut self) {
        say(self.tag);
        self.first.0 += 1;
    }
}

// No `Drop` of its own: just its fields.
struct Pair(Noisy, Noisy);

struct Wrap<T>(T);

impl<T> Drop for Wrap<T> {
    fn drop(&mut self) {
        say(-1);
    }
}

fn make(n: i32) -> Noisy {
    Noisy(n)
}

fn pair(a: i32, b: i32) -> Pair {
    Pair(Noisy(a), Noisy(b))
}

fn take(n: Noisy) -> i32 {
    n.0
}

fn params(_a: Noisy, _b: Noisy) {
    let _c = Noisy(3);
}

fn consume<T>(_t: T) {}

fn pass<T>(t: T) -> T {
    t
}

fn tail() -> i32 {
    let _kept = Noisy(2);
    make(1).id()
}

fn early(stop: bool) -> i32 {
    let _a = Noisy(1);
    if stop {
        let _b = Noisy(2);
        return 0;
    }
    let _c = Noisy(3);
    4
}

fn both(_a: Noisy, _b: Noisy) {}

// An argument made before another one returns is dropped by the return.
fn diverging(stop: bool) -> i32 {
    let _x = Noisy(0);
    both(make(1), if stop { return 5 } else { make(2) });
    let _t = (make(3), loop {
        if !stop {
            break make(4);
        }
    });
    9
}

// A receiver taken by value is the call's first argument.
fn receiver(stop: bool) -> i32 {
    make(6).plus(if stop { return 7 } else { make(8) })
}

fn maybe(cond: bool) {
    let n = Noisy(5);
    if cond {
        take(n);
    }
    say(0);
}

fn main() {
    {
        let _a = Noisy(1);
        let _b = Noisy(2);
        let _ = make(3);
        make(4);
        say(make(5).id());
    }
    line();
    params(Noisy(1), Noisy(2));
    line();
    say(tail());
    line();
    {
        let r = &make(7);
        let s = make(8).id();
        say(r.0 + s);
    }
    line();
    {
        let mut x = Noisy(1);
        say(x.0 - 1);
        x = Noisy(2);
        say(x.0);
    }
    line();
    {
        let o = Outer {
            first: Noisy(1),
            second: Noisy(2),
            tag: 100,
        };
        say(o.second.id());
    }
    line();
    {
        let p = pair(1, 2);
        let a = p.0;
        say(a.0);
        let q = pair(3, 4).1;
        say(q.0);
    }
    line();
    maybe(true);
    line();
    maybe(false);
    line();
    {
        let mut i = 0;
        loop {
            let _n = Noisy(i);
            i += 1;
            if i == 2 {
                continue;
            }
            if i == 4 {
                break;
            }
            say(10 * i);
        }
    }
    line();
    say(early(true));
    line();
    say(early(false));
    line();
    say(diverging(true));
    line();
    say(diverging(false));
    line();
    say(receiver(true));
    line();
    say(receiver(false));
    line();
    {
        // A temporary is dropped after those made while computing it,
        // and an operand of `&&` drops its own before the next runs.
        let v = make(7).tenfold().0;
        say(v);
        let both = make(1).positive() && make(2).positive();
        say(both as i32);
        // Borrows through `*` and fields extend too.
        let r = &*&make(3);
        let f = &(&make(4)).0;
        say(r.0 + *f);
    }
    line();
    {
        consume(Noisy(1));
        consume(Wrap(Noisy(2)));
        consume((Noisy(3), 4, Noisy(5)));
        let w = pass(Wrap(Pair(Noisy(6), Noisy(7))));
        say(0);
        drop(w);
        say(0);
    }
    line();
    {
        let mut t = (Noisy(1), Noisy(2));
        t.0 = Noisy(3);
        let moved = t.1;
        t.1 = Noisy(4);
        say(moved.0);
    }
    line();
}

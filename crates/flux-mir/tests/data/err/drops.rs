// Moving a field out of a struct with a `Drop` impl (E0509), since its
// `drop` will see it, whether by field, by pattern, or by struct update
// syntax (once per field moved); `Copy` fields may be read; the move is
// not made, so using the value later is no further error. Drops in
// constants and `const fn`s that may run code (E0493), of bindings,
// parameters, type parameters, which may have a destructor, and values
// partly moved out, which still have the rest; one per place the value
// is dropped, assignment included. And a block's temporary dropped while
// a reference to it is still to be used (E0716).

struct Noisy(i32);

impl Drop for Noisy {
    fn drop(&mut self) {}
}

struct Owner {
    inner: Noisy,
    count: i32,
}

impl Drop for Owner {
    fn drop(&mut self) {}
}

struct Wrap<T>(T);

impl<T> Drop for Wrap<T> {
    fn drop(&mut self) {}
}

struct Plain {
    owner: Owner,
}

const K: i32 = {
    let _n = Noisy(1);
    3
};

const fn generic<T>(_t: T) {}

const fn moved(n: Noisy) -> i32 {
    let kept = n;
    4
}

const fn passed(n: Noisy) -> Noisy {
    n
}

const fn partly(t: (Noisy, i32)) -> Noisy {
    t.0
}

const fn reassigned(mut n: Noisy, other: Noisy) -> Noisy {
    n = other;
    n
}

struct Two {
    a: Noisy,
    b: Noisy,
    count: i32,
}

impl Drop for Two {
    fn drop(&mut self) {}
}

fn spread(two: Two) {
    let moved = Two { count: 1, ..two };
    let again = two;
}

fn dropped_while_borrowed() -> i32 {
    let x = { &Noisy(10) }.0;
    x
}

fn main() {
    let o = Owner {
        inner: Noisy(2),
        count: 3,
    };
    let inner = o.inner;
    let o2 = Owner {
        inner: Noisy(2),
        count: 3,
    };
    let Owner { inner: bound, .. } = o2;
    let w = Wrap(Noisy(1));
    let Wrap(n) = w;
    let o3 = Owner {
        inner: Noisy(2),
        count: 3,
    };
    let count = o3.count;
    let o4 = Owner { count: 4, ..o3 };
    let p = Plain {
        owner: Owner {
            inner: Noisy(5),
            count: 6,
        },
    };
    let deep = p.owner.inner;
}

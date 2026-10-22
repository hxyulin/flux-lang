// Higher-ranked outputs must keep their input slot through substitution,
// opaque returns and recursive trait-impl obligations.
fn apply<T, F: Fn(T, &u8) -> &u8>(value: T, f: F) -> &'static u8 {
    f(value, &0)
}

fn captured<'a>(value: &'a u8) -> impl Fn(&u8) -> &u8 + 'a {
    move |_| value
}

trait Read {
    fn read(self) -> u8;
}
struct Reader<F> {
    f: F,
}
impl<F: Fn(&u8) -> &u8> Read for Reader<F> {
    fn read(self) -> u8 {
        *(self.f)(&0)
    }
}
fn read<R: Read>(reader: R) -> u8 {
    reader.read()
}

trait Value {}
impl Value for u8 {}
fn dyn_call<F: Fn(&dyn Value) -> &u8>(f: F) -> u8 {
    *f(&0u8)
}
fn apply_item<I: Iterator, F: Fn(I::Item, &u8) -> &u8>(mut iter: I, f: F) -> &'static u8 {
    f(iter.next().unwrap(), &0)
}

fn select<'a, T, F: Fn(&T) -> &u8>(value: &'a T, f: F) -> &'a u8 {
    f(value)
}
fn unrelated<'a, 'b>(value: &'a &'a u8, other: &'b u8) -> &'a u8 {
    select(value, |_| other)
}

fn main() {
    let local = 7;
    let escaped = apply(&local, |value, _| value);
    println!("{}", escaped);
    let reader = Reader { f: |_: &u8| &local };
    println!("{}", read(reader));
    println!("{}", dyn_call(|_| &local));
    println!("{}", apply_item([&local].into_iter(), |value, _| value));
}

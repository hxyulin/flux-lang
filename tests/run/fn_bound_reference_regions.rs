//! Reference output slots stay attached to the written input after
//! substituting types containing references, including opaque returns.
fn apply<T, F: Fn(T, &u8) -> &u8>(value: T, f: F) -> &'static u8 {
    f(value, &23)
}

fn make<T>() -> impl Fn(T, &u8) -> &u8 {
    |_, value| value
}

fn identity<T>() -> impl Fn(&T) -> &T {
    |value| value
}

fn wrap<F: Fn(&u8) -> &u8>(f: F) -> impl Fn(&u8) -> &u8 {
    f
}

fn constant() -> impl Fn(&u8) -> &u8 {
    |_| &31
}

fn once() -> impl FnOnce(&u8) -> &u8 {
    |value| value
}

trait Read {
    fn read(self) -> u8;
}
struct Reader<F> {
    f: F,
}
impl<F: Fn(&u8) -> &u8> Read for Reader<F> {
    fn read(self) -> u8 {
        *(self.f)(&41)
    }
}
fn read<R: Read>(reader: R) -> u8 {
    reader.read()
}

trait Value {
    fn value(&self) -> &u8;
}
impl Value for u8 {
    fn value(&self) -> &u8 { self }
}
fn dyn_call<F: Fn(&dyn Value) -> &u8>(f: F) -> u8 {
    *f(&43u8)
}
fn apply_item<I: Iterator, F: Fn(I::Item, &u8) -> &u8>(mut iter: I, f: F) -> &'static u8 {
    f(iter.next().unwrap(), &47)
}

fn reborrow<T, F: Fn(&T) -> &u8>(value: &T, f: F) -> u8 {
    *f(value)
}

fn main() {
    let local = 17;
    let a = apply(&local, |_, value| value);
    let b = apply(Some(&local), |_, value| value);
    let c = make::<&u8>()(&local, &local);
    let d = identity::<u8>()(&local);
    let e = wrap(|value| value)(&local);
    let f = constant()(&local);
    let g = once()(&local);
    let reader = Reader { f: wrap(|value| value) };
    let h = dyn_call(|value| value.value());
    let i = apply_item([&local].into_iter(), |_, value| value);
    let j = reborrow(&&local, |value| *value);
    let k = reborrow(&&local, |_| &local);
    println!("{a} {b} {c} {d} {e} {f} {g} {} {h} {i} {j} {k}", read(reader));
}

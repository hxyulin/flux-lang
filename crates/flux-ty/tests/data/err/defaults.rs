// `Default` that does not hold: a derived struct's field without one
// (at the field), an array of more than 32 elements, and
// `unwrap_or_default` of a type without one. An enum's derive bounds no
// parameters, as in rustc.

struct NoDefault;

#[derive(Default)]
struct S {
    a: i32,
    b: NoDefault,
}

#[derive(Default)]
struct W<T>(T);

#[derive(Default)]
enum E<T> {
    #[default]
    A,
    B(T),
}

fn main() {
    let _: W<u8> = Default::default();
    let _: E<NoDefault> = Default::default();
    let _: [u8; 33] = Default::default();
    let _: [u8; 32] = Default::default();
    let _ = Some(NoDefault).unwrap_or_default();
}

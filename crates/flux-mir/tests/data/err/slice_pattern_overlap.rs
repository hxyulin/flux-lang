// A rest pattern's move and an element's overlap when the rest covers the
// element (E0382), whichever comes first; moves of disjoint parts are fine.

struct S(String);

fn s() -> S {
    S(String::new())
}

fn rest_then_element() {
    let a = [s(), s(), s()];
    let [_, rest @ ..] = a;
    let [_, b, _] = a;
    drop((rest, b));
}

fn element_then_rest() {
    let a = [s(), s(), s()];
    let [_, b, _] = a;
    let [_, rest @ ..] = a;
    drop((rest, b));
}

fn last_then_rest() {
    let a = [s(), s(), s()];
    let [.., c] = a;
    let [_, rest @ ..] = a;
    drop((rest, c));
}

fn two_rests() {
    let a = [s(), s(), s()];
    let [_, r1 @ ..] = a;
    let [r2 @ .., _] = a;
    drop((r1, r2));
}

fn rest_then_borrow() {
    let a = [s(), s(), s()];
    let [_, rest @ ..] = a;
    let [_, _, ref c] = a;
    drop(rest);
    let _ = c;
}

fn disjoint() {
    let a = [s(), s(), s()];
    let [x, ..] = a;
    let [_, rest @ ..] = a;
    let [y, z] = rest;
    drop((x, y, z));
}

fn main() {
    rest_then_element();
    element_then_rest();
    last_then_rest();
    two_rests();
    rest_then_borrow();
    disjoint();
}

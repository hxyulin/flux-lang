enum Action { State(&'static [(u8, Action)]), End }
static START: [(u8, Action); 2] = [(1, Action::State(&[(2, Action::State(&START))])), (3, Action::End)];
fn next(state: &'static [(u8, Action)]) -> &'static [(u8, Action)] {
    match state[0].1 { Action::State(next) => next, Action::End => state }
}
fn main() {
    let nested = next(&START);
    let root = next(nested);
    println!("{} {} {}", nested[0].0, root[1].0, std::ptr::eq(root.as_ptr(), START.as_ptr()));
}

// `?` moves its operand, as rustc's `Try::branch(x)` does, so using a
// value that is not `Copy` again is E0382, even after `?` on a field.

struct S(i32);

fn use_after(x: Option<S>) -> Option<i32> {
    let s = x?;
    let _again = x;
    Some(s.0)
}

fn field_after(pair: (Option<S>, i32)) -> Option<i32> {
    let s = pair.0?;
    let _whole = pair;
    Some(s.0)
}

fn main() {
    use_after(None);
    field_after((None, 1));
}

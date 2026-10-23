fn duplicate<T: Clone>(value: &T) -> T {
    value.clone()
}
unsafe fn danger(value: &i32) -> i32 {
    *value
}
fn static_only(value: &'static i32) -> i32 {
    *value
}
fn needs_fn<F: Fn(&i32) -> i32>(_: F) {}

fn main() {
    let pointer: unsafe fn(&i32) -> i32 = danger;
    let cloned = duplicate(&pointer);
    let safe: fn(&i32) -> i32 = cloned;
    cloned(&1);
    needs_fn(cloned);
    let restricted = duplicate(&static_only);
    let general: fn(&i32) -> i32 = restricted;
}

fn tied<'a>(_: &'a mut &'a i32, _: &'a i32) {}
fn tied_clone() {
    let item = duplicate(&tied);
    let _: fn(&mut &i32, &i32) = item;
}

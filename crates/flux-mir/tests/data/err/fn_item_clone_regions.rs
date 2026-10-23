fn duplicate<T: Clone>(value: &T) -> T {
    value.clone()
}
fn identity(value: &i32) -> &i32 {
    value
}
fn require_static(value: &'static i32) -> i32 {
    *value
}

fn main() {
    let cloned = duplicate(&identity);
    let local = 1;
    let escaped: &'static i32 = cloned(&local);
    let restricted = duplicate(&require_static);
    restricted(&local);
    let pointer: fn(&i32) -> &i32 = identity;
    let cloned_pointer = duplicate(&pointer);
    let pointer_escaped: &'static i32 = cloned_pointer(&local);
    println!("{} {}", escaped, pointer_escaped);
}

#[derive(Clone, Copy)]
enum Void {}
#[derive(Clone, Copy)]
enum Value { Live(u64), Dead(Void) }
fn copy(value: &Value) -> Value { *value }
fn main() { match copy(&Value::Live(37)) { Value::Live(n) => assert_eq!(n,37), Value::Dead(x) => match x {} } }

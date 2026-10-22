// `?` in MIR: the operand moves into a temporary, whose discriminant is
// tested; the `Some` or `Ok` value is moved out, or else the function
// returns `None`, or `Err` with the error, converted by a call of
// `From::from` where the types differ, after dropping what is in scope,
// as `return` does. The operand is moved even where only its value
// would be, as rustc's `Try::branch(x)` takes it.

struct Guard(i32);

impl Drop for Guard {
    fn drop(&mut self) {}
}

struct Wide(i64);

impl From<i32> for Wide {
    fn from(n: i32) -> Wide {
        Wide(n as i64)
    }
}

fn option(x: Option<i32>) -> Option<i32> {
    Some(x? + 1)
}

fn result(x: Result<Guard, i32>) -> Result<i32, i32> {
    let g = x?;
    Ok(g.0)
}

fn converted(x: Result<i32, i32>) -> Result<i32, Wide> {
    let _guard = Guard(1);
    Ok(x?)
}

fn main() {
    option(None);
    let _ = result(Err(1));
    if let Err(wide) = converted(Ok(2)) {
        let _ = wide.0;
    }
}

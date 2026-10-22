// `?` on `Option` and `Result`: its type is the `Some` or `Ok` value's,
// in functions returning an `Option` of any type, or a `Result` whose
// error type converts from the operand's with `From`: itself, the
// program's impls, `core`'s integer widenings, and generic error types.
// Literals and other unknowns take their types from the context.

struct Small;

struct Big;

impl From<Small> for Big {
    fn from(_: Small) -> Big {
        Big
    }
}

fn option(a: Option<i32>, b: Option<u8>) -> Option<u64> {
    let x = a?;
    let y: u8 = b?;
    Some(x as u64 + y as u64)
}

fn same_error(r: Result<i32, Small>) -> Result<i64, Small> {
    Ok(r? as i64)
}

fn converted(r: Result<i32, Small>) -> Result<i32, Big> {
    let v = r?;
    Ok(v + 1)
}

fn widened(r: Result<bool, u8>, s: Result<i8, bool>) -> Result<i16, i64> {
    let flag = r?;
    let n = s?;
    Ok(if flag { n as i16 } else { 0 })
}

fn generic<T, E>(r: Result<T, E>) -> Result<T, E> {
    let value = r?;
    Ok(value)
}

fn nested(o: Option<Option<i32>>) -> Option<i32> {
    Some(o??.abs())
}

fn literal() -> Option<u16> {
    let n = Some(5)?;
    Some(n)
}

fn early(values: (Option<i32>, Option<i32>)) -> Option<i32> {
    let (a, b) = values;
    if a? > 0 {
        return Some(b? * 2);
    }
    None
}

fn main() {
    option(Some(1), None);
    let _ = same_error(Ok(1));
    let _ = converted(Err(Small));
    let _ = widened(Ok(true), Err(false));
    let _ = generic::<i32, u8>(Ok(1));
    nested(Some(Some(1)));
    literal();
    early((Some(1), None));
}

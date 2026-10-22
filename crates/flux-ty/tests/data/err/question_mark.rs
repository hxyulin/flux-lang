// `?` where rustc's `Try` and `FromResidual` bounds do not hold, with
// its messages (E0277): on a value that is neither an `Option` nor a
// `Result`, in a function returning neither or the other one, and with
// an error the function's error type does not convert from; its value
// where another type is wanted (E0308); and in constants (E0572) and
// `const fn`s (E0658).

struct E1;

struct E2;

struct E3;

impl From<E1> for E2 {
    fn from(_: E1) -> E2 {
        E2
    }
}

fn not_try(x: i32, o: &Option<i32>) -> Option<i32> {
    let _a = x?;
    Some(o? + 1)
}

fn returns_value(x: Option<i32>) -> i32 {
    x?
}

fn returns_unit(x: Result<i32, E1>) {
    x?;
}

fn returns_generic<T>(x: Option<T>) -> T {
    x?
}

fn result_in_option(x: Result<i32, E1>) -> Option<i32> {
    Some(x?)
}

fn option_in_result(x: Option<i32>) -> Result<i32, E1> {
    Ok(x?)
}

fn no_conversion(x: Result<i32, E1>, y: Result<i32, E2>) -> Result<i32, E3> {
    Ok(x? + y?)
}

fn no_reverse_conversion(x: Result<i32, E2>) -> Result<i32, E1> {
    Ok(x?)
}

fn narrowing(x: Result<i32, i64>) -> Result<i32, i32> {
    Ok(x?)
}

fn generic_error<E>(x: Result<i32, E>) -> Result<i32, E1> {
    Ok(x?)
}

fn incompatible(x: Result<i32, E1>) -> Result<u8, E1> {
    let y: u8 = x?;
    Ok(y)
}

const X: Option<i32> = Some(1);
const C: Option<i32> = Some(X? + 1);

const fn in_const_fn(x: Option<i32>) -> Option<i32> {
    Some(x? + 1)
}

fn main() {}

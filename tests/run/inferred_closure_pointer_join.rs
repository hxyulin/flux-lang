#[derive(Clone)]
struct Value(u32);
enum Format {
    First,
    Second,
    Third,
    Other,
}

fn first(value: &Value) -> Result<Value, ()> {
    Ok(Value(value.0 + 1))
}
fn second(value: &Value, stage: u32) -> Result<Value, ()> {
    Ok(Value(value.0 + stage))
}
fn third(value: &Value, stage: u32) -> Result<Value, ()> {
    Ok(Value(value.0 * stage))
}

fn convert(values: &[Value], format: Format, stage: u32) -> Result<Vec<Value>, ()> {
    let conv = match format {
        Format::First => |value, _| first(value),
        Format::Second => |value, stage| second(value, stage),
        Format::Third => |value, stage| third(value, stage),
        Format::Other => return Err(()),
    };
    values.iter().map(|value| conv(value, stage)).collect()
}

fn main() {
    let values = [Value(7), Value(13)];
    for format in [Format::First, Format::Second, Format::Third, Format::Other] {
        println!(
            "{:?}",
            convert(&values, format, 4).map(|v| v.iter().map(|value| value.0).collect::<Vec<_>>())
        );
    }
}

// A body with an error reports no unknown let types, as rustc reports
// none once it has errors.
fn f(s: &str) -> Option<String> {
    let date = s.split(" ").next().map(|s| s.no_such_method());
    date.map(|s| s.len().to_string())
}

fn main() {}

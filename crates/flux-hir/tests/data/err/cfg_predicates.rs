#[cfg]
fn bare() {}
#[cfg = "test"]
fn value() {}
#[cfg[]]
fn brackets() {}
#[cfg()]
fn empty() {}
#[cfg(all(test test))]
fn missing_comma() {}
#[cfg(not())]
fn empty_not() {}
#[cfg(not(test, unix))]
fn extra_not() {}
#[cfg(target_os = 1)]
fn non_string() {}
#[cfg(test, unix)]
fn extra_predicate() {}
#[cfg(unknown(test))]
fn unknown_operator() {}
#[cfg(test)]
#[cfg(not())]
fn validate_every_attribute() {}
#[cfg(all(any(), not()))]
fn validate_every_operand() {}

fn builtin_predicates() {
    let _ = cfg!(all(test test));
    let _ = cfg!(not());
    let _ = cfg!(not(test, unix));
    let _ = cfg!(target_os = 1);
}

struct S;
trait Invalid {
    #[cfg(test)]
    #[cfg(not())]
    type Output;
}
impl S {
    #[cfg(test)]
    #[cfg(not())]
    fn method() {}
}

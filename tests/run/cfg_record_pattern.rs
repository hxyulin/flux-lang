struct Value { value: usize }
fn main(){
    let Value { #[cfg(any())] value: nonexistent, #[cfg(all())] value: selected } = Value{value:17};
    println!("{selected}");
}

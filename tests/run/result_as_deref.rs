fn main() {
    let value: Result<String, usize> = Ok(String::from("hello"));
    assert_eq!(value.as_deref(), Ok("hello"));
    let error: Result<String, usize> = Err(17);
    assert_eq!(error.as_deref(), Err(&17));
    let array: Result<Box<[usize; 2]>, usize> = Ok(Box::new([3, 7]));
    assert_eq!(array.as_deref().unwrap()[1], 7);
    println!("{} {} {}", value.unwrap(), error.unwrap_err(), array.unwrap()[0]);
}

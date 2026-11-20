fn pack<'a, F: Fn(&u32) -> u32 + 'a>(callback: F) -> Box<dyn Fn(&u32)->u32 + 'a> {
    let boxed = Box::new(Box::new(callback) as Box<dyn Fn(&u32)->u32>);
    let raw = Box::into_raw(boxed);
    unsafe { Box::from_raw(raw) }
}
fn main() {
    let input = 21;
    let cb = pack(|x| *x + input);
    println!("{}", cb(&21));
}

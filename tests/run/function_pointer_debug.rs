fn value(x: u32) -> u32 {
    x + 1
}
unsafe fn unchecked(x: u32) -> u32 {
    x + 2
}
extern "C" fn foreign(x: u32) -> u32 {
    x + 3
}
unsafe extern "C" fn foreign_unsafe(x: u32) -> u32 {
    x + 4
}
fn borrow<'a>(x: &'a u32) -> &'a u32 {
    x
}
struct Command(u32);
fn command(x: Command) -> Command {
    x
}
fn main() {
    let p: fn(u32) -> u32 = value;
    let raw = (p as usize) as *const ();
    assert_eq!(format!("{:?}", p), format!("{:p}", raw));
    assert_eq!(format!("{:#?}", p), format!("{:#p}", raw));
    assert_eq!(format!("{:>24?}", p), format!("{:>24p}", raw));
    assert_eq!(format!("{:?}", Some(p)), format!("Some({:p})", raw));
    let p: unsafe fn(u32) -> u32 = unchecked;
    assert_eq!(
        format!("{:?}", p),
        format!("{:p}", (p as usize) as *const ())
    );
    let p: extern "C" fn(u32) -> u32 = foreign;
    assert_eq!(
        format!("{:?}", p),
        format!("{:p}", (p as usize) as *const ())
    );
    let p: unsafe extern "C" fn(u32) -> u32 = foreign_unsafe;
    assert_eq!(
        format!("{:?}", p),
        format!("{:p}", (p as usize) as *const ())
    );
    let p: for<'a> fn(&'a u32) -> &'a u32 = borrow;
    assert_eq!(
        format!("{:?}", p),
        format!("{:p}", (p as usize) as *const ())
    );
    let x = 7;
    assert_eq!(*p(&x), 7);
    let p: fn(Command) -> Command = command;
    assert_eq!(
        format!("{:?}", Some(p)),
        format!("Some({:p})", (p as usize) as *const ())
    );
    assert_eq!(p(Command(9)).0, 9);
    println!("function pointers actual-address 6");
}

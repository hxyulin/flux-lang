struct Guard(&'static str);
impl Drop for Guard {
    fn drop(&mut self) {
        println!("drop {}", self.0);
    }
}
fn get(flag: bool) -> Result<Guard, Guard> {
    if flag {
        Ok(Guard("ok"))
    } else {
        Err(Guard("err"))
    }
}
fn borrow(flag: bool) {
    let Ok(data) = &mut get(flag) else {
        println!("else borrow");
        return;
    };
    println!("borrow {}", data.0);
}
fn pattern(flag: bool) {
    let Ok(ref data) = get(flag) else {
        println!("else pattern");
        return;
    };
    println!("pattern {}", data.0);
}
fn moved(flag: bool) {
    let Ok(data) = get(flag) else {
        println!("else move");
        return;
    };
    println!("move {}", data.0);
}
fn main() {
    mixed();
    mixed_reversed();
    borrow(false);
    pattern(false);
    moved(false);
    borrow(true);
    pattern(true);
    moved(true);
}
fn wrap(_: &Guard) -> Option<Guard> {
    None
}
fn mixed() {
    let (_, Some(_)) = (&Guard("extended first"), wrap(&Guard("short second"))) else {
        println!("else mixed first");
        return;
    };
}
fn mixed_reversed() {
    let (Some(_), _) = (wrap(&Guard("short first")), &Guard("extended second")) else {
        println!("else mixed second");
        return;
    };
}

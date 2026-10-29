fn match_while_borrowed() {
    let mut o = Some(1);
    let r = &mut o;
    match o {
        Some(_) => {}
        None => {}
    }
    *r = None;
}

fn if_let_while_borrowed() {
    let mut o: Option<i32> = None;
    let r = &mut o;
    if let Some(_) = o {}
    *r = Some(2);
}

fn shared_borrow_is_fine() {
    let o = Some(1);
    let r = &o;
    match o {
        Some(_) => {}
        None => {}
    }
    let _ = r;
}

fn main() {
    match_while_borrowed();
    if_let_while_borrowed();
    shared_borrow_is_fine();
}

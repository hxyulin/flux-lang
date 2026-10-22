fn mutate_while_iterating() {
    let mut a = [1, 2, 3];
    for x in &a {
        a[0] = *x;
    }
}

fn item_escapes() -> &'static i32 {
    let a = [1];
    let s: &[i32] = &a;
    s.iter().next().unwrap()
}

fn first_outlives() {
    let r;
    {
        let a = [1, 2];
        let s: &[i32] = &a;
        r = s.first().unwrap();
    }
    let _ = *r;
}

fn kept_iterator() {
    let mut a = [1, 2];
    let s: &[i32] = &a;
    let mut it = s.iter();
    a[1] = 5;
    let _ = it.next();
}

fn main() {}

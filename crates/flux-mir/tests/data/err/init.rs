fn unassigned() -> i32 {
    let x: i32;
    x + 1
}

fn conditional(flag: bool) -> i32 {
    let x;
    if flag {
        x = 1;
    }
    x
}

fn twice(flag: bool) -> i32 {
    let x;
    x = 1;
    if flag {
        x = 2;
    }
    x
}

fn in_loop() {
    let x: i32;
    loop {
        x = 1;
    }
}

fn after_loop() -> i32 {
    let x: i32;
    loop {
        x = 1;
        break;
    }
    x
}

fn comparisons() {
    let _ = true == false == false;
    let _ = 1 < 2 > 3;
    let _ = 1 == 2 && 3 == 4 == true;
}

fn ranges() {
    let _ = 1..2..3;
    let _ = ..1..2;
    let _ = 1..=2..3;
}

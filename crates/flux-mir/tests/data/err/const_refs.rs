const LOCAL: &i32 = {
    let a = 1;
    &a
};

static FROM_CALL: &i32 = &one();

const fn one() -> i32 {
    1
}

fn main() {
    let _ = *LOCAL + *FROM_CALL;
}

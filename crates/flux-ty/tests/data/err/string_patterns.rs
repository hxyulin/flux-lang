fn strings(s: &str) -> i32 {
    match s {
        "a" => 1,
        "b" => 2,
    }
}

fn tuples(s: &str) -> i32 {
    match (s, 1) {
        ("a", 1) => 1,
        (_, 2) => 2,
    }
}

fn refutable(s: &str) {
    let "a" = s;
}

fn main() {}

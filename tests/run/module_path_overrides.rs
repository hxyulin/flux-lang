#[path = "module_path_overrides"]
mod renamed {
    #[cfg_attr(all(), path = "value.rs")]
    mod chosen;

    pub fn value() -> i32 {
        chosen::value()
    }
}

fn main() {
    #[path = "module_path_overrides/value.rs"]
    mod local;

    assert_eq!(renamed::value(), 37);
    println!("{}", renamed::value() + local::value());
}

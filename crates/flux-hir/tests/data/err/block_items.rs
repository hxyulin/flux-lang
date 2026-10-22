fn later() -> i32 {
    0
}

fn outer(x: i32) -> i32 {
    let y = 5;
    fn uses_local() -> i32 {
        y
    }
    fn uses_param() -> i32 {
        x
    }
    const C: i32 = y;
    mod file_module;
    {
        fn scoped() {}
    }
    scoped();
    self::later();
    super::later();
    0
}

fn private_items() {
    mod hidden {
        fn secret() {}
    }
    hidden::secret();
}

fn main() {}

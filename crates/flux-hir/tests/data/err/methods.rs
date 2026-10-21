struct S {
    a: i32,
}

impl S {
    fn new() -> Self {
        Self { a: 0 }
    }

    fn get(&self) -> i32 {
        self.a
    }

    fn twice(&self) -> i32 {
        get() * 2
    }

    fn no_receiver() -> i32 {
        self.a
    }

    fn get(&self) -> i32 {
        0
    }
}

impl S {
    fn new() -> S {
        S { a: 1 }
    }
}

impl Missing {
    fn f() {}
}

impl i32 {
    fn g() {}
}

fn free(self) {}

fn free_ref(&self) {}

fn self_value() -> i32 {
    self
}

fn self_type(s: Self) {}

fn self_ctor() {
    let _ = Self { a: 1 };
}

fn unknown_assoc() {
    S::nope();
}

impl S {
    fn via_self() -> i32 {
        Self::missing()
    }
}

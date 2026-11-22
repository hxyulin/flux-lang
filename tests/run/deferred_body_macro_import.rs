//! Deferred macro blocks can be empty or introduce a new macro import.

fn main() {
    println!("{} {}", outer::plain!(), outer::call!());
}

mod outer {
    macro_rules! plain {
        () => {{ crate::values::number!() }};
    }
    macro_rules! call {
        () => {{
            use crate::values::number;
            number!()
        }};
    }
    pub(crate) use {call, plain};
}

mod values {
    macro_rules! number {
        () => { 42 };
    }
    pub(crate) use number;
}

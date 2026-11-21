macro_rules! existing {
    () => { 1 };
}

alias::declare!();

mod alias {
    #[macro_export]
    macro_rules! declare_number {
        () => { const DECLARED: u8 = 4; };
    }

    #[doc = concat!("This import ", "participates in item lookup.")]
    pub(crate) use crate::declare_number as declare;
}

mod consumer {
    pub fn before() -> u8 {
        existing!()
    }

    crate::later::define!($);
    pub(crate) use value as imported;
    make_function!();

    #[deprecated = concat!("generated: ", message!())]
    pub fn attributed() {}

    pub fn after() -> output_type!() {
        value!()
    }

    mod nested {
        pub fn inherited() -> u8 {
            value!()
        }
    }

    macro_rules! value {
        () => { 3 };
    }

    pub const LAST: u8 = value!();
    pub use nested::inherited;
}

mod later {
    macro_rules! define {
        ($d:tt) => {
            crate::later::define_value!();
            macro_rules! output_type {
                () => { u8 };
            }
            macro_rules! message {
                () => { "deferred" };
            }
            macro_rules! make_function {
                () => {
                    pub fn generated() -> u8 { value!() }
                };
            }
        };
    }
    macro_rules! define_value {
        () => {
            macro_rules! value {
                () => { 2 };
            }
        };
    }
    pub(crate) use {define, define_value};
}

#[allow(deprecated)]
fn main() {
    consumer::attributed();
    assert_eq!(DECLARED, 4);
    assert_eq!(consumer::before(), 1);
    assert_eq!(consumer::after(), 2);
    assert_eq!(consumer::inherited(), 2);
    assert_eq!(consumer::generated(), 2);
    assert_eq!(consumer::imported!(), 2);
    assert_eq!(consumer::LAST, 3);
    assert_eq!(existing!(), 1);
}

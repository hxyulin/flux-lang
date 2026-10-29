use std::fmt::Write;

struct Counter(u32);
trait Read {
    fn read(&self) -> u32;
}
#[automatically_derived]
impl Read for Counter {
    fn read(&self) -> u32 {
        self.0
    }
}

macro_rules! source_text {
    ($e:expr) => {
        std::stringify!($e)
    };
}

fn main() {
    use std::stringify as source;
    assert_eq!(Counter(37).read(), 37);
    let first = crate::late_make!(7);
    let second = crate::late_make!(11);
    assert_eq!(first.0 + second.0, 18);
    assert_eq!(first.1, 1);
    assert_eq!(late_module::read(), 13);
    let mut output = String::new();
    std::write!(
        &mut output,
        "{}:{}",
        source!(some::path),
        source_text!(1 + 2)
    )
    .unwrap();
    assert_eq!(output, "some::path:1 + 2");
    assert_eq!(std::stringify!(r#type), "r#type");
    assert_eq!(std::stringify!(), "");
    assert_eq!(std::stringify!('a), "'a");
    assert_eq!(core::stringify!((a, b)), "(a, b)");
    println!("generated {} {}", Counter(37).read(), output);
}

mod late_module {
    use crate::exports::late;
    pub fn read() -> u8 {
        late!(13).0
    }
}
mod exports {
    pub use crate::late_make as late;
}
#[macro_export]
macro_rules! late_make {
    ($value:expr) => {{
        use core::mem::size_of;
        #[derive(Clone, Copy)]
        struct Local(u8);
        let value = Local($value).clone();
        (value.0, size_of::<Local>())
    }};
}

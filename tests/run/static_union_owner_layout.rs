use std::{mem, rc::Rc, sync::Arc};

const _: fn() = || {
    let _ = mem::transmute::<[usize; 2], &'static str>;
    let _ = mem::transmute::<Box<str>, Arc<str>>;
    let _ = mem::transmute::<Box<str>, Rc<str>>;
};

#[derive(Clone, Copy)]
#[repr(C)]
struct Text {
    text: &'static str,
    tag: u8,
}

#[repr(C)]
union Value {
    inline: [u8; 24],
    text: Text,
    direct: &'static str,
}

const VALUE: Value = Value {
    text: Text {
        text: "hello",
        tag: 9,
    },
};

fn main() {
    let direct = Value { direct: "world" };
    let (text, tag, other) = unsafe { (VALUE.text.text, VALUE.text.tag, direct.direct) };
    println!("{text} {tag} {other}");
    println!(
        "{} {} {} {}",
        mem::size_of::<Box<str>>(),
        mem::size_of::<Arc<str>>(),
        mem::size_of::<Rc<str>>(),
        mem::size_of::<Arc<u32>>()
    );
}

//! Erasing a generic callback keeps its proven static return lifetime.
use std::borrow::Cow;

type Callback = Box<dyn Fn(&str, u32) -> Option<Cow<'static, [u8]>>>;

fn erase<F>(callback: F) -> Callback
where
    F: Fn(&str, u32) -> Option<Cow<'static, [u8]>> + 'static,
{
    Box::new(callback)
}

fn fallback(name: &str, size: u32) -> Option<Cow<'static, [u8]>> {
    if !name.is_empty() && size == 16 {
        Some(Cow::Borrowed(b"cursor"))
    } else {
        None
    }
}

fn main() {
    let callback = erase(fallback);
    let name = String::from("arrow");
    let data = callback(&name, 16).unwrap();
    drop(name);
    println!("{:?}", data.as_ref());
    assert!(callback("arrow", 24).is_none());
}

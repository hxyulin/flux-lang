#![no_std]
extern crate std;
extern crate proc_macro;
use proc_macro::{TokenStream, TokenTree, Ident, Group, Literal, Punct};
mod nested { pub fn available() -> bool { proc_macro::is_available() } }
fn main() {
    assert!(!nested::available());
    assert!(!proc_macro::is_available());
    let mut tokens = TokenStream::new();
    assert!(tokens.is_empty());
    tokens.extend(std::iter::empty::<TokenTree>());
    tokens.extend([TokenStream::default(), TokenStream::new()]);
    tokens.extend(std::iter::empty::<Ident>());
    tokens.extend(std::iter::empty::<Group>());
    tokens.extend(std::iter::empty::<Literal>());
    tokens.extend(std::iter::empty::<Punct>());
    let cloned = tokens.clone();
    let mut iter = cloned.into_iter();
    assert_eq!(iter.size_hint(), (0, Some(0)));
    assert!(iter.next().is_none());
    assert_eq!(iter.clone().count(), 0);
    let collected: TokenStream = std::iter::empty::<TokenTree>().collect();
    let joined: TokenStream = [tokens, collected].into_iter().collect();
    assert!(joined.is_empty());
    std::println!("{} {:?} {}", proc_macro::is_available(), joined, joined);
}

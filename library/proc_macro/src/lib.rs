//! Target-side procedural macro API outside an active compiler host.
//!
//! Empty token streams work in ordinary programs. Token construction, parsing
//! and span queries require an active host and panic here. Flux macro hosts
//! bind their separate invocation API as an explicit dependency.
use std::fmt;
use std::marker::PhantomData;
use std::rc::Rc;
use std::str::FromStr;

fn unavailable() -> ! {
    panic!("procedural macro API is used outside of a procedural macro")
}

/// Whether this thread is executing a procedural macro invocation.
pub fn is_available() -> bool { false }

/// An opaque stream of compiler tokens.
#[derive(Clone)]
pub struct TokenStream { marker: PhantomData<Rc<()>> }
impl TokenStream {
    /// Creates an empty token stream without requiring a compiler host.
    pub fn new() -> Self { Self { marker: PhantomData } }
    /// Returns whether the stream contains no tokens.
    pub fn is_empty(&self) -> bool { true }
}
impl Default for TokenStream { fn default() -> Self { Self::new() } }
impl fmt::Display for TokenStream {
    fn fmt(&self, _: &mut fmt::Formatter<'_>) -> fmt::Result { Ok(()) }
}
impl fmt::Debug for TokenStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str("TokenStream []") }
}
/// An error parsing compiler tokens.
#[derive(Debug)]
pub struct LexError { message: String, marker: PhantomData<Rc<()>> }
impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(&self.message) }
}
impl std::error::Error for LexError {}
impl FromStr for TokenStream {
    type Err = LexError;
    fn from_str(_: &str) -> Result<Self, LexError> { unavailable() }
}
impl From<TokenTree> for TokenStream { fn from(_: TokenTree) -> Self { unavailable() } }
impl Extend<TokenTree> for TokenStream {
    fn extend<I: IntoIterator<Item=TokenTree>>(&mut self, values:I) {
        for _ in values { unavailable(); }
    }
}
impl Extend<TokenStream> for TokenStream {
    fn extend<I: IntoIterator<Item=TokenStream>>(&mut self, values:I) {
        for value in values { self.extend(value); }
    }
}
impl FromIterator<TokenTree> for TokenStream {
    fn from_iter<I: IntoIterator<Item=TokenTree>>(values:I) -> Self {
        let mut stream = Self::new(); stream.extend(values); stream
    }
}
impl FromIterator<TokenStream> for TokenStream {
    fn from_iter<I: IntoIterator<Item=TokenStream>>(values:I) -> Self {
        let mut stream = Self::new(); stream.extend(values); stream
    }
}
/// Iteration over the top-level tokens of a stream.
pub mod token_stream {
    use super::*;
    /// An owned iterator over compiler tokens.
    #[derive(Clone)]
    pub struct IntoIter { pub(super) marker: PhantomData<Rc<()>> }
    impl Iterator for IntoIter {
        type Item = TokenTree;
        fn next(&mut self) -> Option<TokenTree> { None }
        fn size_hint(&self) -> (usize, Option<usize>) { (0,Some(0)) }
    }
}
impl IntoIterator for TokenStream {
    type Item = TokenTree;
    type IntoIter = token_stream::IntoIter;
    fn into_iter(self) -> Self::IntoIter { token_stream::IntoIter { marker: PhantomData } }
}
/// A compiler source location and name-resolution context.
#[derive(Copy,Clone)]
pub struct Span { marker: PhantomData<Rc<()>> }
impl Span {
    pub fn call_site() -> Self { unavailable() }
    pub fn mixed_site() -> Self { unavailable() }
    pub fn start(&self) -> Self { unavailable() }
    pub fn end(&self) -> Self { unavailable() }
    pub fn line(&self) -> usize { unavailable() }
    pub fn column(&self) -> usize { unavailable() }
    pub fn file(&self) -> String { unavailable() }
    pub fn local_file(&self) -> Option<std::path::PathBuf> { unavailable() }
    pub fn resolved_at(&self, _: Self) -> Self { unavailable() }
    pub fn located_at(&self, _: Self) -> Self { unavailable() }
    pub fn source_text(&self) -> Option<String> { unavailable() }
}
impl fmt::Debug for Span {
    fn fmt(&self, _: &mut fmt::Formatter<'_>) -> fmt::Result { unavailable() }
}
/// One group, identifier, punctuation character, or literal.
#[derive(Clone,Debug)]
pub enum TokenTree { Group(Group), Ident(Ident), Punct(Punct), Literal(Literal) }
impl TokenTree {
    pub fn span(&self) -> Span { unavailable() }
    pub fn set_span(&mut self, _: Span) { unavailable() }
}
impl fmt::Display for TokenTree {
    fn fmt(&self, _: &mut fmt::Formatter<'_>) -> fmt::Result { unavailable() }
}
/// The delimiter around a group of tokens.
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum Delimiter { Parenthesis, Brace, Bracket, None }
/// Whether punctuation joins the following character.
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum Spacing { Alone, Joint }
/// A delimited group of compiler tokens.
#[derive(Clone)]
pub struct Group { span: Span }
impl Group {
    pub fn new(_: Delimiter, _: TokenStream) -> Self { unavailable() }
    pub fn delimiter(&self) -> Delimiter { unavailable() }
    pub fn stream(&self) -> TokenStream { unavailable() }
    pub fn span(&self) -> Span { unavailable() }
    pub fn span_open(&self) -> Span { unavailable() }
    pub fn span_close(&self) -> Span { unavailable() }
    pub fn set_span(&mut self, _: Span) { unavailable() }
}
/// An identifier and its compiler span.
#[derive(Clone)]
pub struct Ident { span: Span }
impl Ident {
    pub fn new(_: &str, _: Span) -> Self { unavailable() }
    pub fn new_raw(_: &str, _: Span) -> Self { unavailable() }
    pub fn span(&self) -> Span { unavailable() }
    pub fn set_span(&mut self, _: Span) { unavailable() }
}
/// A punctuation character and its compiler span.
#[derive(Clone)]
pub struct Punct { span: Span }
impl Punct {
    pub fn new(_: char, _: Spacing) -> Self { unavailable() }
    pub fn as_char(&self) -> char { unavailable() }
    pub fn spacing(&self) -> Spacing { unavailable() }
    pub fn span(&self) -> Span { unavailable() }
    pub fn set_span(&mut self, _: Span) { unavailable() }
}
impl PartialEq<char> for Punct { fn eq(&self, _: &char) -> bool { unavailable() } }
impl PartialEq<Punct> for char { fn eq(&self, _: &Punct) -> bool { unavailable() } }
/// A compiler literal token.
#[derive(Clone)]
pub struct Literal { span: Span }
impl Literal {
    pub fn string(_: &str) -> Self { unavailable() }
    pub fn character(_: char) -> Self { unavailable() }
    pub fn byte_character(_: u8) -> Self { unavailable() }
    pub fn byte_string(_: &[u8]) -> Self { unavailable() }
    pub fn c_string(_: &std::ffi::CStr) -> Self { unavailable() }
    pub fn span(&self) -> Span { unavailable() }
    pub fn set_span(&mut self, _: Span) { unavailable() }
}
impl FromStr for Literal {
    type Err = LexError;
    fn from_str(_: &str) -> Result<Self, LexError> { unavailable() }
}
macro_rules! numeric_literal {
    ($ty:ty,$suffixed:ident,$unsuffixed:ident) => {
        impl Literal {
            pub fn $suffixed(_: $ty) -> Self { unavailable() }
            pub fn $unsuffixed(_: $ty) -> Self { unavailable() }
        }
    };
}
numeric_literal!(u8,u8_suffixed,u8_unsuffixed);
numeric_literal!(u16,u16_suffixed,u16_unsuffixed);
numeric_literal!(u32,u32_suffixed,u32_unsuffixed);
numeric_literal!(u64,u64_suffixed,u64_unsuffixed);
numeric_literal!(u128,u128_suffixed,u128_unsuffixed);
numeric_literal!(usize,usize_suffixed,usize_unsuffixed);
numeric_literal!(i8,i8_suffixed,i8_unsuffixed);
numeric_literal!(i16,i16_suffixed,i16_unsuffixed);
numeric_literal!(i32,i32_suffixed,i32_unsuffixed);
numeric_literal!(i64,i64_suffixed,i64_unsuffixed);
numeric_literal!(i128,i128_suffixed,i128_unsuffixed);
numeric_literal!(isize,isize_suffixed,isize_unsuffixed);
numeric_literal!(f32,f32_suffixed,f32_unsuffixed);
numeric_literal!(f64,f64_suffixed,f64_unsuffixed);
macro_rules! token_impls {
    ($ty:ident) => {
        impl From<$ty> for TokenTree { fn from(value:$ty)->Self { Self::$ty(value) } }
        impl Extend<$ty> for TokenStream {
            fn extend<I:IntoIterator<Item=$ty>>(&mut self, values:I) {
                self.extend(values.into_iter().map(TokenTree::$ty));
            }
        }
        impl fmt::Debug for $ty {
            fn fmt(&self, _: &mut fmt::Formatter<'_>) -> fmt::Result { unavailable() }
        }
        impl fmt::Display for $ty {
            fn fmt(&self, _: &mut fmt::Formatter<'_>) -> fmt::Result { unavailable() }
        }
    };
}
token_impls!(Group);
token_impls!(Ident);
token_impls!(Punct);
token_impls!(Literal);

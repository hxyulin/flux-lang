// `#[derive]` rejected while lowering: on items other than structs
// (E0774), without a list of traits, and for traits flux cannot derive
// yet, named alone or by path. The derives it can make are still made
// for the other traits in the same list.

// E0774: on functions, traits, impls, modules, constants, imports and
// methods.
#[derive(Clone)]
fn function() {}

#[derive(Clone)]
trait Tr {}

#[derive(Clone)]
impl Tr for i32 {}

#[derive(Clone)]
mod module {}

#[derive(Clone)]
const C: i32 = 1;

#[derive(PartialEq)]
use core::clone::Clone as Duplicate;

struct S;

impl S {
    #[derive(Clone)]
    fn method(&self) {}
}

// Malformed: no list of traits.
#[derive]
struct Bare;

#[derive = "Clone"]
struct Value;

// `Debug` and `Default` are derived; `Hash` not yet, alone or among
// supported ones.
#[derive(Debug)]
struct Debugged;

#[derive(Clone, Default, Copy, Hash)]
struct Mixed;

#[derive(PartialOrd, Ord, PartialEq, Eq)]
struct Ordered;

// Paths to the traits are not supported yet either.
#[derive(core::clone::Clone, ::core::cmp::PartialEq)]
struct ByPath;

// Malformed lists, reported as rustc does.
#[derive(,Clone)]
struct LeadingComma;

#[derive(Clone,,PartialEq)]
struct DoubleComma;

#[derive(Clone(x))]
struct WithArguments;

#[derive(Clone Copy)]
struct NoComma;

#[derive("Clone")]
struct Literal;

#[derive[Clone]]
struct Brackets;

#[derive(Nope)]
struct Unknown;

// On fields and parameters, `derive` is an attribute macro where only
// inert attributes may be.
struct Fields {
    #[derive(Clone)]
    x: i32,
}

fn parameter(#[derive(Clone)] _x: i32) {}

fn main() {}

// Macro calls that do not expand, with rustc's errors: unknown macros,
// calls before the definition, no rule matching, ambiguity, recursion
// past the limit, invalid definitions and expansions.

macro_rules! one {
    ($a:expr) => {
        $a
    };
    ($a:expr, $b:ident) => {
        $a
    };
}

macro_rules! ambiguous {
    ($($i:ident)* $j:ident) => {};
}

macro_rules! forever {
    ($a:expr) => {
        forever!($a)
    };
}

macro_rules! extra {
    () => {
        1; 2
    };
}

macro_rules! zip {
    ($($a:ident),*; $($b:ident),*) => {
        $( ($a, $b) );*
    };
}

macro_rules! bad_follow {
    ($a:expr + $b:expr) => {};
}

macro_rules! hygienic {
    () => {
        x
    };
}

fn main() {
    early!();
    unknown!();
    one!(1 2);
    one!(1,);
    one!();
    ambiguous!(a b);
    forever!(1);
    let _ = extra!();
    zip!(a, b; c);
    one!(1 +);
    let x = 1;
    let _ = hygienic!() + x;
}

macro_rules! early {
    () => {};
}

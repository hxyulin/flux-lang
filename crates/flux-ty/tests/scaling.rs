//! Type checking takes time linear in the program: four times as many
//! `impl` blocks, supertraits, associated types, generic calls, statements
//! or errors take about four times as long. Only checking is timed;
//! lowering has its own tests in `flux-hir`.

#[path = "../../flux-syntax/tests/support/scaling.rs"]
mod scaling;

use flux_hir::Hir;
use scaling::{assert_scales, repeat};

/// The program `text`, lowered, which must be clean.
fn lowered(text: String) -> Hir {
    let (_, _, hir) = flux_hir::lower_text("main.rs", &text);
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics.first());
    hir
}

/// Checks `hir`, which must be clean unless `errors`.
fn check(hir: &Hir, errors: bool) {
    let typed = flux_ty::check_file(hir);
    let first = typed.diagnostics.first();
    assert_eq!(!typed.diagnostics.is_empty(), errors, "{first:?}");
}

/// Modules with a struct, an inherent `impl` block, a trait and its
/// `impl` block, and methods called through both and through a bound.
#[test]
fn many_impls() {
    let make = |n| {
        let items = repeat(n, |i| {
            format!(
                "mod m{i} {{\n\
                 struct S(i32);\n\
                 trait Tr {{ fn f(&self) -> i32; }}\n\
                 impl S {{ fn get(&self) -> i32 {{ self.0 }} }}\n\
                 impl Tr for S {{ fn f(&self) -> i32 {{ self.get() + {i} }} }}\n\
                 fn call<T: Tr>(t: &T) -> i32 {{ t.f() }}\n\
                 fn g(s: S) -> i32 {{ s.f() + call(&s) }}\n\
                 }}"
            )
        });
        lowered(format!("{items}fn main() {{}}\n"))
    };
    assert_scales("impl blocks", 300, make, |hir| check(hir, false));
}

/// Traits in one module, each with a method called in a body of its
/// own. Each body once listed the traits in scope, comparing each with
/// those found before it, and each call looked through them all for the
/// method's name; a module's traits are now listed once, and a call
/// looks through those with the name if they are fewer.
#[test]
fn many_traits_in_scope() {
    let make = |n| {
        let items = repeat(n, |i| {
            format!(
                "struct S{i}(i32);\n\
                 trait Tr{i} {{ fn f{i}(&self) -> i32; }}\n\
                 impl Tr{i} for S{i} {{ fn f{i}(&self) -> i32 {{ self.0 }} }}\n\
                 fn g{i}(s: S{i}) -> i32 {{ s.f{i}() }}"
            )
        });
        lowered(format!("{items}fn main() {{}}\n"))
    };
    assert_scales("traits in scope", 200, make, |hir| check(hir, false));
}

/// `impl` blocks of one trait, for different structs, and calls that
/// need them. Every block is compared with every earlier one for
/// overlap, and every goal looks through all of them, where rustc finds
/// those for a type by its outermost constructor.
#[test]
#[ignore = "impl blocks of a trait are compared and searched pairwise: quadratic"]
fn many_impls_of_one_trait() {
    let make = |n| {
        let items = repeat(n, |i| {
            format!(
                "struct S{i}(i32);\n\
                 impl Tr for S{i} {{ fn f(&self) -> i32 {{ self.0 + {i} }} }}\n\
                 fn g{i}(s: S{i}) -> i32 {{ s.f() + call(&s) }}"
            )
        });
        lowered(format!(
            "trait Tr {{ fn f(&self) -> i32; }}\n\
             fn call<T: Tr>(t: &T) -> i32 {{ t.f() }}\n{items}fn main() {{}}\n"
        ))
    };
    assert_scales("impl blocks of one trait", 200, make, |hir| {
        check(hir, false)
    });
}

/// A trait with `n` supertraits, and a few functions bounded by it that
/// call methods of its supertraits: elaborating a bound once compared
/// each supertrait with all before it.
#[test]
fn many_supertraits() {
    let make = |n| {
        let traits = repeat(n, |i| {
            format!("trait T{i}: Base {{ fn m{i}(&self) -> i32; }}")
        });
        let impls = repeat(n, |i| {
            format!("impl T{i} for S {{ fn m{i}(&self) -> i32 {{ {i} }} }}")
        });
        let supertraits: Vec<String> = (0..n).map(|i| format!("T{i}")).collect();
        let fns = repeat(8, |k| {
            format!(
                "fn f{k}<X: All>(x: &X) -> i32 {{ x.base() + x.m{}() }}",
                k * (n - 1) / 7
            )
        });
        lowered(format!(
            "trait Base {{ fn base(&self) -> i32; }}\n{traits}\
             trait All: {} {{}}\n\
             struct S;\n\
             impl Base for S {{ fn base(&self) -> i32 {{ 0 }} }}\n{impls}\
             impl All for S {{}}\n{fns}\
             fn main() {{ f0(&S); }}\n",
            supertraits.join(" + "),
        ))
    };
    assert_scales("supertraits", 300, make, |hir| check(hir, false));
}

/// A trait with `n` associated types bounded by the trait itself, as
/// ndarray's `Dimension` is, and a function naming each: adding their
/// bounds to every function's, four levels deep, once took time in the
/// fourth power of `n`.
#[test]
fn many_bounded_associated_types() {
    let make = |n| {
        let types = repeat(n, |i| format!("    type A{i}: Dim;"));
        let defs = repeat(n, |i| format!("    type A{i} = S;"));
        let fns = repeat(n, |i| {
            format!("fn f{i}<D: Dim>(d: &D, a: &D::A{i}) -> i32 {{ d.ndim() + a.ndim() }}")
        });
        lowered(format!(
            "trait Dim {{\n{types}    fn ndim(&self) -> i32;\n}}\n\
             struct S;\n\
             impl Dim for S {{\n{defs}    fn ndim(&self) -> i32 {{ 1 }}\n}}\n\
             {fns}fn main() {{}}\n"
        ))
    };
    assert_scales("bounded associated types", 100, make, |hir| {
        check(hir, false)
    });
}

/// Generic functions that call each other, each with bounds.
#[test]
fn many_generic_calls() {
    let make = |n| {
        let fns = repeat(n, |i| {
            format!(
                "fn f{}<T: Copy + Tr>(t: T) -> i32 {{ f{i}(t) + t.get() }}",
                i + 1
            )
        });
        lowered(format!(
            "trait Tr {{ fn get(&self) -> i32; }}\n\
             impl Tr for i32 {{ fn get(&self) -> i32 {{ *self }} }}\n\
             fn f0<T: Copy + Tr>(t: T) -> i32 {{ t.get() }}\n{fns}\
             fn main() {{ f{n}(1); }}\n"
        ))
    };
    assert_scales("generic calls", 300, make, |hir| check(hir, false));
}

/// Each statement names the binding just before it.
#[test]
fn long_body() {
    let make = |n| {
        let body = repeat(n, |i| {
            format!(
                "    let x{} = if x{i} > {i} {{ x{i} - 1 }} else {{ x{i} + 1 }};",
                i + 1
            )
        });
        lowered(format!(
            "fn f(x0: i32) -> i32 {{\n{body}    x{n}\n}}\nfn main() {{}}\n"
        ))
    };
    assert_scales("statements in a body", 1000, make, |hir| check(hir, false));
}

/// `impl` blocks of a trait for types that do not exist, and functions
/// returning the wrong type: an error each. Resolving the type of each
/// block once gave a file to every error so far.
#[test]
fn many_errors() {
    let make = |n| {
        let items = repeat(n, |i| {
            format!(
                "impl Tr for Missing{i} {{}}\n\
                 fn f{i}(x: i32) -> bool {{ x }}"
            )
        });
        let text = format!("trait Tr {{}}\n{items}fn main() {{}}\n");
        flux_hir::lower_text("main.rs", &text).2
    };
    assert_scales("errors", 1000, make, |hir| check(hir, true));
}

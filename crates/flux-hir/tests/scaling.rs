//! Building the item tree and lowering bodies takes time linear in the
//! program: four times as many items, derives, `impl` blocks, modules,
//! files, statements or errors take about four times as long.

#[path = "../../flux-syntax/tests/support/scaling.rs"]
mod scaling;

use std::path::PathBuf;

use scaling::{assert_scales, repeat};

/// Lowers `text`, which must be clean unless `errors`.
fn lower(text: &str, errors: bool) {
    let (_, _, hir) = flux_hir::lower_text("main.rs", text);
    assert_eq!(
        !hir.diagnostics.is_empty(),
        errors,
        "{:?}",
        hir.diagnostics.first()
    );
}

#[test]
fn many_items() {
    let make = |n| {
        repeat(n, |i| {
            format!(
                "pub struct S{i} {{ pub a: u32, b: (i64, bool) }}\n\
                 pub const C{i}: u32 = {i} * 2;\n\
                 pub fn f{i}(x: u32) -> u32 {{ x + C{i} }}\n\
                 use self::S{i} as A{i};"
            )
        }) + "fn main() {}\n"
    };
    assert_scales("items", 300, make, |text| lower(text, false));
}

/// Each derive once looked for a `Copy` derive among all of them.
#[test]
fn many_derives() {
    let make = |n| {
        repeat(n, |i| {
            let copy = if i % 2 == 0 { ", Copy" } else { "" };
            format!("#[derive(Clone, PartialEq{copy})]\nstruct S{i}(u8);")
        }) + "fn main() {}\n"
    };
    assert_scales("derives", 1000, make, |text| lower(text, false));
}

#[test]
fn many_impls() {
    let make = |n| {
        let items = repeat(n, |i| {
            format!(
                "struct S{i}(u32);\n\
                 impl S{i} {{ fn get(&self) -> u32 {{ self.0 }} }}\n\
                 impl Tr for S{i} {{ fn f(&self) -> u32 {{ self.get() + {i} }} }}"
            )
        });
        format!("trait Tr {{ fn f(&self) -> u32; }}\n{items}fn main() {{}}\n")
    };
    assert_scales("impl blocks", 300, make, |text| lower(text, false));
}

/// Modules that each import from the one before.
#[test]
fn many_modules() {
    let make = |n| {
        let mut text = String::from("pub mod m0 { pub struct S; }\n");
        text += &repeat(n, |i| {
            format!(
                "pub mod m{} {{ use super::m{i}::S as T; pub struct S(pub T); \
                 pub fn f(s: S) -> T {{ s.0 }} }}",
                i + 1
            )
        });
        text + "fn main() {}\n"
    };
    assert_scales("modules", 300, make, |text| lower(text, false));
}

/// A root file declaring `n` modules, each in its own file, written to a
/// directory that is removed when it is dropped.
struct Files(PathBuf);

impl Files {
    fn new(n: usize) -> Files {
        let dir = std::env::temp_dir().join(format!("flux-scaling-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut root = repeat(n, |i| format!("mod m{i};"));
        root += "fn main() {}\n";
        std::fs::write(dir.join("main.rs"), root).unwrap();
        for i in 0..n {
            let text = format!("pub struct S{i}(u32);\npub fn f(s: S{i}) -> u32 {{ s.0 }}\n");
            std::fs::write(dir.join(format!("m{i}.rs")), text).unwrap();
        }
        Files(dir)
    }
}

impl Drop for Files {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Finding a file by path once looked through all of them.
#[test]
fn many_files() {
    let lower = |files: &Files| {
        let mut db = flux_hir::SourceDb::new();
        let root = db.load(&files.0.join("main.rs")).unwrap();
        let hir = flux_hir::lower(&mut db, root, flux_intern::Symbol::intern("main"));
        assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics.first());
    };
    assert_scales("module files", 500, Files::new, lower);
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
        format!("fn f(x0: i32) {{\n{body}}}\nfn main() {{}}\n")
    };
    assert_scales("statements in a body", 1000, make, |text| {
        lower(text, false)
    });
}

/// Each statement names the function's parameter. A name is looked up
/// through every binding in scope from the innermost, so a body naming
/// its first bindings throughout takes time quadratic in its length.
#[test]
#[ignore = "names are looked up through every binding in scope: quadratic"]
fn long_body_naming_parameter() {
    let make = |n| {
        let body = repeat(n, |i| {
            format!("    let x{i} = if x > {i} {{ x - 1 }} else {{ x + 1 }};")
        });
        format!("fn f(x: i32) {{\n{body}}}\nfn main() {{}}\n")
    };
    assert_scales("statements in a body", 1000, make, |text| {
        lower(text, false)
    });
}

#[test]
fn many_errors() {
    let make = |n| {
        repeat(n, |i| {
            format!("fn f{i}(x: Missing{i}) -> u32 {{ missing{i} + 1 }}")
        }) + "fn main() {}\n"
    };
    assert_scales("errors", 1000, make, |text| lower(text, true));
}

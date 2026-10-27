//! Opaque lifetime capture follows the edition of its `impl` token.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;
use flux_hir::{SourceCrate, SourceDb};
use flux_intern::Symbol;
use flux_syntax::Edition;

fn check(hir: &flux_hir::Hir) -> Vec<flux_hir::Diagnostic> {
    let mut errors = hir.diagnostics.clone();
    if errors.is_empty() {
        let typed = flux_mir::check_file(hir);
        errors.extend(typed.diagnostics.clone());
        if errors.is_empty() {
            errors.extend(flux_mir::build_file(hir, &typed).diagnostics);
        }
    }
    errors
}

#[test]
fn implicit_and_precise_captures_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-pre2024-rpit-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "hidden_ref",
            "fn f<'a>(x:&'a u32)->impl Sized{x}fn main(){}",
            [false, false, true],
        ),
        (
            "hidden_ref_bound",
            "fn f<'a>(x:&'a u32)->impl Sized+'a{x}fn main(){}",
            [true, true, true],
        ),
        (
            "iterator_item",
            "fn f<'a>(x:&'a[u32])->impl Iterator<Item=&'a u32>{x.iter()}fn main(){}",
            [true, true, true],
        ),
        (
            "receiver_independence",
            "struct S<'a>{xs:&'a[u32]}impl<'a>S<'a>{fn iter(&self)->impl Iterator<Item=&'a u32>{self.xs.iter()}}fn main(){let data=[1];let s=S{xs:&data};let mut it=s.iter();drop(s);assert_eq!(it.next(),Some(&1));}",
            [true, true, false],
        ),
        (
            "unused_receiver",
            "struct S(u32);impl S{fn value(&self)->impl Sized{0u32}}fn main(){let s=S(1);let r=s.value();drop(s);drop(r);}",
            [true, true, false],
        ),
        (
            "named_receiver",
            "struct S(u32);impl S{fn f<'a>(&'a self)->impl Sized{0u32}}fn main(){let s=S(1);let r=s.f();drop(s);drop(r);}",
            [true, true, false],
        ),
        (
            "callback_short_receiver",
            "struct S(u32);impl S{fn cb<'a>(&self,x:&'a u32)->impl Fn()->u32+'a{move||*x}}fn main(){let x=7;let cb;{let s=S(1);cb=s.cb(&x);}assert_eq!(cb(),7);}",
            [true, true, true],
        ),
        (
            "callback_elided",
            "fn cb(x:&u32)->impl Fn()->u32+'_ {move||*x}fn main(){let x=7;assert_eq!(cb(&x)(),7);}",
            [true, true, true],
        ),
        (
            "two_captures",
            "fn cb<'a,'b>(x:&'a u32,y:&'b u32)->impl Fn()->u32+'a where 'b:'a {move||*x+*y}fn main(){}",
            [true, true, true],
        ),
        (
            "nested_opaque",
            "fn f<'a>(x:&'a u32)->Option<impl Fn()->u32+'a>{Some(move||*x)}fn main(){}",
            [true, true, true],
        ),
        (
            "generic_captured",
            "fn f<T>(x:T)->impl Sized{x}fn main(){let r;{let x=1;r=f(&x);}drop(r);}",
            [false, false, false],
        ),
        (
            "unused_generic_captured",
            "fn f<T>(_:T)->impl Sized{0u32}fn main(){let r;{let x=1;r=f(&x);}drop(r);}",
            [false, false, false],
        ),
        (
            "generic_bound",
            "fn f<'a,T:'a>(x:T)->impl Sized+'a{x}fn main(){}",
            [true, true, true],
        ),
        (
            "missing_generic_bound",
            "fn f<'a,T>(x:T)->impl Sized+'a{x}fn main(){}",
            [false, false, false],
        ),
        (
            "precise_capture",
            "fn f<'a>(x:&'a u32)->impl Sized+use<'a>{x}fn main(){}",
            [true, true, true],
        ),
        (
            "precise_empty",
            "fn f<'a>(_:&'a u32)->impl Sized+use<>{0u32}fn main(){let r;{let x=1;r=f(&x);}drop(r);}",
            [true, true, true],
        ),
        (
            "precise_wrong",
            "fn f<'a>(x:&'a u32)->impl Sized+use<>{x}fn main(){}",
            [false, false, false],
        ),
        (
            "precise_generic",
            "fn f<'a,T>(x:T,_:&'a u32)->impl Sized+use<T>{x}fn main(){}",
            [true, true, true],
        ),
        (
            "precise_omitted_type",
            "fn f<T>(x:T)->impl Sized+use<>{x}fn main(){}",
            [false, false, false],
        ),
        (
            "item_escape",
            "fn f<'a>(x:&'a[u32])->impl Iterator<Item=&'a u32>{x.iter()}fn main(){let mut r;{let x=[1];r=f(&x);}println!(\"{}\",r.next().unwrap());}",
            [false, false, false],
        ),
        (
            "bound_escape",
            "fn f<'a>(x:&'a u32)->impl Sized+'a{x}fn main(){let r;{let x=1;r=f(&x);}drop(r);}",
            [false, false, false],
        ),
        (
            "bound_wrong_input",
            "fn f<'a,'b>(x:&'b u32,_:&'a u32)->impl Sized+'a{x}fn main(){}",
            [false, false, false],
        ),
        (
            "const_generic",
            "fn f<const N:usize>()->impl Sized{[0u8;N]}fn main(){let _=f::<4>();}",
            [true, true, true],
        ),
        (
            "const_precise",
            "fn f<const N:usize>()->impl Sized+use<N>{[0u8;N]}fn main(){}",
            [true, true, true],
        ),
        (
            "precise_item_missing",
            "fn f<'a>(x:&'a[u32])->impl Iterator<Item=&'a u32>+use<>{x.iter()}fn main(){}",
            [false, false, false],
        ),
        (
            "static_bound_short",
            "fn f<'a>(x:&'a u32)->impl Sized+'static{x}fn main(){}",
            [false, false, false],
        ),
        (
            "item_elision",
            "fn f(x:&[u32])->impl Iterator<Item=&u32>{x.iter()}fn main(){}",
            [true, true, true],
        ),
        (
            "item_alias",
            "type R<'a>=&'a u32;fn f<'a>(x:&'a[u32])->impl Iterator<Item=R<'a>>{x.iter()}fn main(){}",
            [true, true, true],
        ),
        (
            "mutable_item",
            "fn f<'a>(x:&'a mut[u32])->impl Iterator<Item=&'a mut u32>{x.iter_mut()}fn main(){}",
            [true, true, true],
        ),
        (
            "mutable_escape",
            "fn f<'a>(x:&'a mut[u32])->impl Iterator<Item=&'a mut u32>{x.iter_mut()}fn main(){let mut it;{let mut x=[1];it=f(&mut x);}*it.next().unwrap()=2;}",
            [false, false, false],
        ),
        (
            "fn_bound_pointer_binder",
            "fn f<'a>(x:&'a u32)->impl Fn(for<'b> fn(&'b u32))->u32{move|_|*x}fn main(){}",
            [false, false, true],
        ),
        (
            "fn_bound_pointer_explicit",
            "fn f<'a>(x:&'a u32)->impl Fn(for<'b> fn(&'b u32))->u32+'a{move|_|*x}fn main(){}",
            [true, true, true],
        ),
        (
            "fn_bound_inputs_not_capture",
            "fn f<'a>(x:&'a u32)->impl Fn(&u32)->u32{move|y|*x+*y}fn main(){}",
            [false, false, true],
        ),
        (
            "fn_bound_named_input",
            "fn f<'a>(x:&'a u32)->impl Fn(&'a u32)->u32{move|y|*x+*y}fn main(){}",
            [true, true, true],
        ),
    ] {
        if !case_filter::selected(name) {
            continue;
        }
        for ((edition, text), expected) in [
            (Edition::E2015, "2015"),
            (Edition::E2018, "2018"),
            (Edition::E2021, "2021"),
            (Edition::E2024, "2024"),
        ]
        .into_iter()
        .zip([accepted[0], accepted[0], accepted[1], accepted[2]])
        {
            let path = root.join(format!("{name}.rs"));
            std::fs::write(&path, source).unwrap();
            let oracle = std::process::Command::new("rustc")
                .args(["--emit=metadata", "--edition", text])
                .arg(&path)
                .arg("-o")
                .arg(root.join("oracle.rmeta"))
                .output()
                .unwrap();
            assert_eq!(
                oracle.status.success(),
                expected,
                "{name}/{text}: {}",
                String::from_utf8_lossy(&oracle.stderr)
            );
            let mut db = SourceDb::new();
            let file = db.add_with_edition(&path, source, edition);
            let hir = flux_hir::lower(&mut db, file, Symbol::intern("probe"));
            let errors = check(&hir);
            assert_eq!(errors.is_empty(), expected, "{name}/{text}: {errors:?}");
        }
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn macro_definition_edition_controls_implicit_captures() {
    let root = std::env::temp_dir().join(format!("flux-rpit-macro-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let source = "#[macro_export] macro_rules! make {()=>{pub fn f(x:&u32)->impl Sized{x}}}";
    let consumer = "extern crate legacy;legacy::make!();fn main(){}";
    for (definition, def_text) in [
        (Edition::E2015, "2015"),
        (Edition::E2018, "2018"),
        (Edition::E2021, "2021"),
        (Edition::E2024, "2024"),
    ] {
        let library = root.join("legacy.rs");
        std::fs::write(&library, source).unwrap();
        let rlib = root.join("liblegacy.rlib");
        let oracle = std::process::Command::new("rustc")
            .args([
                "--crate-name=legacy",
                "--crate-type=rlib",
                "--edition",
                def_text,
            ])
            .arg(&library)
            .arg("-o")
            .arg(&rlib)
            .output()
            .unwrap();
        assert!(
            oracle.status.success(),
            "{}",
            String::from_utf8_lossy(&oracle.stderr)
        );
        for (invocation, call_text) in [
            (Edition::E2015, "2015"),
            (Edition::E2018, "2018"),
            (Edition::E2021, "2021"),
            (Edition::E2024, "2024"),
        ] {
            let path = root.join("consumer.rs");
            std::fs::write(&path, consumer).unwrap();
            let oracle = std::process::Command::new("rustc")
                .args(["--emit=metadata", "--edition", call_text, "--extern"])
                .arg(format!("legacy={}", rlib.display()))
                .arg(&path)
                .arg("-o")
                .arg(root.join("consumer.rmeta"))
                .output()
                .unwrap();
            let expected = definition == Edition::E2024;
            assert_eq!(
                oracle.status.success(),
                expected,
                "definition{def_text}/invocation{call_text}"
            );
            let mut db = SourceDb::new();
            let lib = db.add_with_edition(&library, source, definition);
            let main = db.add_with_edition(&path, consumer, invocation);
            let hir = flux_hir::lower_sources(
                &mut db,
                &[
                    SourceCrate {
                        root: lib,
                        name: Symbol::intern("legacy"),
                        dependencies: vec![],
                    },
                    SourceCrate {
                        root: main,
                        name: Symbol::intern("consumer"),
                        dependencies: vec![(Symbol::intern("legacy"), 0)],
                    },
                ],
            );
            let errors = check(&hir);
            assert_eq!(
                errors.is_empty(),
                expected,
                "definition{def_text}/invocation{call_text}: {errors:?}"
            );
        }
    }
    std::fs::remove_dir_all(root).unwrap();
}

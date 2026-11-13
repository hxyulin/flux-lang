//! Bound methods still obey lexical trait scope on projections and opaque types.
#[test]
fn bound_method_scope_matches_rustc() {
    let root = std::env::temp_dir().join(format!("flux-method-trait-scope-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let mut mismatches = Vec::new();
    for (name, source, accepted) in [
        (
            "implied_only",
            "mod tr {pub trait A {fn m(&self)->u32;} pub trait B {fn m(&self)->u32;} pub trait P {type Q:A;}}\nfn f<T:tr::P>(x:T::Q)->u32{x.m()} fn main(){}",
            false,
        ),
        (
            "explicit_only",
            "mod tr {pub trait A {fn m(&self)->u32;} pub trait B {fn m(&self)->u32;} pub trait P {type Q:A;}}\nfn f<T:tr::P>(x:T::Q)->u32 where T::Q:tr::B{x.m()} fn main(){}",
            false,
        ),
        (
            "both_explicit",
            "mod tr {pub trait A {fn m(&self)->u32;} pub trait B {fn m(&self)->u32;} pub trait P {type Q:A;}}\nfn f<T:tr::P>(x:T::Q)->u32 where T::Q:tr::A+tr::B{x.m()} fn main(){}",
            false,
        ),
        (
            "implied_import",
            "mod tr {pub trait A {fn m(&self)->u32;} pub trait B {fn m(&self)->u32;} pub trait P {type Q:A;}}\nuse tr::A; fn f<T:tr::P>(x:T::Q)->u32{x.m()} fn main(){}",
            true,
        ),
        (
            "explicit_with_import",
            "mod tr {pub trait A {fn m(&self)->u32;} pub trait B {fn m(&self)->u32;} pub trait P {type Q:A;}}\nuse tr::A; fn f<T:tr::P>(x:T::Q)->u32 where T::Q:tr::B{x.m()} fn main(){}",
            true,
        ),
        (
            "explicit_with_both_import",
            "mod tr {pub trait A {fn m(&self)->u32;} pub trait B {fn m(&self)->u32;} pub trait P {type Q:A;}}\nuse tr::{A,B}; fn f<T:tr::P>(x:T::Q)->u32 where T::Q:tr::B{x.m()} fn main(){}",
            false,
        ),
        (
            "param_no_import",
            "mod tr {pub trait A {fn m(&self)->u32;} pub trait B {fn m(&self)->u32;} pub trait P {type Q:A;}}\nfn f<T:tr::A>(x:T)->u32{x.m()} fn main(){}",
            true,
        ),
        (
            "param_both_bounds",
            "mod tr {pub trait A {fn m(&self)->u32;} pub trait B {fn m(&self)->u32;} pub trait P {type Q:A;}}\nfn f<T:tr::A+tr::B>(x:T)->u32{x.m()} fn main(){}",
            false,
        ),
        (
            "dynamic_no_import",
            "mod tr {pub trait A {fn m(&self)->u32;} pub trait B {fn m(&self)->u32;} pub trait P {type Q:A;}}\nfn f(x:&dyn tr::A)->u32{x.m()} fn main(){}",
            true,
        ),
        (
            "opaque_no_import",
            "mod tr {pub trait A {fn m(&self)->u32;} pub trait B {fn m(&self)->u32;} pub trait P {type Q:A;}}\nimpl tr::A for u8{fn m(&self)->u32{1}}fn make()->impl tr::A{0u8} fn main(){let _=make().m();}",
            false,
        ),
        (
            "opaque_import",
            "mod tr {pub trait A {fn m(&self)->u32;} pub trait B {fn m(&self)->u32;} pub trait P {type Q:A;}}\nuse tr::A;impl tr::A for u8{fn m(&self)->u32{1}}fn make()->impl tr::A{0u8} fn main(){let _=make().m();}",
            true,
        ),
        (
            "concrete_generic_no_import",
            "mod tr {pub trait A {fn m(&self)->u32;} pub trait B {fn m(&self)->u32;} pub trait P {type Q:A;}}\nstruct S<T>(T);fn f<T>(x:S<T>)->u32 where S<T>:tr::A{x.m()}fn main(){}",
            false,
        ),
        (
            "enclosing_impl",
            "mod tr {pub trait A {fn m(&self)->u32;} pub trait B {fn m(&self)->u32;} pub trait P {type Q:A;}}\nstruct S<T:tr::P>(T::Q);impl<T:tr::P>tr::B for S<T> where T::Q:tr::B{fn m(&self)->u32{self.0.m()}}fn main(){}",
            true,
        ),
        (
            "param_extension",
            "mod tr {pub trait A {fn m(&self)->u32;} pub trait B {fn m(&self)->u32;} pub trait P {type Q:A;}}\ntrait Extra{fn m(&self)->u32{2}}impl<T>Extra for T{}fn f<T:tr::A>(x:T)->u32{x.m()}fn main(){}",
            true,
        ),
    ] {
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let oracle = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("oracle.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            oracle.status.success(),
            accepted,
            "{name}: {}",
            String::from_utf8_lossy(&oracle.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text(&path, source);
        assert!(hir.diagnostics.is_empty(), "{name}: {:?}", hir.diagnostics);
        let typed = flux_ty::check_file(&hir);
        if typed.diagnostics.is_empty() != accepted {
            mismatches.push(format!("{name}: {:?}", typed.diagnostics));
        }
    }
    std::fs::remove_dir_all(root).unwrap();
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}

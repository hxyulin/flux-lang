//! Associated paths prioritize explicit bounds only on declared type parameters.
use flux_hir::{SourceCrate, SourceDb};
use flux_intern::Symbol;
use flux_syntax::Edition;
#[test]
fn parameter_bounds_precede_inferred_impls() {
    let root = std::env::temp_dir().join(format!(
        "flux-parameter-bound-priority-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "bound_path",
            "trait A{fn f(&self)->u8;}trait B{fn f(&self)->u8;}impl<T:A>B for T{fn f(&self)->u8{99}}fn go<T:A>(x:&T)->u8{T::f(x)}fn main(){}",
            true,
        ),
        (
            "bound_path_inside_blanket",
            "trait A{fn f(&self)->u8;}trait B{fn f(&self)->u8;}impl<T:A>B for T{fn f(&self)->u8{T::f(self)}}fn main(){}",
            true,
        ),
        (
            "both_bounds",
            "trait A{fn f(&self)->u8;}trait B{fn f(&self)->u8;}impl<T:A>B for T{fn f(&self)->u8{99}}fn go<T:A+B>(x:&T)->u8{T::f(x)}fn main(){}",
            false,
        ),
        (
            "concrete",
            "trait A{fn f(&self)->u8;}trait B{fn f(&self)->u8;}impl<T:A>B for T{fn f(&self)->u8{99}}struct S;impl A for S{fn f(&self)->u8{7}}fn go(x:&S)->u8{S::f(x)}fn main(){}",
            false,
        ),
        (
            "nominal_generic",
            "trait A{fn f(&self)->u8;}trait B{fn f(&self)->u8;}impl<T:A>B for T{fn f(&self)->u8{99}}struct W<T>(T);fn go<T>(x:&W<T>)->u8 where W<T>:A{W::<T>::f(x)}fn main(){}",
            false,
        ),
        (
            "supertrait",
            "trait A{fn f(&self)->u8;}trait B{fn f(&self)->u8;}impl<T:A>B for T{fn f(&self)->u8{99}}trait Child:A{}fn go<T:Child>(x:&T)->u8{T::f(x)}fn main(){}",
            true,
        ),
        (
            "supertrait_diamond",
            "trait A{fn f(&self)->u8;}trait B{fn f(&self)->u8;}impl<T:A>B for T{fn f(&self)->u8{99}}trait L:A{}trait R:A{}trait Child:L+R{}fn go<T:Child>(x:&T)->u8{T::f(x)}fn main(){}",
            true,
        ),
        (
            "method_syntax",
            "trait A{fn f(&self)->u8;}trait B{fn f(&self)->u8;}impl<T:A>B for T{fn f(&self)->u8{99}}fn go<T:A>(x:&T)->u8{x.f()}fn main(){}",
            true,
        ),
        (
            "method_receiver_modes",
            "trait A{fn f(&mut self)->u8;}trait B{fn f(&self)->u8;}impl<T:A>B for T{fn f(&self)->u8{99}}fn go<T:A>(x:&T)->u8{x.f()}fn main(){}",
            true,
        ),
        (
            "static_function",
            "trait A{fn f()->u8;}trait B{fn f()->u8;}impl<T:A>B for T{fn f()->u8{99}}fn go<T:A>()->u8{T::f()}fn main(){}",
            true,
        ),
        (
            "associated_constant",
            "trait A{const K:u8;}trait B{const K:u8;}impl<T:A>B for T{const K:u8=99;}fn go<T:A>()->u8{T::K}fn main(){}",
            true,
        ),
        (
            "both_constants",
            "trait A{const K:u8;}trait B{const K:u8;}fn go<T:A+B>()->u8{T::K}fn main(){}",
            false,
        ),
        (
            "unknown_self",
            "trait A{fn f()->u8;}trait B{fn f()->u8;}impl<T:A>B for T{fn f()->u8{99}}fn go<T:A>()->u8{<_>::f()}fn main(){}",
            false,
        ),
        (
            "multiple_argument_bounds",
            "trait A<T>{fn f(x:T)->T;}trait B{fn f(x:u16)->u16;}impl<X:A<u8>+A<u16>>B for X{fn f(x:u16)->u16{x}}fn go<X:A<u8>+A<u16>>(x:u16)->u16{X::f(x)}fn main(){}",
            true,
        ),
        (
            "inherent_priority",
            "trait A{fn f(&self)->u8;}trait B{fn f(&self)->u8;}impl<T:A>B for T{fn f(&self)->u8{99}}struct W<T>(T);impl<T>W<T>{fn f(&self)->u8{13}}fn go<T>(x:&W<T>)->u8 where W<T>:A{W::<T>::f(x)}fn main(){}",
            true,
        ),
    ] {
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let output = std::process::Command::new("rustc")
            .args(["--edition=2021", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("r.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            output.status.success(),
            accepted,
            "Rust {name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let mut db = SourceDb::new();
        let file = db.add_with_edition(&path, source, Edition::E2021);
        let hir = flux_hir::lower_sources(
            &mut db,
            &[SourceCrate {
                root: file,
                name: Symbol::intern(name),
                dependencies: vec![],
            }],
        );
        let typed = flux_ty::check_file(&hir);
        let mut errors = hir.diagnostics.clone();
        errors.extend(typed.diagnostics.clone());
        assert_eq!(errors.is_empty(), accepted, "Flux {name}: {errors:?}");
    }
    std::fs::remove_dir_all(root).unwrap();
}

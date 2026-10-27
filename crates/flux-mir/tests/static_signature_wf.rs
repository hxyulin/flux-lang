//! Static ADT outlives contracts are implied by signature types, not by requested bounds.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;

#[test]
fn static_signature_wf_matches_rustc() {
    let root =
        std::env::temp_dir().join(format!("flux-static-signature-wf-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "impl_self",
            "struct U<T:'static>(T);trait M{}impl<T>M for U<T>{}fn main(){}",
            true,
        ),
        (
            "impl_header_argument",
            "struct U<T:'static>(T);trait M<A>{fn known()->bool;}struct S;fn need<T:'static>(){}impl<T>M<U<T>>for S{fn known()->bool{need::<T>();true}}fn main(){}",
            true,
        ),
        (
            "inherent_self",
            "struct U<T:'static>(T);fn need<T:'static>(){}impl<T>U<T>{fn known(&self){need::<T>();}}fn main(){}",
            true,
        ),
        (
            "function_input",
            "struct U<T:'static>(T);fn need<T:'static>(){}fn f<T>(_:U<T>){need::<T>();}fn main(){}",
            true,
        ),
        (
            "function_nested_input",
            "struct U<T:'static>(T);fn need<T:'static>(){}fn f<T>(_:Option<U<T>>){need::<T>();}fn main(){}",
            true,
        ),
        (
            "function_return",
            "struct U<T:'static>(T);fn need<T:'static>(){}fn f<T>()->U<T>{need::<T>();panic!()}fn main(){}",
            true,
        ),
        (
            "constructive_function_return",
            "struct U<T:'static>(T);fn need<T:'static>(){}fn make<T>(x:T)->U<T>{need::<T>();U(x)}fn main(){let _=make(7u32);}",
            true,
        ),
        (
            "borrowed_function_input",
            "struct U<T:'static>(T);fn need<T:'static>(){}fn f<T>(_:&U<T>){need::<T>();}fn main(){}",
            true,
        ),
        (
            "projection_static",
            "trait Endian:Copy+'static{}trait Header{type E:Endian;}struct U<T:Endian>(T);struct F<T:Header>(U<T::E>);trait M{}impl<T:Header>M for F<T>{}fn main(){}",
            true,
        ),
        (
            "concrete_projection_static",
            "trait Endian:Copy+'static{}impl Endian for u32{}trait Header{type E:Endian;}struct U<T:Endian>(T);struct H;impl Header for H{type E=u32;}struct G<T:Header>(U<T::E>);enum F{G(G<H>)}trait M{}impl M for F{}fn main(){}",
            true,
        ),
        (
            "projection_static_derive",
            "trait Endian:Copy+'static{}trait Header{type E:Endian;}struct U<T:Endian>(T);#[derive(Debug)]struct F<T:Header>(core::marker::PhantomData<U<T::E>>);fn main(){}",
            true,
        ),
        (
            "where_argument_requirement",
            "struct U<T:'static>(T);trait M<A>{}struct S;fn need<T:'static>(){}fn f<T>()where S:M<U<T>>{need::<T>();}fn main(){}",
            false,
        ),
        (
            "trait_parameter_requirement",
            "trait M<T:'static>{}fn need<T:'static>(){}fn f<T,S:M<T>>(){need::<T>();}fn main(){}",
            false,
        ),
        (
            "trait_reference_requirement",
            "trait M<'a,T:'a>{}fn f<'a,'b,S:M<'a,&'b u32>>(){}fn main(){}",
            false,
        ),
        (
            "impl_trait_reference_requirement",
            "trait M<'a,T:'a>{}struct S;impl<'a,'b>M<'a,&'b u32>for S{}fn main(){}",
            false,
        ),
        (
            "short_input_call",
            "struct U<T:'static>(T);fn take<T>(_:U<T>){}fn main(){let x=7;take(U(&x));}",
            false,
        ),
        (
            "short_output_call",
            "struct U<T:'static>(T);fn make<T>(x:T)->U<T>{U(x)}fn main(){let x=7;let _=make(&x);}",
            false,
        ),
        (
            "short_impl_call",
            "struct U<T:'static>(T);impl<T>U<T>{fn new(x:T)->Self{U(x)}}fn main(){let x=7;let _=U::new(&x);}",
            false,
        ),
    ] {
        if !case_filter::selected(name) {
            continue;
        }
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
        let typed = flux_mir::check_file(&hir);
        if typed.diagnostics.is_empty() {
            let mir = flux_mir::build_file(&hir, &typed);
            assert_eq!(
                mir.diagnostics.is_empty(),
                accepted,
                "{name}: {:?}",
                mir.diagnostics
            );
        } else {
            assert!(!accepted, "{name}: {:?}", typed.diagnostics);
        }
    }
    std::fs::remove_dir_all(root).unwrap();
}

// This source has no user impls or signature inputs/outputs. Its generated
// constructor already carries the struct's explicit T:'static predicate.
// Discarding the resulting aggregate currently bypasses its formation check.
#[test]
#[ignore = "discarded static-only aggregate does not enforce its formation lifetime"]
fn discarded_static_aggregate_requires_wf() {
    let source = "struct U<T:'static>(T);fn main(){let x=7;let _=U(&x);}";
    let root =
        std::env::temp_dir().join(format!("flux-discarded-static-wf-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("probe.rs");
    std::fs::write(&path, source).unwrap();
    let oracle = std::process::Command::new("rustc")
        .args(["--edition=2024", "--emit=metadata"])
        .arg(&path)
        .arg("-o")
        .arg(root.join("oracle.rmeta"))
        .output()
        .unwrap();
    assert!(!oracle.status.success());
    let (_, file, hir) = flux_hir::lower_text(&path, source);
    let typed = flux_mir::check_file(&hir);
    let constructor = hir
        .items
        .functions
        .iter()
        .find(|(_, f)| f.ctor.is_some() && f.loc.file == file)
        .unwrap();
    assert!(!constructor.1.generics.type_outlives.is_empty());
    assert!(typed.sigs.regions.fns[constructor.0].has_bounds);
    let mir = flux_mir::build_file(&hir, &typed);
    std::fs::remove_dir_all(root).unwrap();
    assert!(
        !mir.diagnostics.is_empty(),
        "discarded aggregate was accepted"
    );
}

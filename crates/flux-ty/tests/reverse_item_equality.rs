//! Reverse projection equalities need independent, unique, region-free answers.
#[test]
fn independent_projection_answers_match_rustc() {
    for (name, text, accepted) in [
        (
            "positive",
            "fn f<I:Iterator<Item=char>,T:IntoIterator<IntoIter=I>>(t:T)->I{t.into_iter()}",
            true,
        ),
        (
            "competing",
            "trait View{type Item;}trait Source{type X;type A:View<Item=Self::X>;type B:View<Item=Self::X>;}fn f<T:Source<A=I,B=J>,I:View<Item=u8>,J:View<Item=u16>>(x:T::X)->u8{x}",
            false,
        ),
        (
            "cycle",
            "trait Link{type Out;}trait Cycle{type A:Link<Out=Self::B>;type B:Link<Out=Self::A>;}fn f<T:Cycle>(x:T::A)->u8{x}",
            false,
        ),
        (
            "direct_priority",
            "trait View{type Item;}trait Source{type X;type A:View<Item=Self::X>;}fn f<T:Source<X=u16,A=I>,I:View<Item=u16>>(x:T::X)->u16{x}",
            true,
        ),
        (
            "direct_conflict",
            "trait View{type Item;}trait Source{type X;type A:View<Item=Self::X>;}fn f<T:Source<X=u16,A=I>,I:View<Item=u8>>(x:T::X)->u8{x}",
            false,
        ),
        (
            "gat_family",
            "trait View{type Item<T>;}trait Source{type X;type A:View<Item<u8>=Self::X>;}fn f<T:Source<A=I>,I:View<Item<u16>=u8>>(x:T::X)->u8{x}",
            false,
        ),
        (
            "reference_lifetime",
            "trait View{type Item;}trait Source{type X;type A:View<Item=Self::X>;}fn f<'a,T:Source<A=I>,I:View<Item=&'a u8>>(x:T::X)->&'static u8{x}",
            false,
        ),
    ] {
        let source = format!("{text}\nfn main(){{}}\n");
        let root =
            std::env::temp_dir().join(format!("flux-reverse-item-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("case.rs");
        std::fs::write(&path, &source).unwrap();
        let reference = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata", "--out-dir"])
            .arg(&root)
            .arg(&path)
            .output()
            .unwrap();
        assert_eq!(
            reference.status.success(),
            accepted,
            "rustc {name}: {}",
            String::from_utf8_lossy(&reference.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text(&path, &source);
        assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
        let typed = flux_ty::check_file(&hir);
        assert_eq!(
            typed.diagnostics.is_empty(),
            accepted,
            "flux {name}: {:?}",
            typed.diagnostics
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}

//! Projection occurrences retain the lifetime arguments of their written bounds.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;

#[test]
fn trait_projection_owners_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-trait-projection-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, body, accepts) in [
        (
            "impl_associated_value",
            "struct W<S>(S); impl<'a,S:Source<'a>> Source<'a> for W<S>{type Out=S::Out;fn get(&self)->Self::Out{self.0.get()}}",
            true,
        ),
        (
            "impl_trait_argument",
            "trait Sink<T>{} struct W<S>(S); impl<'a,S:Source<'a>> Sink<S::Out> for W<S>{}",
            true,
        ),
        (
            "impl_self_type",
            "struct W<T>(T); trait Sink<T>{} impl<'a,S:Source<'a>> Sink<S> for W<S::Out>{}",
            true,
        ),
        (
            "predicate_subject",
            "fn f<'a,S:Source<'a>>() where S::Out:Copy{}",
            true,
        ),
        (
            "predicate_argument",
            "trait Sink<T>{} fn f<'a,S:Source<'a>,T:Sink<S::Out>>(){}",
            true,
        ),
        (
            "explicit_projection",
            "struct W<S>(S); impl<'a,S:Source<'a>> Source<'a> for W<S>{type Out=<S as Source<'a>>::Out;fn get(&self)->Self::Out{self.0.get()}}",
            true,
        ),
        (
            "outlives_does_not_extend_owner",
            "fn f<'a,S:Source<'a>>(x:&'a u32)->&'static u32 where S::Out:'static{x}",
            false,
        ),
        (
            "borrowed_projection_does_not_extend_owner",
            "fn f<'a,'b,S:Source<'a>>(x:&'a u32,_:&'b S::Out)->&'b u32{x}",
            false,
        ),
        (
            "nested_projection_does_not_extend_owner",
            "fn f<'a,'b,S:Source<'a>>(x:&'a u32,_:&'b (S::Out,))->&'b u32{x}",
            false,
        ),
        (
            "nominal_projection_does_not_extend_owner",
            "struct Storage<'b,T:'b>(&'b T);fn f<'a,'b,S:Source<'a>>(x:&'a u32,_:Storage<'b,S::Out>)->&'b u32{x}",
            false,
        ),
        (
            "projection_outlives_unsupported",
            "fn f<'a,S:Source<'a>>() where S::Out:'static{}",
            true,
        ),
        (
            "qualified_projection_outlives_unsupported",
            "fn f<'a,S:Source<'a>>() where <S as Source<'a>>::Out:'static{}",
            true,
        ),
        (
            "qualified_outlives_does_not_extend_owner",
            "fn f<'a,S:Source<'a>>(x:&'a u32)->&'static u32 where <S as Source<'a>>::Out:'static{x}",
            false,
        ),
        (
            "caller_must_prove_output_outlives",
            "fn f<'a,S:Source<'a>>(_: &'a (),s:&S)->S::Out where S::Out:'static{s.get()}fn g(){let n=7;let _=f(&(),&Borrow(&n));}",
            false,
        ),
        (
            "repeated_same_owner",
            "struct W<S>(S); impl<'a,S:Source<'a>> Source<'a> for W<S>{type Out=(S::Out,S::Out);fn get(&self)->Self::Out{(self.0.get(),self.0.get())}}",
            true,
        ),
        (
            "distinct_source_owner",
            "struct W<'a,S>(S,core::marker::PhantomData<&'a ()>);impl<'a,'b,S:Source<'a>> Source<'b> for W<'a,S>{type Out=S::Out;fn get(&self)->Self::Out{self.0.get()}}",
            true,
        ),
        (
            "static_owner",
            "struct W<S>(S); impl<S:Source<'static>> Source<'static> for W<S>{type Out=S::Out;fn get(&self)->Self::Out{self.0.get()}}",
            true,
        ),
        (
            "distinct_explicit_owners",
            "struct W<A,B>(A,B);trait Pair<X,Y>{}impl<'a,'b,A:Source<'a>,B:Source<'b>> Pair<<A as Source<'a>>::Out,<B as Source<'b>>::Out> for W<A,B>{}",
            true,
        ),
        (
            "borrow_cannot_escape",
            "struct W<S>(S);impl<'a,S:Source<'a>> Source<'a> for W<S>{type Out=S::Out;fn get(&self)->Self::Out{self.0.get()}}fn f<'a>(w:&W<Borrow<'a>>)->&'static u32{w.get()}",
            false,
        ),
        (
            "owner_cannot_change",
            "struct W<S>(S);impl<'a,S:Source<'a>> Source<'a> for W<S>{type Out=S::Out;fn get(&self)->Self::Out{self.0.get()}}fn f<'a,'b>(w:&W<Borrow<'a>>)->&'b u32{w.get()}",
            false,
        ),
        (
            "wrong_selected_output",
            "struct W<S>(S);impl<'a,S:Source<'a>> Source<'a> for W<S>{type Out=&'static u32;fn get(&self)->Self::Out{self.0.get()}}",
            false,
        ),
        (
            "returned_projection",
            "fn f<'a,S:Source<'a>>(s:&S)->S::Out{s.get()}",
            true,
        ),
    ] {
        if !case_filter::selected(name) {
            continue;
        }
        let source = format!(
            "trait Source<'a>{{type Out;fn get(&self)->Self::Out;}} struct Borrow<'a>(&'a u32);impl<'a> Source<'a> for Borrow<'a>{{type Out=&'a u32;fn get(&self)->Self::Out{{self.0}}}} {body} fn main(){{}}"
        );
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, &source).unwrap();
        let oracle = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("probe.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            oracle.status.success(),
            accepts,
            "rustc {name}: {}",
            String::from_utf8_lossy(&oracle.stderr)
        );
        let (db, _, hir) = flux_hir::lower_text(&path, &source);
        assert!(
            db.syntax_errors().is_empty(),
            "{name}: {:?}",
            db.syntax_errors()
        );
        let mut errors = hir.diagnostics.clone();
        if errors.is_empty() {
            let typed = flux_mir::check_file(&hir);
            errors.extend(typed.diagnostics.iter().cloned());
            if errors.is_empty() {
                errors.extend(flux_mir::build_file(&hir, &typed).diagnostics);
            }
        }
        if name.ends_with("_unsupported") {
            assert!(errors.iter().any(|d| d.message == "type-outlives bounds on lifetime-bearing projections are not supported yet"),"Flux {name}: {errors:?}");
        } else {
            assert_eq!(errors.is_empty(), accepts, "Flux {name}: {errors:?}");
        }
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn missing_or_wrong_projection_owner_metadata_is_rejected() {
    use flux_ty::regions::Region;
    let source = "trait Source<'a>{type Out;fn get(&self)->Self::Out;} struct W<'a,S>(S,core::marker::PhantomData<&'a ()>);impl<'a,'b,S:Source<'a>> Source<'b> for W<'a,S>{type Out=S::Out;fn get(&self)->Self::Out{self.0.get()}} fn main(){}";
    let (_, _, hir) = flux_hir::lower_text("projection_metadata.rs", source);
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    assert!(flux_mir::build_file(&hir, &typed).diagnostics.is_empty());
    let (imp, _) = hir
        .items
        .impls
        .iter()
        .find(|(_, data)| {
            data.file
                == hir
                    .items
                    .functions
                    .iter()
                    .find(|(_, f)| f.name.as_str() == "main")
                    .unwrap()
                    .1
                    .loc
                    .file
        })
        .unwrap();
    for corruption in 0..4 {
        let mut typed = flux_mir::check_file(&hir);
        let block = typed.sigs.regions.impls.get_mut(imp).unwrap();
        let selections = &mut block.type_selections[0].1;
        match corruption {
            0 => *selections = Box::default(),
            1 => selections[0].lifetimes = Box::default(),
            2 => selections[0].lifetimes[0] = Region::Erased,
            _ => selections[0].lifetimes[0] = Region::Param(1),
        }
        let mir = flux_mir::build_file(&hir, &typed);
        assert!(
            !mir.diagnostics.is_empty(),
            "corruption {corruption} was accepted"
        );
    }
}

//! Candidate discovery shares type evidence but never proves lifetime obligations.
use std::process::Command;

#[test]
fn contextual_candidates_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-rigid-solver-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "borrowed_callback_closed_output",
            "trait Once<A>{type Out;fn once(self,a:A)->Self::Out;}trait Mut<A>:Once<A>{fn call(&mut self,a:A)->Self::Out;}struct Inspect<F>(F);impl<'a,F,T>Once<&'a T>for Inspect<F>where F:FnOnce(&'a T){type Out=();fn once(self,a:&'a T)->Self::Out{(self.0)(a)}}impl<'a,F,T>Mut<&'a T>for Inspect<F>where F:FnMut(&'a T){fn call(&mut self,a:&'a T)->Self::Out{(self.0)(a)}}",
            true,
        ),
        (
            "borrowed_callback_static_header",
            "trait Once<A>{type Out;}trait Mut<A>:Once<A>{fn call(&mut self,a:A)->Self::Out;}struct Inspect;impl Once<&'static u32>for Inspect{type Out=();}impl<'a>Mut<&'a u32>for Inspect{fn call(&mut self,_:&'a u32)->Self::Out{}}",
            false,
        ),
        (
            "borrowed_callback_static_requirement",
            "trait Once<A>{type Out;}trait Mut<A>:Once<A>{fn call(&mut self,a:A)->Self::Out;}struct Inspect<F>(F);impl<'a,F:FnOnce(&'a u32)+'static>Once<&'a u32>for Inspect<F>{type Out=();}impl<'a,F:FnMut(&'a u32)>Mut<&'a u32>for Inspect<F>{fn call(&mut self,a:&'a u32)->Self::Out{(self.0)(a)}}",
            false,
        ),
        (
            "ranked_callback_supertrait",
            "trait Call<A>{type Out;}impl<F:FnMut(A)->R,A,R>Call<A>for F{type Out=R;}trait Source{type Item;}trait Fused:Source{}struct Filter<S,R,F>(S,R,F);impl<S:Source,R,F>Source for Filter<S,R,F>where F:for<'a>Call<&'a S::Item,Out=R>{type Item=S::Item;}impl<S:Fused,R,F>Fused for Filter<S,R,F>where F:FnMut(&S::Item)->R{}",
            true,
        ),
        (
            "local_supertrait_output_in_self_header",
            "trait Parent{type Item;}trait Child:Parent{}trait Mark{}struct Pair<T,U>(T,U);impl<T:Child>Mark for Pair<T,T::Item>{}fn need<T:Mark>(_:T){}fn check<T:Child<Item=u32>>(t:T){need(Pair(t,7u32));}",
            true,
        ),
        (
            "inherited_output_in_self_header",
            "trait Mark{}struct Pair<I,T>(I,T);impl<I:DoubleEndedIterator>Mark for Pair<I,I::Item>{}fn need<T:Mark>(_:T){}fn check<I:DoubleEndedIterator<Item=u32>>(i:I){need(Pair(i,7u32));}",
            true,
        ),
        (
            "object_output_signature",
            "trait Child:Iterator{fn get(&mut self)->Option<Self::Item>;}impl Child for dyn Iterator<Item=u32>{fn get(&mut self)->Option<u32>{self.next()}}",
            true,
        ),
        (
            "recursive_header_projection",
            "trait Tr{type Out;}struct W<T,U>(T,U);impl<T>Tr for W<T,<W<T,u32>as Tr>::Out>{type Out=u32;}",
            false,
        ),
        (
            "projection_in_self_header",
            "trait Source{type Out;}struct Pair<T,U>(T,U);trait Base{}impl<T:Source>Base for Pair<T,T::Out>{}fn need<T:Base>(_:T){}fn f<T:Source<Out=u32>>(t:T){need(Pair(t,7u32));}",
            true,
        ),
        (
            "wrong_projection_in_self_header",
            "trait Source{type Out;}struct Pair<T,U>(T,U);trait Base{}impl<T:Source>Base for Pair<T,T::Out>{}fn need<T:Base>(_:T){}fn f<T:Source<Out=u32>>(t:T){need(Pair(t,7u64));}",
            false,
        ),
        (
            "abstract_output_parameter",
            "trait Source{type Out;}trait Base{}struct W<T>(T);impl<R,T:Source<Out=R>>Base for W<T>{}trait Child:Base{}impl<T:Source>Child for W<T>{}",
            true,
        ),
        (
            "abstract_output_missing_owner",
            "trait Source{type Out;}trait Base{}struct W<T>(T);impl<R,T:Source<Out=R>>Base for W<T>{}trait Child:Base{}impl<T>Child for W<T>{}",
            false,
        ),
        (
            "unsized_source",
            "trait Source{type Out;}trait Base{type Out;}impl<R,T:Source<Out=R>+?Sized>Base for T{type Out=R;}trait Child:Base{}impl<T:Source<Out=u32>+?Sized>Child for T{}",
            true,
        ),
        (
            "unsized_output",
            "trait Source{type Out:?Sized;}trait Tag{type Out;}trait Base{}impl<R:?Sized+Tag<Out=u32>,T:Source<Out=R>>Base for T{}trait Child:Base{}impl<T:Source<Out=U>,U:?Sized+Tag<Out=u32>>Child for T{}",
            true,
        ),
        (
            "scalar_output",
            "trait Source{type Out;}trait Base{type Out;}impl<R,T:Source<Out=R>>Base for T{type Out=R;}trait Child:Base{fn get()->Self::Out;}impl<T:Source<Out=u32>>Child for T{fn get()->u32{73}}",
            true,
        ),
        (
            "chained_output",
            "trait Source{type Out;}trait Base{type Out;}impl<T:Source<Out=U>,U:Source<Out=R>,R>Base for T{type Out=R;}trait Child:Base{fn get()->Self::Out;}impl<T:Source<Out=U>,U:Source<Out=u32>>Child for T{fn get()->u32{73}}",
            true,
        ),
        (
            "borrowed_output",
            "trait Source{type Out;}trait Base{type Out;}impl<R,T:Source<Out=R>>Base for T{type Out=R;}trait Child:Base{}impl<'a,T:Source<Out=&'a u32>>Child for T{}",
            true,
        ),
        (
            "output_outlives_missing",
            "trait Source{type Out;}trait Base{type Out;}impl<R:'static,T:Source<Out=R>>Base for T{type Out=R;}trait Child:Base{}impl<'a,T:Source<Out=&'a u32>>Child for T{}",
            false,
        ),
        (
            "output_outlives_known",
            "trait Source{type Out;}trait Base{type Out;}impl<R:'static,T:Source<Out=R>>Base for T{type Out=R;}trait Child:Base{}impl<'a:'static,T:Source<Out=&'a u32>>Child for T{}",
            true,
        ),
        (
            "different_trait_lifetimes",
            "trait Source<'a>{type Out;}trait Base{type Out;}impl<R,T:Source<'static,Out=R>>Base for T{type Out=R;}trait Child:Base{}impl<'a,T:Source<'a,Out=u32>>Child for T{}",
            false,
        ),
        (
            "missing_requirement",
            "trait Source{type Out;}trait Marker{}trait Base{type Out;}impl<R,T:Source<Out=R>+Marker>Base for T{type Out=R;}trait Child:Base{}impl<T:Source<Out=u32>>Child for T{}",
            false,
        ),
        (
            "different_output",
            "trait Source{type Out;}trait Other{type Out;}trait Base{type Out;}impl<R,T:Source<Out=R>+Other<Out=R>>Base for T{type Out=R;}trait Child:Base{}impl<T:Source<Out=u32>+Other<Out=u64>>Child for T{}",
            false,
        ),
    ] {
        let source = format!("{source}fn main(){{}}");
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, &source).unwrap();
        let reference = Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("oracle.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            reference.status.success(),
            accepted,
            "rustc {name}: {}",
            String::from_utf8_lossy(&reference.stderr)
        );
        let (db, _, hir) = flux_hir::lower_text(&path, &source);
        assert!(
            db.syntax_errors().is_empty(),
            "{name}: {:?}",
            db.syntax_errors()
        );
        assert!(hir.diagnostics.is_empty(), "{name}: {:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        if matches!(
            name,
            "output_outlives_missing" | "different_trait_lifetimes"
        ) {
            assert!(
                typed.diagnostics.is_empty(),
                "{name} must reach lifetime checking: {:?}",
                typed.diagnostics
            );
        }
        let mut errors = typed.diagnostics.clone();
        if errors.is_empty() {
            errors.extend(flux_mir::build_file(&hir, &typed).diagnostics);
        }
        assert_eq!(errors.is_empty(), accepted, "flux {name}: {errors:?}");
    }
    std::fs::remove_dir_all(root).unwrap();
}

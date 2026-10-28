//! Projection identities are compared after normalizing their input arguments.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;

#[test]
fn normalization_environments_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-normalization-env-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "compound_caller_shadows_blanket",
            "trait Marker{}struct Bit;trait Op<R>{type Out;fn op(self,r:R)->Self::Out;}struct W<T>(T);impl<T,R:Marker>Op<R>for W<T>{type Out=T;fn op(self,_:R)->T{self.0}}fn f<T>(w:W<T>,r:Bit)-><W<T>as Op<Bit>>::Out where W<T>:Op<Bit>{w.op(r)}fn main(){}",
            true,
        ),
        (
            "compound_caller_output_is_abstract",
            "trait Marker{}struct Bit;trait Op<R>{type Out;fn op(self,r:R)->Self::Out;}struct W<T>(T);impl<T,R:Marker>Op<R>for W<T>{type Out=T;fn op(self,_:R)->T{self.0}}fn f<T>(w:W<T>,r:Bit)->T where W<T>:Op<Bit>{w.op(r)}fn main(){}",
            false,
        ),
        (
            "closed_self_open_argument",
            "trait Marker{}trait P<T>{type Out;fn make(t:T)->Self::Out;}impl<T:Marker>P<T>for(){type Out=T;fn make(t:T)->T{t}}fn f<T>(t:T)-><()as P<T>>::Out where ():P<T>{<()as P<T>>::make(t)}fn main(){}",
            true,
        ),
        (
            "closed_self_output_is_abstract",
            "trait Marker{}trait P<T>{type Out;fn make(t:T)->Self::Out;}impl<T:Marker>P<T>for(){type Out=T;fn make(t:T)->T{t}}fn f<T>(t:T)->T where ():P<T>{<()as P<T>>::make(t)}fn main(){}",
            false,
        ),
        (
            "closed_wrong_output",
            "trait Op{type Out;fn op(self)->Self::Out;}struct W;impl Op for W{type Out=u32;fn op(self)->u32{3}}fn f(w:W)->u64 where W:Op{w.op()}fn main(){}",
            false,
        ),
        (
            "closed_caller_projection",
            "trait Op{type Out;fn op(self)->Self::Out;}struct W;impl Op for W{type Out=u32;fn op(self)->u32{3}}fn f(w:W)->u32 where W:Op{w.op()}fn main(){}",
            true,
        ),
        (
            "distinct_trait_arguments",
            "trait Op<R>{type Out;fn op(self,r:R)->Self::Out;}struct W;impl Op<u8>for W{type Out=u32;fn op(self,_:u8)->u32{3}}impl Op<u16>for W{type Out=u64;fn op(self,_:u16)->u64{7}}fn f(w:W)->u64 where W:Op<u8>{<W as Op<u16>>::op(w,0u16)}fn main(){}",
            true,
        ),
        (
            "recursive_impl_method",
            "use core::ops::Mul;trait Num:Copy+'static{}struct N<U,B>(U,B);trait Step<Y,Z>{type Out;fn step(self,y:Y,n:Z)->Self::Out;}type Square<X>=<X as Mul>::Output;impl<Y:Num,U:Num,B:Copy,X:Num>Step<Y,N<N<U,B>,bool>>for X where X:Mul,Square<X>:Step<Y,N<U,B>>{type Out=<Square<X>as Step<Y,N<U,B>>>::Out;fn step(self,y:Y,n:N<N<U,B>,bool>)->Self::Out{(self*self).step(y,n.0)}}fn main(){}",
            true,
        ),
        (
            "default_rhs_method",
            "use core::ops::Mul;trait Step<T>{type Out;fn step(self,t:T)->Self::Out;}fn f<X:Copy+Mul,Y>(x:X,y:Y)-><<X as Mul>::Output as Step<Y>>::Out where <X as Mul>::Output:Step<Y>{(x*x).step(y)}fn main(){}",
            true,
        ),
        (
            "explicit_rhs_method",
            "use core::ops::Mul;trait Step<T>{type Out;fn step(self,t:T)->Self::Out;}fn f<X:Copy+Mul<X>,Y>(x:X,y:Y)-><<X as Mul<X>>::Output as Step<Y>>::Out where <X as Mul<X>>::Output:Step<Y>{(x*x).step(y)}fn main(){}",
            true,
        ),
        (
            "selected_missing_bound",
            "trait Num{}struct Z;struct W<T>(T);trait Step{type Out;}impl Step for Z{type Out=Z;}impl<T:Num+Step>Step for W<T>{type Out=T::Out;}trait Result{type Out;}impl Result for(){type Out=<W<Z>as Step>::Out;}fn main(){}",
            false,
        ),
        (
            "selected_wrong_output_bound",
            "trait Num{}struct Z;impl Num for Z{}struct W<T>(T);trait Step{type Out;}impl Step for Z{type Out=bool;}impl<T:Num+Step>Step for W<T>{type Out=T::Out;}trait Result{type Out:Num;}impl Result for(){type Out=<W<Z>as Step>::Out;}fn main(){}",
            false,
        ),
        (
            "selected_carry_bound",
            "trait Num:Copy+'static{}#[derive(Copy,Clone)]struct Z;impl Num for Z{}#[derive(Copy,Clone)]struct O<T>(T);#[derive(Copy,Clone)]struct I<T>(T);impl<T:Num>Num for O<T>{}impl<T:Num>Num for I<T>{}trait AddOne{type Out;}impl AddOne for Z{type Out=I<Z>;}impl<T:Num>AddOne for O<T>{type Out=I<T>;}impl<T:Num+AddOne>AddOne for I<T>where T::Out:Num{type Out=O<T::Out>;}trait Len{type Out:Num;}impl<T>Len for(T,){type Out=<I<Z>as AddOne>::Out;}fn main(){}",
            true,
        ),
        (
            "selected_associated_bound",
            "trait Num:Copy+'static{}#[derive(Copy,Clone)]struct Z;impl Num for Z{}#[derive(Copy,Clone)]struct O<T>(T);impl<T:Num>Num for O<T>{}trait AddOne{type Out:Num;}impl AddOne for Z{type Out=O<Z>;}impl<T:Num+AddOne>AddOne for O<T>{type Out=O<T::Out>;}trait Len{type Out:Num;}impl<T>Len for(T,){type Out=<O<Z>as AddOne>::Out;}fn main(){}",
            true,
        ),
        (
            "concrete_input_key",
            "trait S{type Out;}struct W;impl S for W{type Out=u32;}trait P<T>{type Out;}fn f<T:P<<W as S>::Out,Out=u8>>(x:<T as P<u32>>::Out)->u8{x}fn main(){}",
            true,
        ),
        (
            "generic_input_key",
            "trait S{type Out;}struct W<T>(T);impl<T>S for W<T>{type Out=T;}trait P<T>{type Out;}fn f<A,T:P<<W<A> as S>::Out,Out=u8>>(x:<T as P<A>>::Out)->u8{x}fn main(){}",
            true,
        ),
        (
            "abstract_output_static",
            "trait S{type Out;}trait Number:'static{}fn f<T:S>()where T::Out:Number{}fn main(){}",
            true,
        ),
        (
            "abstract_output_inherited_static",
            "trait S{type Out;}trait Number:'static{}trait Child:Number{}fn f<T:S>()where T::Out:Child{}fn main(){}",
            true,
        ),
        (
            "abstract_output_as_argument",
            "trait S{type Out;}trait Number:'static{}struct W<T:Number>(T);fn f<T:S>(x:T::Out)->W<T::Out>where T::Out:Number{W(x)}fn main(){}",
            true,
        ),
        (
            "wrong_output",
            "trait S{type Out;}struct W;impl S for W{type Out=u32;}trait P<T>{type Out;}fn f<T:P<<W as S>::Out,Out=u8>>(x:<T as P<u32>>::Out)->u16{x}fn main(){}",
            false,
        ),
        (
            "wrong_input",
            "trait S{type Out;}struct W;impl S for W{type Out=u32;}trait P<T>{type Out;}fn f<T:P<<W as S>::Out,Out=u8>+P<u16>>(x:<T as P<u16>>::Out)->u8{x}fn main(){}",
            false,
        ),
        (
            "missing_static_argument",
            "trait S{type Out;}trait Needs<T:'static>{}fn f<'a,T:Needs<&'a u32>>(){}fn main(){}",
            false,
        ),
        (
            "nonstatic_impl",
            "trait Number:'static{}struct Borrow<'a>(&'a u32);impl<'a>Number for Borrow<'a>{}fn main(){}",
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
            .arg(root.join("probe.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            oracle.status.success(),
            accepted,
            "rustc {name}: {}",
            String::from_utf8_lossy(&oracle.stderr)
        );
        let (db, _, hir) = flux_hir::lower_text(&path, source);
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
        assert_eq!(errors.is_empty(), accepted, "Flux {name}: {errors:?}");
    }
    std::fs::remove_dir_all(root).unwrap();
}

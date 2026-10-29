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
            "closed_projection_goal_wrong_argument",
            "trait Stream{type Token;}impl Stream for u32{type Token=u8;}trait Contains<T>{}impl Contains<u16>for u16{}fn make<I:Stream,S:Contains<I::Token>>(_:S,_:I){}fn f(){make(7u16,0u32);}fn main(){}",
            false,
        ),
        (
            "generic_caller_argument_remains_abstract",
            "trait Marker{}trait Family{type Out;}struct W<T>(T);impl<T:Marker>Family for W<T>{type Out=u8;}trait Accept<T>{fn accept(&self,t:T);}fn f<T,A:Accept<<W<T>as Family>::Out>>(a:&A)where W<T>:Family{a.accept(7u8);}fn main(){}",
            false,
        ),
        (
            "generic_caller_declared_argument",
            "trait Marker{}trait Family{type Out;}struct W<T>(T);impl<T:Marker>Family for W<T>{type Out=u8;}trait Accept<T>{fn accept(&self,t:T);}fn f<T,A:Accept<<W<T>as Family>::Out>>(a:&A,t:<W<T>as Family>::Out)where W<T>:Family{a.accept(t);}fn main(){}",
            true,
        ),
        (
            "nominal_callback_closed_projection",
            "trait Family{type Out;}impl Family for u32{type Out=u8;}struct Node<T>(T);trait Apply<I:Family>{fn apply<F:Fn(Node<I::Out>)>(&self,f:F);}struct A;impl<I:Family>Apply<I>for A{fn apply<F:Fn(Node<I::Out>)>(&self,_:F){}}fn f(){<A as Apply<u32>>::apply(&A,|_:Node<u8>|{});}fn main(){}",
            true,
        ),
        (
            "nominal_callback_wrong_projection",
            "trait Family{type Out;}impl Family for u32{type Out=u8;}struct Node<T>(T);trait Apply<I:Family>{fn apply<F:Fn(Node<I::Out>)>(&self,f:F);}struct A;impl<I:Family>Apply<I>for A{fn apply<F:Fn(Node<I::Out>)>(&self,_:F){}}fn f(){<A as Apply<u32>>::apply(&A,|_:Node<u16>|{});}fn main(){}",
            false,
        ),
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
            "selected_input_bound_later",
            "trait Mark{}trait S{type Out;}struct W<A>(A);impl<A:Mark>S for W<A>{type Out=A;}trait P<T>{type Out;}fn f<A,T:P<<W<A>as S>::Out,Out=u8>>(x:<T as P<A>>::Out)->u8 where A:Mark{x}fn main(){}",
            true,
        ),
        (
            "pending_normalized_fact_forward",
            "trait S{type Out;}trait Mark<A>{}trait P<A>{type Out;}struct Z;impl S for Z{type Out=u32;}struct W<T>(T);impl<T:Mark<u32>>S for W<T>{type Out=u32;}fn f<T:P<<W<T>as S>::Out,Out=u8>+Mark<<Z as S>::Out>>(x:<T as P<u32>>::Out)->u8{x}fn main(){}",
            false,
        ),
        (
            "pending_normalized_fact_reverse",
            "trait S{type Out;}trait Mark<A>{}trait P<A>{type Out;}struct Z;impl S for Z{type Out=u32;}struct W<T>(T);impl<T:Mark<u32>>S for W<T>{type Out=u32;}fn f<T:Mark<<Z as S>::Out>+P<<W<T>as S>::Out,Out=u8>>(x:<T as P<u32>>::Out)->u8{x}fn main(){}",
            false,
        ),
        (
            "normalized_fact_for_value_signature",
            "trait S{type Out;}trait Mark<A>{}struct Z;impl S for Z{type Out=u32;}struct W<T>(T);impl<T:Mark<u32>>S for W<T>{type Out=u32;}fn f<T:Mark<<Z as S>::Out>>()->Option<<W<T>as S>::Out>{None}fn main(){}",
            true,
        ),
        (
            "normalized_fact_for_body",
            "trait S{type Out;}trait Mark<A>{}struct Z;impl S for Z{type Out=u32;}fn need<T:Mark<u32>>(){}fn f<T:Mark<<Z as S>::Out>>(){need::<T>()}fn main(){}",
            true,
        ),
        (
            "selected_input_bound_earlier",
            "trait Mark{}trait S{type Out;}struct W<A>(A);impl<A:Mark>S for W<A>{type Out=A;}trait P<T>{type Out;}fn f<A:Mark,T:P<<W<A>as S>::Out,Out=u8>>(x:<T as P<A>>::Out)->u8{x}fn main(){}",
            true,
        ),
        (
            "selected_input_marker_missing",
            "trait Mark{}trait S{type Out;}struct W<A>(A);impl<A:Mark>S for W<A>{type Out=A;}trait P<T>{type Out;}fn f<A,T:P<<W<A>as S>::Out,Out=u8>>(x:<T as P<A>>::Out)->u8{x}fn main(){}",
            false,
        ),
        (
            "selected_input_static",
            "trait S{type Out;}struct W<A>(A);impl<A:'static>S for W<A>{type Out=A;}trait P<T>{type Out;}fn f<A:'static,T:P<<W<A>as S>::Out,Out=u8>>(x:<T as P<A>>::Out)->u8{x}fn main(){}",
            true,
        ),
        (
            "selected_input_static_missing",
            "trait S{type Out;}struct W<A>(A);impl<A:'static>S for W<A>{type Out=A;}trait P<T>{type Out;}fn f<A,T:P<<W<A>as S>::Out,Out=u8>>(x:<T as P<A>>::Out)->u8{x}fn main(){}",
            false,
        ),
        (
            "borrowed_input_rows",
            "trait P<'d,T>{type Out;}fn f<'d,'a,'b,T:P<'d,(&'a u32,&'b u32),Out=u8>>(x:<T as P<'d,(&'a u32,&'b u32)>>::Out)->u8{x}fn main(){}",
            true,
        ),
        (
            "borrowed_input_rows_swapped",
            "trait P<'d,T>{type Out;}fn f<'d,'a,'b,T:P<'d,(&'a u32,&'b u32),Out=u8>>(x:<T as P<'d,(&'b u32,&'a u32)>>::Out)->u8{x}fn main(){}",
            false,
        ),
        (
            "borrowed_input_row_static",
            "trait P<'d,T>{type Out;}fn f<'d,'a,T:P<'d,&'a u32,Out=u8>>(x:<T as P<'d,&'static u32>>::Out)->u8{x}fn main(){}",
            false,
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

/// Closed projection arguments introduced by RPIT contracts also retain
/// their represented scalar output when lowered to MIR.
#[test]
fn opaque_projection_arguments_match_rustc() {
    let root = std::env::temp_dir().join(format!(
        "flux-opaque-projection-args-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "opaque_closed_input_projection",
            "trait Stream{type Token;fn take(&mut self)->Self::Token;}impl Stream for u32{type Token=u8;fn take(&mut self)->u8{7}}trait Parser<I,O>{fn parse(&mut self,input:&mut I)->O;}struct Run;impl<I:Stream>Parser<I,I::Token>for Run{fn parse(&mut self,input:&mut I)->I::Token{input.take()}}fn make<I:Stream>()->impl Parser<I,I::Token>{Run}fn f(input:&mut u32)->u8{make().parse(input)}fn main(){}",
            true,
        ),
        (
            "opaque_nested_projection_output",
            "trait Stream{type Token;fn take(&mut self)->Self::Token;}impl Stream for u32{type Token=u8;fn take(&mut self)->u8{7}}struct Node<T>(T);trait Parser<I,O>{fn parse(&mut self,input:&mut I)->O;}struct Run;impl<I:Stream>Parser<I,Node<I::Token>>for Run{fn parse(&mut self,input:&mut I)->Node<I::Token>{Node(input.take())}}fn make<I:Stream>()->impl Parser<I,Node<I::Token>>{Run}fn f(input:&mut u32)->Node<u8>{make().parse(input)}fn main(){}",
            true,
        ),
        (
            "closed_projection_goal_argument",
            "trait Stream{type Token;fn take(&mut self)->Self::Token;}impl Stream for u32{type Token=u8;fn take(&mut self)->u8{7}}trait Contains<T>{}impl Contains<u8>for u8{}trait Parser<I,O>{fn parse(&mut self,input:&mut I)->O;}struct Run;impl<I:Stream>Parser<I,I::Token>for Run{fn parse(&mut self,input:&mut I)->I::Token{input.take()}}fn make<I:Stream,S:Contains<I::Token>>(_:S)->impl Parser<I,I::Token>{Run}fn f(input:&mut u32)->u8{make(7u8).parse(input)}fn main(){}",
            true,
        ),
    ] {
        if !case_filter::selected(name) {
            continue;
        }
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let rustc = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("case.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            rustc.status.success(),
            accepted,
            "rustc {name}:{}",
            String::from_utf8_lossy(&rustc.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text(&path, source);
        assert!(hir.diagnostics.is_empty(), "{name}:{:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        assert!(
            typed.diagnostics.is_empty(),
            "{name}:{:?}",
            typed.diagnostics
        );
        let mir = flux_mir::build_file(&hir, &typed);
        assert!(mir.diagnostics.is_empty(), "{name}:{:?}", mir.diagnostics);
    }
}

#[test]
fn parser_map_projection_inference_matches_rustc() {
    let prefix = "trait Stream{type Token;fn take(&mut self)->Self::Token;}impl Stream for u32{type Token=u8;fn take(&mut self)->u8{7}}trait Parser<I,O>{fn parse(&mut self,i:&mut I)->O;fn map<F,O2>(self,f:F)->Mapped<Self,F,I,O,O2>where Self:Sized,F:FnMut(O)->O2{Mapped(self,f,core::marker::PhantomData)}}struct Mapped<P,F,I,O,O2>(P,F,core::marker::PhantomData<(I,O,O2)>);impl<P,F,I,O,O2>Parser<I,O2>for Mapped<P,F,I,O,O2>where P:Parser<I,O>,F:FnMut(O)->O2{fn parse(&mut self,i:&mut I)->O2{(self.1)(self.0.parse(i))}}struct Run;impl<I:Stream>Parser<I,I::Token>for Run{fn parse(&mut self,i:&mut I)->I::Token{i.take()}}fn make<I:Stream>()->impl Parser<I,I::Token>{Run}";
    let root =
        std::env::temp_dir().join(format!("flux-parser-map-inference-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, body, accepted) in [
        (
            "eligibility_late_projected_output",
            "struct WrappedParser<P>(P);trait UseParser<I>{fn use_it(&mut self,i:&mut I)->u8;}impl<I,P:Parser<I,u8>>UseParser<I>for WrappedParser<P>{fn use_it(&mut self,i:&mut I)->u8{self.0.parse(i)}}fn f(i:&mut u32)->u8{WrappedParser(make()).use_it(i)}",
            true,
        ),
        (
            "eligibility_wrong_committed_output",
            "struct WrappedParser<P>(P);trait UseParser<I>{fn use_it(&mut self,i:&mut I)->u8;}impl<I,P:Parser<I,u8>>UseParser<I>for WrappedParser<P>{fn use_it(&mut self,i:&mut I)->u8{self.0.parse(i)}}struct Other;impl Stream for Other{type Token=u16;fn take(&mut self)->u16{9}}fn f(i:&mut Other)->u8{WrappedParser(make()).use_it(i)}",
            false,
        ),
        (
            "forced_output_late_input",
            "fn force<I,P:Parser<I,u8>>(p:P)->P{p}fn f(i:&mut u32)->u8{force(make()).parse(i)}",
            true,
        ),
        (
            "forced_output_wrong_input",
            "struct Other;impl Stream for Other{type Token=u16;fn take(&mut self)->u16{9}}fn force<I,P:Parser<I,u8>>(p:P)->P{p}fn f(i:&mut Other)->u8{force(make()).parse(i)}",
            false,
        ),
        (
            "forced_output_abstract_input",
            "fn force<I,P:Parser<I,u8>>(p:P)->P{p}fn f<I:Stream>(i:&mut I)->u8{force(make()).parse(i)}",
            false,
        ),
        (
            "forced_output_explicit_equality",
            "fn force<I,P:Parser<I,u8>>(p:P)->P{p}fn f<I:Stream<Token=u8>>(i:&mut I)->u8{force(make()).parse(i)}",
            true,
        ),
        (
            "forced_wrong_output",
            "fn force<I,P:Parser<I,u16>>(p:P)->P{p}fn f(i:&mut u32)->u16{force(make()).parse(i)}",
            false,
        ),
        (
            "mapped_late_input",
            "fn f(i:&mut u32)->u8{make().map(|v|match v{1u8=>3u8,_=>4u8}).parse(i)}",
            true,
        ),
        (
            "mapped_wrong_token",
            "fn f(i:&mut u32)->u8{make().map(|v:u16|v as u8).parse(i)}",
            false,
        ),
        (
            "mapped_generic_abstract",
            "fn f<I:Stream>(i:&mut I)->u8{make().map(|v:u8|v).parse(i)}",
            false,
        ),
        (
            "mapped_generic_equality",
            "fn f<I:Stream<Token=u8>>(i:&mut I)->u8{make().map(|v:u8|v).parse(i)}",
            true,
        ),
    ] {
        if !case_filter::selected(name) {
            continue;
        }
        let source = format!("{prefix}{body}fn main(){{}}");
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, &source).unwrap();
        let rustc = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("case.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            rustc.status.success(),
            accepted,
            "rustc {name}:{}",
            String::from_utf8_lossy(&rustc.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text(&path, &source);
        let mut errors = hir.diagnostics.clone();
        if errors.is_empty() {
            let typed = flux_mir::check_file(&hir);
            errors.extend(typed.diagnostics.clone());
            if errors.is_empty() {
                errors.extend(flux_mir::build_file(&hir, &typed).diagnostics);
            }
        }
        assert_eq!(errors.is_empty(), accepted, "flux {name}:{errors:?}");
    }
}

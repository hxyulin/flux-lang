//! Anonymous selected-method inputs retain their original occurrence recipes.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;
#[test]
fn method_projection_occurrences_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-method-projection-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "anonymous",
            "trait Stream{type Checkpoint;}struct TokenSlice<'a,T>(&'a[T]);struct Checkpoint<T>(T);impl<'a,T>Stream for TokenSlice<'a,T>{type Checkpoint=Checkpoint<Self>;}trait Offset<S=Self>{fn offset(&self,other:&S)->usize;}impl<T>Offset<<TokenSlice<'_,T>as Stream>::Checkpoint>for TokenSlice<'_,T>{fn offset(&self,other:&<TokenSlice<'_,T>as Stream>::Checkpoint)->usize{0}}",
            true,
        ),
        (
            "named_header_anonymous_method",
            "trait Stream{type Checkpoint;}struct TokenSlice<'a,T>(&'a[T]);struct Checkpoint<T>(T);impl<'a,T>Stream for TokenSlice<'a,T>{type Checkpoint=Checkpoint<Self>;}trait Offset<S=Self>{fn offset(&self,other:&S)->usize;}impl<'a,'b,T>Offset<<TokenSlice<'a,T>as Stream>::Checkpoint>for TokenSlice<'b,T>{fn offset(&self,other:&<TokenSlice<'_,T>as Stream>::Checkpoint)->usize{0}}",
            true,
        ),
        (
            "independent_named_method",
            "trait Stream{type Checkpoint;}struct TokenSlice<'a,T>(&'a[T]);struct Checkpoint<T>(T);impl<'a,T>Stream for TokenSlice<'a,T>{type Checkpoint=Checkpoint<Self>;}trait Offset<S=Self>{fn offset(&self,other:&S)->usize;}impl<'a,'b,T>Offset<<TokenSlice<'a,T>as Stream>::Checkpoint>for TokenSlice<'b,T>{fn offset<'m>(&self,other:&<TokenSlice<'m,T>as Stream>::Checkpoint)->usize{0}}",
            true,
        ),
        (
            "named_header_wrong_link",
            "trait Stream{type Checkpoint;}struct TokenSlice<'a,T>(&'a[T]);struct Checkpoint<T>(T);impl<'a,T>Stream for TokenSlice<'a,T>{type Checkpoint=Checkpoint<Self>;}trait Offset<S=Self>{fn offset(&self,other:&S)->usize;}impl<'a,'b,T>Offset<<TokenSlice<'a,T>as Stream>::Checkpoint>for TokenSlice<'b,T>{fn offset(&self,other:&<TokenSlice<'b,T>as Stream>::Checkpoint)->usize{0}}",
            false,
        ),
        (
            "named_header_static_method",
            "trait Stream{type Checkpoint;}struct TokenSlice<'a,T>(&'a[T]);struct Checkpoint<T>(T);impl<'a,T>Stream for TokenSlice<'a,T>{type Checkpoint=Checkpoint<Self>;}trait Offset<S=Self>{fn offset(&self,other:&S)->usize;}impl<'a,T>Offset<<TokenSlice<'a,T>as Stream>::Checkpoint>for TokenSlice<'a,T>{fn offset(&self,other:&<TokenSlice<'static,T>as Stream>::Checkpoint)->usize{0}}",
            false,
        ),
        (
            "stricter_named_method",
            "trait Stream{type Checkpoint;}struct TokenSlice<'a,T>(&'a[T]);struct Checkpoint<T>(T);impl<'a,T>Stream for TokenSlice<'a,T>{type Checkpoint=Checkpoint<Self>;}trait Offset<S=Self>{fn offset(&self,other:&S)->usize;}impl<'a,T>Offset<<TokenSlice<'a,T>as Stream>::Checkpoint>for TokenSlice<'a,T>{fn offset<'m:'static>(&self,other:&<TokenSlice<'m,T>as Stream>::Checkpoint)->usize{0}}",
            false,
        ),
        (
            "borrowed_result_escape",
            "trait Stream{type Checkpoint;}struct TokenSlice<'a,T>(&'a[T]);struct Checkpoint<T>(T);impl<'a,T>Stream for TokenSlice<'a,T>{type Checkpoint=Checkpoint<Self>;}trait Read<S>{fn get(&self,other:&S)->&'static u32;}impl<'a>Read<Checkpoint<TokenSlice<'a,u32>>>for (){fn get(&self,other:&<TokenSlice<'_,u32>as Stream>::Checkpoint)->&'static u32{let data=other.0.0;&data[0]}}",
            false,
        ),
        (
            "mutable_independent_inputs",
            "trait Stream{type Checkpoint;}struct TokenSlice<'a,T>(&'a[T]);struct Checkpoint<T>(T);impl<'a,T>Stream for TokenSlice<'a,T>{type Checkpoint=Checkpoint<Self>;}trait Write<A,B>{fn copy(&self,left:&mut A,right:&B);}impl<'a,'b>Write<Checkpoint<TokenSlice<'a,u32>>,Checkpoint<TokenSlice<'b,u32>>>for (){fn copy(&self,left:&mut <TokenSlice<'_,u32>as Stream>::Checkpoint,right:&<TokenSlice<'_,u32>as Stream>::Checkpoint){left.0.0=right.0.0;}}",
            false,
        ),
        (
            "unused_abstract_control",
            "trait Stream{type Slice;fn take(&mut self)->Self::Slice;}trait Parser<I,O>{fn parse(&mut self,input:&mut I)->O;}struct P;impl<I:Stream>Parser<I,I::Slice>for P{fn parse(&mut self,input:&mut I)->I::Slice{input.take()}}",
            true,
        ),
        (
            "represented_family_control",
            "trait Lending{type Item<'a>where Self:'a;fn get(&self)->Self::Item<'_>;}struct S(u32);impl Lending for S{type Item<'a>=&'a u32 where Self:'a;fn get(&self)->Self::Item<'_>{&self.0}}",
            true,
        ),
        (
            "abstract_qualified_input",
            "trait Stream{type Checkpoint;}trait Append<I:Stream>{fn append(self,input:&I,start:&I::Checkpoint)->Self;}struct E;impl<I:Stream>Append<I>for E{fn append(self,_input:&I,_start:&<I as Stream>::Checkpoint)->Self{self}}",
            true,
        ),
        (
            "abstract_qualified_result",
            "trait Stream{type Slice;}trait Parser<I:Stream,E>{fn parse(&mut self,input:&mut I)->Result<I::Slice,E>;}struct P;impl<I:Stream,E>Parser<I,E>for P{fn parse(&mut self,_input:&mut I)->Result<<I as Stream>::Slice,E>{loop{}}}",
            true,
        ),
        (
            "abstract_nested_tuple_result",
            "trait Stream{type Slice;}trait Parser<I:Stream,O,E>{fn parse(&mut self,input:&mut I)->Result<(O,I::Slice),E>;}struct P;impl<I:Stream,O,E>Parser<I,O,E>for P{fn parse(&mut self,_input:&mut I)->Result<(O,<I as Stream>::Slice),E>{loop{}}}",
            true,
        ),
        (
            "partially_normalized_tuple_input",
            "trait Base{type Item;}impl<T:Base>Base for(T,usize){type Item=(T::Item,T);}trait Method<A>{fn f(&self,x:&A);}impl<T:Base>Method<<(T,usize)as Base>::Item>for(T,usize){fn f(&self,_x:&<(T,usize)as Base>::Item){}}",
            true,
        ),
        (
            "partially_normalized_record_input",
            "trait Base{type Item;}struct W<T>(T);struct Check<I,S>(I,core::marker::PhantomData<S>);impl<T:Base>Base for W<T>{type Item=Check<T::Item,Self>;}trait Method<A>{fn f(&self,x:&A);}impl<T:Base>Method<<W<T>as Base>::Item>for W<T>{fn f(&self,_x:&<W<T>as Base>::Item){}}",
            true,
        ),
        (
            "abstract_wrong_item",
            "trait Stream{type Slice;}trait Parser<I:Stream,E>{fn parse(&mut self,input:&mut I)->Result<I::Slice,E>;}struct P;impl<I:Stream,E>Parser<I,E>for P{fn parse(&mut self,_input:&mut I)->Result<u8,E>{loop{}}}",
            false,
        ),
        (
            "abstract_missing_owner",
            "trait Stream{type Slice;}trait Method<A>{fn f(&self,x:&A);}struct P;impl<I>Method<I>for P{fn f(&self,_x:&<I as Stream>::Slice){}}",
            false,
        ),
        (
            "abstract_static_result_wrong",
            "trait Stream{type Slice;}trait Parser<I:Stream>{fn parse(&mut self,input:&mut I)->&'static str;}struct P;impl<'a,I:Stream<Slice=&'a str>>Parser<I>for P{fn parse(&mut self,_input:&mut I)-><I as Stream>::Slice{loop{}}}",
            false,
        ),
        (
            "abstract_static_result_proven",
            "trait Stream{type Slice;}trait Parser<I:Stream>{fn parse(&mut self,input:&mut I)->&'static str;}struct P;impl<'a:'static,I:Stream<Slice=&'a str>>Parser<I>for P{fn parse(&mut self,_input:&mut I)-><I as Stream>::Slice{loop{}}}",
            true,
        ),
        (
            "abstract_different_owner_wrong",
            "trait Stream{type Slice;}trait Parser<I:Stream,J:Stream>{fn parse(&mut self,input:&mut I,other:&mut J)->I::Slice;}struct P;impl<I:Stream,J:Stream>Parser<I,J>for P{fn parse(&mut self,_input:&mut I,_other:&mut J)-><J as Stream>::Slice{loop{}}}",
            false,
        ),
        (
            "abstract_equal_owner_proven",
            "trait Stream{type Slice;}trait Parser<I:Stream,J:Stream>{fn parse(&mut self,input:&mut I,other:&mut J)->I::Slice;}struct P;impl<I:Stream<Slice=J::Slice>,J:Stream>Parser<I,J>for P{fn parse(&mut self,_input:&mut I,_other:&mut J)-><J as Stream>::Slice{loop{}}}",
            true,
        ),
        (
            "review_concrete_two_same_erased_good",
            "trait S{type Item;}impl<'a>S for &'a u32{type Item=&'a u32;}trait M<A,B>{fn f(&self,a:&A,b:&B);}impl<'a,'b>M<<&'a u32 as S>::Item,<&'b u32 as S>::Item>for(){fn f(&self,_:&<&'a u32 as S>::Item,_:&<&'b u32 as S>::Item){}}",
            true,
        ),
        (
            "review_partial_ref_abstract_same",
            "trait S{type Item;}struct W<'a,T>(&'a u32,T);impl<'a,T:S>S for W<'a,T>{type Item=(&'a u32,T::Item);}trait M<A>{fn f(&self,x:&A);}impl<'a,T:S>M<<W<'a,T>as S>::Item>for W<'a,T>{fn f(&self,_:&<W<'a,T>as S>::Item){}}",
            true,
        ),
        (
            "review_partial_mut_wrong_link",
            "trait S{type Item;}struct W<'a,T>(&'a mut u32,T);impl<'a,T:S>S for W<'a,T>{type Item=(&'a mut u32,T::Item);}trait M<A>{fn f(&self,x:&mut A);}impl<'a,'b,T:S>M<<W<'a,T>as S>::Item>for W<'b,T>{fn f(&self,_:&mut <W<'b,T>as S>::Item){}}",
            false,
        ),
        (
            "review_conditional_owner_missing",
            "trait S{type Item;}struct W<'a>(&'a u32);impl<'a:'static>S for W<'a>{type Item=u32;}trait M<A>{fn f(&self,x:&A);}impl<'a>M<u32>for W<'a>{fn f(&self,_:&<W<'a>as S>::Item){}}",
            false,
        ),
        (
            "review_partial_ref_abstract_wrong_static",
            "trait S{type Item;}struct W<'a,T>(&'a u32,T);impl<'a,T:S>S for W<'a,T>{type Item=(&'a u32,T::Item);}trait M<A>{fn f(&self,x:&A);}impl<'a,T:S>M<<W<'a,T>as S>::Item>for W<'a,T>{fn f(&self,_:&(&'static u32,T::Item)){}}",
            false,
        ),
        (
            "review_partial_ref_abstract_wrong_owner",
            "trait S{type Item;}struct W<'a,T>(&'a u32,T);impl<'a,T:S>S for W<'a,T>{type Item=(&'a u32,T::Item);}trait M<A>{fn f(&self,x:&A);}impl<'a,'b,T:S>M<<W<'a,T>as S>::Item>for W<'b,T>{fn f(&self,_:&<W<'b,T>as S>::Item){}}",
            false,
        ),
        (
            "review_partial_ref_abstract_concrete",
            "trait S{type Item;}struct W<'a,T>(&'a u32,T);impl<'a,T:S>S for W<'a,T>{type Item=(&'a u32,T::Item);}trait M<A>{fn f(&self,x:&A);}impl<'a,T:S>M<<W<'a,T>as S>::Item>for W<'a,T>{fn f(&self,_:&(&'a u32,T::Item)){}}",
            true,
        ),
        (
            "review_partial_return_good",
            "trait S{type Item;}struct W<'a,T>(&'a u32,T);impl<'a,T:S>S for W<'a,T>{type Item=(&'a u32,T::Item);}trait M<A>{fn f(&self)->A;}impl<'a,T:S>M<<W<'a,T>as S>::Item>for W<'a,T>{fn f(&self)-><W<'a,T>as S>::Item{loop{}}}",
            true,
        ),
        (
            "review_partial_nested_record",
            "trait S{type Item;}struct Wrap<T>(T);struct W<'a,T>(&'a u32,T);impl<'a,T:S>S for W<'a,T>{type Item=Wrap<(&'a u32,T::Item)>;}trait M<A>{fn f(&self,x:&A);}impl<'a,T:S>M<<W<'a,T>as S>::Item>for W<'a,T>{fn f(&self,_:&<W<'a,T>as S>::Item){}}",
            true,
        ),
        (
            "review_concrete_two_same_erased_swap",
            "trait S{type Item;}impl<'a>S for &'a u32{type Item=&'a u32;}trait M<A,B>{fn f(&self,a:&A,b:&B);}impl<'a,'b>M<<&'a u32 as S>::Item,<&'b u32 as S>::Item>for(){fn f(&self,_:&<&'b u32 as S>::Item,_:&<&'a u32 as S>::Item){}}",
            false,
        ),
        (
            "partial_real_return_local_escape",
            "trait S{type Item;}struct W<'a,T>(&'a u32,T);impl<'a,T:S>S for W<'a,T>{type Item=(&'a u32,T::Item);}trait M<A>{fn f(&self)->A;}impl<'a,T:S<Item=u32>>M<<W<'a,T>as S>::Item>for W<'a,T>{fn f(&self)-><W<'a,T>as S>::Item{let local=9;(&local,0)}}",
            false,
        ),
        (
            "partial_real_return_static_escape",
            "trait S{type Item;}struct W<'a,T>(&'a u32,T);impl<'a,T:S>S for W<'a,T>{type Item=(&'a u32,T::Item);}trait M<A>{fn f(&self)->A;}impl<'a,T:S<Item=u32>>M<(&'static u32,T::Item)>for W<'a,T>{fn f(&self)->(&'static u32,T::Item){(self.0,0)}}",
            false,
        ),
        (
            "partial_mutable_body_wrong_link",
            "trait S{type Item;}struct W<'a,T>(&'a u32,T);impl<'a,T:S>S for W<'a,T>{type Item=(&'a u32,T::Item);}trait M<A,B>{fn f(&self,x:&mut A,y:&B);}impl<'a,'b,T:S>M<<W<'a,T>as S>::Item,<W<'b,T>as S>::Item>for W<'a,T>{fn f(&self,x:&mut <W<'a,T>as S>::Item,y:&<W<'b,T>as S>::Item){x.0=y.0;}}",
            false,
        ),
        (
            "partial_mutable_body_proven_link",
            "trait S{type Item;}struct W<'a,T>(&'a u32,T);impl<'a,T:S>S for W<'a,T>{type Item=(&'a u32,T::Item);}trait M<A,B>{fn f(&self,x:&mut A,y:&B);}impl<'a,'b:'a,T:S>M<<W<'a,T>as S>::Item,<W<'b,T>as S>::Item>for W<'a,T>{fn f(&self,x:&mut <W<'a,T>as S>::Item,y:&<W<'b,T>as S>::Item){x.0=y.0;}}",
            true,
        ),
        (
            "partial_direct_call_escape",
            "trait S{type Item;}struct W<'a,T>(&'a u32,T);impl<'a,T:S>S for W<'a,T>{type Item=(&'a u32,T::Item);}trait M<A>{fn f(&self)->A;}impl<'a,T:S<Item=u32>>M<<W<'a,T>as S>::Item>for W<'a,T>{fn f(&self)-><W<'a,T>as S>::Item{(self.0,0)}}struct Atom;impl S for Atom{type Item=u32;}fn bad()->(&'static u32,u32){let local=7;let value=W(&local,Atom);value.f()}",
            false,
        ),
        (
            "partial_direct_item_call_escape",
            "trait S{type Item;}struct W<'a,T>(&'a u32,T);impl<'a,T:S>S for W<'a,T>{type Item=(&'a u32,T::Item);}trait M<A>{fn f(&self)->A;}impl<'a,T:S<Item=u32>>M<<W<'a,T>as S>::Item>for W<'a,T>{fn f(&self)-><W<'a,T>as S>::Item{(self.0,0)}}struct Atom;impl S for Atom{type Item=u32;}fn bad()->(&'static u32,u32){let local=7;let value=W(&local,Atom);let item=W::<Atom>::f;item(&value)}",
            false,
        ),
        (
            "partial_fndef_call_escape",
            "trait S{type Item;}struct W<'a,T>(&'a u32,T);impl<'a,T:S>S for W<'a,T>{type Item=(&'a u32,T::Item);}trait M<A>{fn f(&self)->A;}impl<'a,T:S<Item=u32>>M<<W<'a,T>as S>::Item>for W<'a,T>{fn f(&self)-><W<'a,T>as S>::Item{(self.0,0)}}struct Atom;impl S for Atom{type Item=u32;}fn invoke<A,R,F:FnOnce(A)->R>(f:F,a:A)->R{f(a)}fn bad()->(&'static u32,u32){let local=7;let value=W(&local,Atom);invoke(W::<Atom>::f,&value)}",
            false,
        ),
    ] {
        if !case_filter::selected(name) {
            continue;
        }
        let source = format!("{source}fn main(){{}}");
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
            if name.starts_with("partial_real_return")
                || name.starts_with("partial_direct")
                || name == "partial_fndef_call_escape"
                || name == "partial_mutable_body_wrong_link"
            {
                assert!(
                    typed.diagnostics.is_empty(),
                    "expected a MIR lifetime rejection for {name}: {:?}",
                    typed.diagnostics
                );
            }
            errors.extend(typed.diagnostics.clone());
            if errors.is_empty() {
                errors.extend(flux_mir::build_file(&hir, &typed).diagnostics);
            }
        }
        if name.starts_with("partial_real_return")
            || name.starts_with("partial_direct")
            || name == "partial_fndef_call_escape"
            || name == "partial_mutable_body_wrong_link"
        {
            assert!(
                errors
                    .iter()
                    .all(|error| !error.message.contains("not supported")
                        && !error.message.contains("not represented")
                        && !error.message.contains("region rows")),
                "expected a lifetime rejection, not a representation boundary for {name}: {errors:?}"
            );
        }
        assert_eq!(errors.is_empty(), accepted, "flux {name}:{errors:?}");
    }
}

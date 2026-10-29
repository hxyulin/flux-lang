//! Opaque bound occurrences retain their own trait argument rows.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;
#[test]
fn opaque_bound_regions_match_rustc() {
    let root =
        std::env::temp_dir().join(format!("flux-opaque-bound-regions-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for edition in ["2021", "2024"] {
        for (name, source, accepted) in [
            (
                "abstract_conditional_impl",
                "trait Stream{type Slice;fn take(&mut self)->Self::Slice;}\ntrait Compare<T>{}\ntrait Parser<I,O,E>{fn parse_next(&mut self,input:&mut I)->Result<O,E>;}\nstruct Literal<L>(L);\nimpl<L,I:Stream+Compare<L>,E>Parser<I,I::Slice,E>for Literal<L>{fn parse_next(&mut self,input:&mut I)->Result<I::Slice,E>{Ok(input.take())}}\nfn literal<L,I,E>(value:L)->impl Parser<I,I::Slice,E>where I:Stream+Compare<L>{Literal(value)}\n\n\ntrait Marker{}impl<T:Marker>Stream for T{type Slice=u32;fn take(&mut self)->u32{7}}fn run<I:Stream+Compare<()>>(input:&mut I)->Result<I::Slice,()>{literal(()).parse_next(input)}fn main(){}",
                true,
            ),
            (
                "abstract_conditional_no_static",
                "trait Stream{type Slice;fn take(&mut self)->Self::Slice;}\ntrait Compare<T>{}\ntrait Parser<I,O,E>{fn parse_next(&mut self,input:&mut I)->Result<O,E>;}\nstruct Literal<L>(L);\nimpl<L,I:Stream+Compare<L>,E>Parser<I,I::Slice,E>for Literal<L>{fn parse_next(&mut self,input:&mut I)->Result<I::Slice,E>{Ok(input.take())}}\nfn literal<L,I,E>(value:L)->impl Parser<I,I::Slice,E>where I:Stream+Compare<L>{Literal(value)}\n\n\ntrait Marker{}impl<T:Marker>Stream for T{type Slice=u32;fn take(&mut self)->u32{7}}fn need<T:'static>(_:T){}fn run<I:Stream+Compare<()>>(input:&mut I){let result:Result<I::Slice,()>=literal(()).parse_next(input);need(result.unwrap());}fn main(){}",
                false,
            ),
            (
                "abstract_wrong_literal_input",
                "trait Stream{type Slice;fn take(&mut self)->Self::Slice;}\ntrait Compare<T>{}\ntrait Parser<I,O,E>{fn parse_next(&mut self,input:&mut I)->Result<O,E>;}\nstruct Literal<L>(L);\nimpl<L,I:Stream+Compare<L>,E>Parser<I,I::Slice,E>for Literal<L>{fn parse_next(&mut self,input:&mut I)->Result<I::Slice,E>{Ok(input.take())}}\nfn literal<L,I,E>(value:L)->impl Parser<I,I::Slice,E>where I:Stream+Compare<L>{Literal(value)}\nfn run<'s,I:Stream+Compare<&'static str>,E>(value:&'s str,input:&mut I)->Result<I::Slice,E>{literal(value).parse_next(input)}\nfn main(){}\n",
                false,
            ),
            (
                "abstract_literal_impl",
                "trait Stream{type Slice;fn take(&mut self)->Self::Slice;}\ntrait Compare<T>{}\ntrait Parser<I,O,E>{fn parse_next(&mut self,input:&mut I)->Result<O,E>;}\nstruct Literal<L>(L);\nimpl<L,I:Stream+Compare<L>,E>Parser<I,I::Slice,E>for Literal<L>{fn parse_next(&mut self,input:&mut I)->Result<I::Slice,E>{Ok(input.take())}}\nfn literal<L,I,E>(value:L)->impl Parser<I,I::Slice,E>where I:Stream+Compare<L>{Literal(value)}\nimpl<'s,I:Stream+Compare<&'s str>,E>Parser<I,I::Slice,E>for &'s str{fn parse_next(&mut self,input:&mut I)->Result<I::Slice,E>{literal(*self).parse_next(input)}}\n\nfn main(){}",
                true,
            ),
            (
                "abstract_literal_free",
                "trait Stream{type Slice;fn take(&mut self)->Self::Slice;}\ntrait Compare<T>{}\ntrait Parser<I,O,E>{fn parse_next(&mut self,input:&mut I)->Result<O,E>;}\nstruct Literal<L>(L);\nimpl<L,I:Stream+Compare<L>,E>Parser<I,I::Slice,E>for Literal<L>{fn parse_next(&mut self,input:&mut I)->Result<I::Slice,E>{Ok(input.take())}}\nfn literal<L,I,E>(value:L)->impl Parser<I,I::Slice,E>where I:Stream+Compare<L>{Literal(value)}\nfn run<'s,I:Stream+Compare<&'s str>,E>(value:&'s str,input:&mut I)->Result<I::Slice,E>{literal(value).parse_next(input)}\nfn main(){}\n",
                true,
            ),
            (
                "abstract_trace",
                "trait Stream{type Slice;fn take(&mut self)->Self::Slice;}\ntrait Compare<T>{}\ntrait Parser<I,O,E>{fn parse_next(&mut self,input:&mut I)->Result<O,E>;}\nstruct Literal<L>(L);\nimpl<L,I:Stream+Compare<L>,E>Parser<I,I::Slice,E>for Literal<L>{fn parse_next(&mut self,input:&mut I)->Result<I::Slice,E>{Ok(input.take())}}\nfn literal<L,I,E>(value:L)->impl Parser<I,I::Slice,E>where I:Stream+Compare<L>{Literal(value)}\n\n\nfn trace<I,O,E,P:Parser<I,O,E>>(p:P)->impl Parser<I,O,E>{p}fn run<I:Stream+Compare<&'static str>,E>(input:&mut I)->Result<I::Slice,E>{trace(\"alpha\").parse_next(input)}impl<'s,I:Stream+Compare<&'s str>,E>Parser<I,I::Slice,E>for &'s str{fn parse_next(&mut self,input:&mut I)->Result<I::Slice,E>{literal(*self).parse_next(input)}}fn main(){}",
                true,
            ),
            (
                "abstract_requires_static",
                "trait Stream{type Slice;fn take(&mut self)->Self::Slice;}\ntrait Compare<T>{}\ntrait Parser<I,O,E>{fn parse_next(&mut self,input:&mut I)->Result<O,E>;}\nstruct Literal<L>(L);\nimpl<L,I:Stream+Compare<L>,E>Parser<I,I::Slice,E>for Literal<L>{fn parse_next(&mut self,input:&mut I)->Result<I::Slice,E>{Ok(input.take())}}\nfn literal<L,I,E>(value:L)->impl Parser<I,I::Slice,E>where I:Stream+Compare<L>{Literal(value)}\n\n\nfn need<T:'static>(_:T){}fn run<I:Stream+Compare<()>>(input:&mut I){let result:Result<I::Slice,()>=literal(()).parse_next(input);need(result.unwrap());}fn main(){}",
                false,
            ),
            (
                "abstract_declared_static",
                "trait Stream{type Slice;fn take(&mut self)->Self::Slice;}\ntrait Compare<T>{}\ntrait Parser<I,O,E>{fn parse_next(&mut self,input:&mut I)->Result<O,E>;}\nstruct Literal<L>(L);\nimpl<L,I:Stream+Compare<L>,E>Parser<I,I::Slice,E>for Literal<L>{fn parse_next(&mut self,input:&mut I)->Result<I::Slice,E>{Ok(input.take())}}\nfn literal<L,I,E>(value:L)->impl Parser<I,I::Slice,E>where I:Stream+Compare<L>{Literal(value)}\n\n\nfn need<T:'static>(_:T){}fn run<I:Stream+Compare<()>>(input:&mut I)where I::Slice:'static{let result:Result<I::Slice,()>=literal(()).parse_next(input);need(result.unwrap());}fn main(){}",
                true,
            ),
            (
                "projected_borrowed_callback",
                "trait Stream{type Token;}struct Input<'a>(&'a u32);impl<'a>Stream for Input<'a>{type Token=&'a u32;}fn identity<T>(x:T)->T{x}fn apply<T,F:Fn(T)->T>(f:F,x:T)->T{f(x)}fn f<'a>(x:&'a u32)->&'a u32{apply(identity::<<Input<'_>as Stream>::Token>,x)}fn main(){}",
                true,
            ),
            (
                "projected_borrowed_callback_escape",
                "trait Stream{type Token;}struct Input<'a>(&'a u32);impl<'a>Stream for Input<'a>{type Token=&'a u32;}fn identity<T>(x:T)->T{x}fn apply<T,F:Fn(T)->T>(f:F,x:T)->T{f(x)}fn f<'a>(x:&'a u32)->&'static u32{apply(identity::<<Input<'_>as Stream>::Token>,x)}fn main(){}",
                false,
            ),
            (
                "selection_bound_lifetime",
                "trait Stream{type Token;fn take(&mut self)->Self::Token;}trait Parser<I,O>{fn parse(&mut self,input:&mut I)->O;}struct Run;impl<I:Stream>Parser<I,I::Token>for Run{fn parse(&mut self,input:&mut I)->I::Token{input.take()}}fn make<I:Stream>()->impl Parser<I,I::Token>{Run}trait Marker<'a>{}struct Input<'a,T>(&'a u32,core::marker::PhantomData<T>);impl<'a,T:Marker<'a>>Stream for Input<'a,T>{type Token=&'a u32;fn take(&mut self)->Self::Token{self.0}}fn f<'a,T:Marker<'a>>(input:&mut Input<'a,T>)->&'a u32{make().parse(input)}fn main(){}",
                true,
            ),
            (
                "selection_wrong_bound_lifetime",
                "trait Stream{type Token;fn take(&mut self)->Self::Token;}trait Parser<I,O>{fn parse(&mut self,input:&mut I)->O;}struct Run;impl<I:Stream>Parser<I,I::Token>for Run{fn parse(&mut self,input:&mut I)->I::Token{input.take()}}fn make<I:Stream>()->impl Parser<I,I::Token>{Run}trait Marker<'a>{}struct Input<'a,T>(&'a u32,core::marker::PhantomData<T>);impl<'a,T:Marker<'a>>Stream for Input<'a,T>{type Token=&'a u32;fn take(&mut self)->Self::Token{self.0}}fn f<'a,T:Marker<'static>>(input:&mut Input<'a,T>)->&'a u32{make().parse(input)}fn main(){}",
                false,
            ),
            (
                "ordinary_selection_bound",
                "trait Stream{type Token;fn take(&mut self)->Self::Token;}trait Parser<I,O>{fn parse(&mut self,input:&mut I)->O;}struct Run;impl<I:Stream>Parser<I,I::Token>for Run{fn parse(&mut self,input:&mut I)->I::Token{input.take()}}fn make<I:Stream>()->impl Parser<I,I::Token>{Run}trait Marker{}struct Input<'a,T>(&'a u32,core::marker::PhantomData<T>);impl<'a,T:Marker>Stream for Input<'a,T>{type Token=&'a u32;fn take(&mut self)->Self::Token{self.0}}fn f<'a,T:Marker>(input:&mut Input<'a,T>)->&'a u32{make().parse(input)}fn main(){}",
                true,
            ),
            (
                "ordinary_selection_bound_escape",
                "trait Stream{type Token;fn take(&mut self)->Self::Token;}trait Parser<I,O>{fn parse(&mut self,input:&mut I)->O;}struct Run;impl<I:Stream>Parser<I,I::Token>for Run{fn parse(&mut self,input:&mut I)->I::Token{input.take()}}fn make<I:Stream>()->impl Parser<I,I::Token>{Run}trait Marker{}struct Input<'a,T>(&'a u32,core::marker::PhantomData<T>);impl<'a,T:Marker>Stream for Input<'a,T>{type Token=&'a u32;fn take(&mut self)->Self::Token{self.0}}fn f<'a,T:Marker>(input:&mut Input<'a,T>)->&'static u32{make().parse(input)}fn main(){}",
                false,
            ),
            (
                "projection_pair",
                "trait Stream{type Token;fn take(&mut self)->Self::Token;}trait Parser<I,O>{fn parse(&mut self,input:&mut I)->O;}struct Run;impl<I:Stream>Parser<I,I::Token>for Run{fn parse(&mut self,input:&mut I)->I::Token{input.take()}}fn make<I:Stream>()->impl Parser<I,I::Token>{Run}struct Input<'a,'b>(&'a u32,&'b u32);impl<'a,'b>Stream for Input<'a,'b>{type Token=(&'b u32,&'a u32);fn take(&mut self)->Self::Token{(self.1,self.0)}}fn f<'a,'b>(input:&mut Input<'a,'b>)->(&'b u32,&'a u32){make().parse(input)}fn main(){}",
                true,
            ),
            (
                "projection_pair_swapped",
                "trait Stream{type Token;fn take(&mut self)->Self::Token;}trait Parser<I,O>{fn parse(&mut self,input:&mut I)->O;}struct Run;impl<I:Stream>Parser<I,I::Token>for Run{fn parse(&mut self,input:&mut I)->I::Token{input.take()}}fn make<I:Stream>()->impl Parser<I,I::Token>{Run}struct Input<'a,'b>(&'a u32,&'b u32);impl<'a,'b>Stream for Input<'a,'b>{type Token=(&'b u32,&'a u32);fn take(&mut self)->Self::Token{(self.1,self.0)}}fn f<'a,'b>(input:&mut Input<'a,'b>)->(&'a u32,&'b u32){make().parse(input)}fn main(){}",
                false,
            ),
            (
                "selection_static",
                "trait Stream{type Token;fn take(&mut self)->Self::Token;}trait Parser<I,O>{fn parse(&mut self,input:&mut I)->O;}struct Run;impl<I:Stream>Parser<I,I::Token>for Run{fn parse(&mut self,input:&mut I)->I::Token{input.take()}}fn make<I:Stream>()->impl Parser<I,I::Token>{Run}struct Input<'a>(&'a u32);impl<'a:'static>Stream for Input<'a>{type Token=&'a u32;fn take(&mut self)->Self::Token{self.0}}fn f(input:&mut Input<'static>)->&'static u32{make().parse(input)}fn main(){}",
                true,
            ),
            (
                "selection_requires_static",
                "trait Stream{type Token;fn take(&mut self)->Self::Token;}trait Parser<I,O>{fn parse(&mut self,input:&mut I)->O;}struct Run;impl<I:Stream>Parser<I,I::Token>for Run{fn parse(&mut self,input:&mut I)->I::Token{input.take()}}fn make<I:Stream>()->impl Parser<I,I::Token>{Run}struct Input<'a>(&'a u32);impl<'a:'static>Stream for Input<'a>{type Token=&'a u32;fn take(&mut self)->Self::Token{self.0}}fn f<'a>(input:&mut Input<'a>)->&'a u32{make().parse(input)}fn main(){}",
                false,
            ),
            (
                "projection_mut",
                "trait Stream{type Token;fn take(&mut self)->Self::Token;}trait Parser<I,O>{fn parse(&mut self,input:&mut I)->O;}struct Run;impl<I:Stream>Parser<I,I::Token>for Run{fn parse(&mut self,input:&mut I)->I::Token{input.take()}}fn make<I:Stream>()->impl Parser<I,I::Token>{Run}struct Input<'a>(Option<&'a mut &'a u32>);impl<'a>Stream for Input<'a>{type Token=&'a mut &'a u32;fn take(&mut self)->Self::Token{self.0.take().unwrap()}}fn f<'a>(input:&mut Input<'a>)->&'a mut &'a u32{make().parse(input)}fn main(){}",
                true,
            ),
            (
                "projection_mut_wrong",
                "trait Stream{type Token;fn take(&mut self)->Self::Token;}trait Parser<I,O>{fn parse(&mut self,input:&mut I)->O;}struct Run;impl<I:Stream>Parser<I,I::Token>for Run{fn parse(&mut self,input:&mut I)->I::Token{input.take()}}fn make<I:Stream>()->impl Parser<I,I::Token>{Run}struct Input<'a>(Option<&'a mut &'a u32>);impl<'a>Stream for Input<'a>{type Token=&'a mut &'a u32;fn take(&mut self)->Self::Token{self.0.take().unwrap()}}fn f<'a,'b>(input:&mut Input<'a>)->&'a mut &'b u32{make().parse(input)}fn main(){}",
                false,
            ),
            (
                "projected_identity",
                "trait Stream{type Token;}struct Input<'a>(&'a u32);impl<'a>Stream for Input<'a>{type Token=&'a u32;}fn identity<T>(x:T)->T{x}fn f<'a>(x:&'a u32)->&'a u32{identity::<<Input<'_>as Stream>::Token>(x)}fn main(){}",
                true,
            ),
            (
                "projected_identity_escape",
                "trait Stream{type Token;}struct Input<'a>(&'a u32);impl<'a>Stream for Input<'a>{type Token=&'a u32;}fn identity<T>(x:T)->T{x}fn f<'a>(x:&'a u32)->&'static u32{identity::<<Input<'_>as Stream>::Token>(x)}fn main(){}",
                false,
            ),
            (
                "projected_callback",
                "trait Stream{type Token;}struct Input;impl Stream for Input{type Token=&'static u32;}fn identity<T>(x:T)->T{x}fn apply<T,F:Fn(T)->T>(f:F,x:T)->T{f(x)}fn f(x:&'static u32)->&'static u32{apply(identity::<<Input as Stream>::Token>,x)}fn main(){}",
                true,
            ),
            (
                "projected_callback_escape",
                "trait Stream{type Token;}struct Input;impl Stream for Input{type Token=&'static u32;}fn identity<T>(x:T)->T{x}fn apply<T,F:Fn(T)->T>(f:F,x:T)->T{f(x)}fn f<'a>(x:&'a u32)->&'static u32{apply(identity::<<Input as Stream>::Token>,x)}fn main(){}",
                false,
            ),
            (
                "trait_lifetime",
                "trait Read<'a>{fn read(self,x:&'a u32)->&'a u32;}struct R;impl<'a> Read<'a> for R{fn read(self,x:&'a u32)->&'a u32{x}}fn make<'a>()->impl Read<'a>{R}fn use_it<'a>(x:&'a u32)->&'a u32{make().read(x)}fn main(){}",
                true,
            ),
            (
                "type_argument",
                "trait Read<T>{fn read(self,x:T)->T;}struct R;impl<T> Read<T> for R{fn read(self,x:T)->T{x}}fn make<'a>()->impl Read<&'a u32>{R}fn use_it<'a>(x:&'a u32)->&'a u32{make().read(x)}fn main(){}",
                true,
            ),
            (
                "forward_bound",
                "trait Read<'a>{}struct R;impl<'a> Read<'a> for R{}fn make<'a>()->impl Read<'a>{R}fn need<'a,T:Read<'a>>(_:T,_:&'a u32){}fn go(x:&u32){need(make(),x)}fn main(){}",
                true,
            ),
            (
                "hidden_wrong_lifetime",
                "trait Read<'a>{}struct R<'a>(&'a u32);impl<'a> Read<'a> for R<'a>{}fn make<'a,'b>(x:&'a u32)->impl Read<'b>{R(x)}fn main(){}",
                false,
            ),
            (
                "hidden_wrong_type_argument",
                "trait Read<T>{}struct R<'a>(&'a u32);impl<'a> Read<&'a u32> for R<'a>{}fn make<'a,'b>(x:&'a u32)->impl Read<&'b u32>{R(x)}fn main(){}",
                false,
            ),
            (
                "caller_wrong_lifetime",
                "trait Read<'a>{}struct R;impl<'a> Read<'a> for R{}fn make<'a>(_:&'a u32)->impl Read<'a>+use<'a>{R}fn need<T:Read<'static>>(_:T){}fn go(x:&u32){need(make(x))}fn main(){}",
                false,
            ),
            (
                "caller_wrong_type_argument",
                "trait Read<T>{}struct R;impl<T> Read<T> for R{}fn make<'a>(_:&'a u32)->impl Read<&'a u32>+use<'a>{R}fn need<T:Read<&'static u32>>(_:T){}fn go(x:&u32){need(make(x))}fn main(){}",
                false,
            ),
            (
                "associated_output",
                "trait Read<'a>{type Out;fn read(self)->Self::Out;}struct R<'a>(&'a u32);impl<'a> Read<'a> for R<'a>{type Out=&'a u32;fn read(self)->Self::Out{self.0}}fn make<'a>(x:&'a u32)->impl Read<'a,Out=&'a u32>{R(x)}fn go<'a>(x:&'a u32)->&'a u32{make(x).read()}fn main(){}",
                true,
            ),
            (
                "old_edition_capture",
                "trait Read<'a>{}struct R;impl<'a> Read<'a> for R{}fn make<'a>(_:&'a u32)->impl Read<'a>{R}fn main(){}",
                true,
            ),
            (
                "capture_escape",
                "trait Read<'a>{}struct R;impl<'a> Read<'a> for R{}fn make<'a>(_:&'a u32)->impl Read<'a>{R}fn main(){let p;{let x=4;p=make(&x);}let _=&p;}",
                false,
            ),
            (
                "borrowed_projection_parser",
                "trait Stream{type Token;fn take(&mut self)->Self::Token;}struct Input<'a>(&'a u32);impl<'a> Stream for Input<'a>{type Token=&'a u32;fn take(&mut self)->Self::Token{self.0}}trait Parser<I,O>{fn parse(&mut self,input:&mut I)->O;}struct Run;impl<I:Stream>Parser<I,I::Token>for Run{fn parse(&mut self,input:&mut I)->I::Token{input.take()}}fn make<I:Stream>()->impl Parser<I,I::Token>{Run}fn f<'a>(input:&mut Input<'a>)->&'a u32{make().parse(input)}fn main(){}",
                true,
            ),
            (
                "borrowed_projection_escape",
                "trait Stream{type Token;fn take(&mut self)->Self::Token;}struct Input<'a>(&'a u32);impl<'a> Stream for Input<'a>{type Token=&'a u32;fn take(&mut self)->Self::Token{self.0}}trait Parser<I,O>{fn parse(&mut self,input:&mut I)->O;}struct Run;impl<I:Stream>Parser<I,I::Token>for Run{fn parse(&mut self,input:&mut I)->I::Token{input.take()}}fn make<I:Stream>()->impl Parser<I,I::Token>{Run}fn f(input:&mut Input<'_>)->&'static u32{make().parse(input)}fn main(){}",
                false,
            ),
            (
                "scalar_projection_parser",
                "trait Stream{type Token;fn take(&mut self)->Self::Token;}impl Stream for u32{type Token=u8;fn take(&mut self)->u8{7}}trait Parser<I,O>{fn parse(&mut self,input:&mut I)->O;}struct Run;impl<I:Stream>Parser<I,I::Token>for Run{fn parse(&mut self,input:&mut I)->I::Token{input.take()}}fn make<I:Stream>()->impl Parser<I,I::Token>{Run}fn f(input:&mut u32)->u8{make().parse(input)}fn main(){}",
                true,
            ),
            (
                "receiver_capture",
                "trait Captures<'a>{}impl<'a,T> Captures<'a> for T{}struct S(u32);impl S{fn get(&self)->impl Iterator<Item=&u32>+Captures<'_>{core::iter::once(&self.0)}}fn main(){}",
                true,
            ),
            (
                "duplicate_erased_bounds",
                "trait Read<'a>{}struct R;impl<'a> Read<'a> for R{}fn make<'a,'b>()->impl Read<'a>+Read<'b>{R}fn need<'a,T:Read<'a>>(_:T,_:&'a u32){}fn go(x:&u32){need(make(),x)}fn main(){}",
                true,
            ),
            (
                "supertrait",
                "trait Base<T>{fn get(self,x:T)->T;}trait Read<T>:Base<T>{}struct R;impl<T> Base<T> for R{fn get(self,x:T)->T{x}}impl<T> Read<T> for R{}fn make<'a>()->impl Read<&'a u32>{R}fn need<'a,T:Base<&'a u32>>(p:T,x:&'a u32)->&'a u32{p.get(x)}fn go<'a>(x:&'a u32)->&'a u32{need(make(),x)}fn main(){}",
                true,
            ),
            (
                "mutable_type_argument",
                "trait Pass<T>{fn pass(self,x:T)->T;}struct R;impl<T> Pass<T> for R{fn pass(self,x:T)->T{x}}fn make<'a>()->impl Pass<&'a mut u32>{R}fn go<'a>(x:&'a mut u32)->&'a mut u32{make().pass(x)}fn main(){}",
                true,
            ),
            (
                "mutable_invariance",
                "trait Pass<T>{fn pass(self,x:T)->T;}struct R;impl<T> Pass<T> for R{fn pass(self,x:T)->T{x}}fn make<'a>()->impl Pass<&'a mut &'a u32>{R}fn go<'a,'b>(x:&'a mut &'b u32)->&'a mut &'b u32{make().pass(x)}fn main(){}",
                false,
            ),
            (
                "hidden_missing_bound",
                "trait Read<'a>{}struct R;fn make<'a>()->impl Read<'a>{R}fn main(){}",
                false,
            ),
            (
                "opaque_owner_output",
                "trait Read<'a,T>{type Out;}struct Reader;impl<'a,T:'a>Read<'a,T>for Reader{type Out=&'a T;}fn make<'a,T:'a>()->impl Read<'a,T,Out=&'a T>{Reader}fn need<'a,T:'a,R:Read<'a,T,Out=&'a T>>(_:R,_:&'a T){}fn main(){let n=7;need(make(),&n)}",
                true,
            ),
            (
                "opaque_owner_equivalent_duplicates",
                "trait Read<'a>{type Out;}struct Reader;impl<'a>Read<'a>for Reader{type Out=&'a u32;}fn make<'a>(_:&'a u32)->impl Read<'a,Out=&'a u32>+Read<'a,Out=&'a u32>{Reader}fn need<'a,R:Read<'a,Out=&'a u32>>(_:R,_:&'a u32){}fn f<'a>(a:&'a u32){need(make(a),a)}fn main(){}",
                true,
            ),
            (
                "opaque_owner_static_reject",
                "trait Read<'a>{type Out;}struct Reader;impl<'a>Read<'a>for Reader{type Out=&'a u32;}fn make<'a>(_:&'a u32)->impl Read<'a,Out=&'a u32>{Reader}fn need<'a,R:Read<'a,Out=&'static u32>>(_:R,_:&'a u32){}fn f<'a>(a:&'a u32){need(make(a),a)}fn main(){}",
                false,
            ),
            (
                "opaque_owner_output_permutation",
                "trait Read<'a,T>{type Out;}struct Reader;impl<'a,'b>Read<'a,&'b u32>for Reader{type Out=(&'b u32,&'a u32);}fn make<'a,'b>(_:&'a u32,_:&'b u32)->impl Read<'a,&'b u32,Out=(&'b u32,&'a u32)>{Reader}fn need<'a,'b,R:Read<'a,&'b u32,Out=(&'b u32,&'a u32)>>(_:R,_:&'a u32,_:&'b u32){}fn f<'a,'b>(a:&'a u32,b:&'b u32){need(make(a,b),a,b)}fn main(){}",
                true,
            ),
            (
                "opaque_owner_wrong_output_link",
                "trait Read<'a,T>{type Out;}struct Reader;impl<'a,'b>Read<'a,&'b u32>for Reader{type Out=(&'b u32,&'a u32);}fn make<'a,'b>(_:&'a u32,_:&'b u32)->impl Read<'a,&'b u32,Out=(&'b u32,&'a u32)>{Reader}fn need<'a,'b,R:Read<'a,&'b u32,Out=(&'a u32,&'b u32)>>(_:R,_:&'a u32,_:&'b u32)->(&'a u32,&'b u32){loop{}}fn f<'a,'b>(a:&'a u32,b:&'b u32)->(&'a u32,&'b u32){need(make(a,b),a,b)}fn main(){}",
                false,
            ),
            (
                "opaque_owner_mutable",
                "trait Read<'a>{type Out;fn get(self,v:&'a mut u32)->Self::Out;}struct Reader;impl<'a>Read<'a>for Reader{type Out=&'a mut u32;fn get(self,v:&'a mut u32)->Self::Out{v}}fn make<'a>()->impl Read<'a,Out=&'a mut u32>{Reader}fn need<'a,R:Read<'a,Out=&'a mut u32>>(r:R,v:&'a mut u32)->&'a mut u32{r.get(v)}fn f<'a>(v:&'a mut u32)->&'a mut u32{need(make(),v)}fn main(){}",
                true,
            ),
            (
                "opaque_owner_mutable_static_reject",
                "trait Read<'a>{type Out;fn get(self,v:&'a mut u32)->Self::Out;}struct Reader;impl<'a>Read<'a>for Reader{type Out=&'a mut u32;fn get(self,v:&'a mut u32)->Self::Out{v}}fn make<'a>()->impl Read<'a,Out=&'a mut u32>{Reader}fn need<'a,R:Read<'a,Out=&'a mut u32>>(r:R,v:&'a mut u32)->&'a mut u32{r.get(v)}fn f<'a>(v:&'a mut u32)->&'static mut u32{need(make(),v)}fn main(){}",
                false,
            ),
            (
                "opaque_owner_source_index_second",
                "trait Read<'a>{type Out;}trait Extra{}struct Reader;impl Extra for Reader{}impl<'a>Read<'a>for Reader{type Out=&'a u32;}fn make<'a>(_:&'a u32)->impl Extra+Read<'a,Out=&'a u32>{Reader}fn need<'a,R:Read<'a,Out=&'a u32>>(_:R,_:&'a u32){}fn f<'a>(a:&'a u32){need(make(a),a)}fn main(){}",
                true,
            ),
            (
                "opaque_owner_invariant_type_argument",
                "trait Read<'a,T>{type Out;}struct R;impl<'a,T>Read<'a,T>for R{type Out=();}fn make<'a>()->impl Read<'a,&'static u32,Out=()>{R}fn need<'a,'b,P:Read<'a,&'b u32,Out=()>>(_:P,_:&'a u32,_:&'b u32){}fn f<'a,'b>(a:&'a u32,b:&'b u32){need(make(),a,b)}fn main(){}",
                false,
            ),
            (
                "opaque_owner_invariant_mutable_type_argument",
                "trait Read<'a,T>{type Out;}struct R;impl<'a,T>Read<'a,T>for R{type Out=();}fn make<'a>()->impl Read<'a,&'static mut u32,Out=()>{R}fn need<'a,'b,P:Read<'a,&'b mut u32,Out=()>>(_:P,_:&'a u32,_:&'b mut u32){}fn f<'a,'b>(a:&'a u32,b:&'b mut u32){need(make(),a,b)}fn main(){}",
                false,
            ),
            (
                "ranked_opaque_fn_consumer",
                "fn make()->impl Fn(&u32)->u32{|v|*v}fn need<F:Fn(&u32)->u32>(f:F)->u32{f(&7)}fn main(){assert_eq!(need(make()),7);}",
                true,
            ),
            (
                "ranked_opaque_fnmut_consumer",
                "fn make()->impl for<'a>FnMut(&'a u32)->u32{let mut n=0;move|v:&u32|{n+=*v;n}}fn need<F:for<'a>FnMut(&'a u32)->u32>(mut f:F)->u32{f(&7)+f(&3)}fn main(){assert_eq!(need(make()),17);}",
                true,
            ),
            (
                "ranked_opaque_borrowed_output",
                "fn make()->impl for<'a>Fn(&'a u32)->&'a u32{|v|v}fn need<'a,F:for<'r>Fn(&'r u32)->&'r u32>(f:F,v:&'a u32)->&'a u32{f(v)}fn main(){let n=7;assert_eq!(*need(make(),&n),7);}",
                true,
            ),
            (
                "ranked_opaque_captured_output_reject",
                "fn make<'a>(data:&'a u32)->impl Fn(&u32)->&'a u32{move|_|data}fn need<F:for<'r>Fn(&'r u32)->&'r u32>(_:F){}fn f<'a>(data:&'a u32){need(make(data))}fn main(){}",
                false,
            ),
            (
                "ranked_opaque_wrong_static_output_reject",
                "fn make()->impl for<'a>Fn(&'a u32)->&'a u32{|v|v}fn need<F:for<'r>Fn(&'r u32)->&'static u32>(_:F){}fn main(){need(make());}",
                false,
            ),
            (
                "ranked_opaque_static_input_reject",
                "fn make()->impl Fn(&'static u32)->u32{|v|*v}fn need<F:Fn(&u32)->u32>(_:F){}fn main(){need(make());}",
                false,
            ),
            (
                "ranked_opaque_independent_inputs",
                "fn make()->impl Fn(&u32,&u32)->u32{|a,b|*a+*b}fn need<F:for<'a,'b>Fn(&'a u32,&'b u32)->u32>(f:F)->u32{f(&3,&4)}fn main(){assert_eq!(need(make()),7);}",
                true,
            ),
            (
                "ranked_opaque_shared_input_reject",
                "fn make()->impl for<'a>Fn(&'a u32,&'a u32)->u32{|a,b|*a+*b}fn need<F:for<'a,'b>Fn(&'a u32,&'b u32)->u32>(_:F){}fn main(){need(make());}",
                false,
            ),
        ] {
            if !case_filter::selected(name) {
                continue;
            }
            let path = root.join(format!("{name}.rs"));
            std::fs::write(&path, source).unwrap();
            let oracle = std::process::Command::new("rustc")
                .arg(format!("--edition={edition}"))
                .arg("--emit=metadata")
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
            let mut db = flux_hir::SourceDb::new();
            let root_file = db.add_with_edition(
                &path,
                source,
                if edition == "2021" {
                    flux_syntax::Edition::E2021
                } else {
                    flux_syntax::Edition::E2024
                },
            );
            let hir = flux_hir::lower(&mut db, root_file, flux_intern::Symbol::intern("main"));
            let mut errors = hir.diagnostics.clone();
            if errors.is_empty() {
                let typed = flux_mir::check_file(&hir);
                errors.extend(typed.diagnostics.clone());
                if errors.is_empty() {
                    errors.extend(flux_mir::build_file(&hir, &typed).diagnostics);
                }
            }
            assert_eq!(errors.is_empty(), accepted, "{edition}/{name}: {errors:?}");
        }
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn malformed_opaque_bounds_fail_closed() {
    let source = "trait Read<'a,T>{}struct R;impl<'a,T> Read<'a,T> for R{}fn make<'a>()->impl Read<'a,&'a u32>{R}fn main(){let _=make();}";
    let (_, _, hir) = flux_hir::lower_text("opaque-metadata.rs", source);
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let function = hir
        .items
        .functions
        .iter()
        .find(|(_, f)| f.name.as_str() == "make")
        .unwrap()
        .0;
    for corruption in 0..11 {
        let mut typed = flux_mir::check_file(&hir);
        assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
        let row = &mut typed.sigs.regions.fns.get_mut(function).unwrap().opaques[0];
        match corruption {
            0 => row.bounds = Box::default(),
            1 => row.bounds[0].origin = 999,
            2 => row.bounds[0].regions.lifetimes = Box::default(),
            3 => row.bounds[0].regions.args = Box::default(),
            4 => row.bounds[0].regions.args[0] = Box::default(),
            5 => row.bounds[0].regions.lifetimes[0] = flux_ty::regions::Region::Param(999),
            6 => row.bounds[0].predicate.self_ty = flux_ty::Ty::BOOL,
            7 => row.bounds[0].regions.lifetimes[0] = flux_ty::regions::Region::Late(0),
            8 => row.bounds[0].regions.lifetimes[0] = flux_ty::regions::Region::Late(999),
            9 => row.bounds[0].regions.lifetimes[0] = flux_ty::regions::Region::Erased,
            _ => typed.sigs.opaques.get_mut(function).unwrap()[0].bound_origins = Box::default(),
        }
        let mir = flux_mir::build_file(&hir, &typed);
        assert!(
            mir.diagnostics
                .iter()
                .any(|d| d.message.contains("opaque bound")),
            "{corruption}: {:?}",
            mir.diagnostics
        );
    }
}

#[test]
fn malformed_opaque_projection_rows_fail_closed() {
    let source = "trait Read{type Out;}struct R<'a>(&'a u32);impl<'a> Read for R<'a>{type Out=&'a u32;}fn make<'a>(x:&'a u32)->impl Read<Out=&'a u32>{R(x)}fn main(){}";
    let (_, _, hir) = flux_hir::lower_text("opaque-projection.rs", source);
    let function = hir
        .items
        .functions
        .iter()
        .find(|(_, f)| f.name.as_str() == "make")
        .unwrap()
        .0;
    for corruption in 0..4 {
        let mut typed = flux_mir::check_file(&hir);
        assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
        let row = &mut typed.sigs.regions.fns.get_mut(function).unwrap().opaques[0];
        row.projections[0] = match corruption {
            0 => Box::default(),
            1 => vec![flux_ty::regions::Region::Late(999)].into(),
            2 => vec![flux_ty::regions::Region::Param(999)].into(),
            _ => vec![flux_ty::regions::Region::Erased].into(),
        };
        let mir = flux_mir::build_file(&hir, &typed);
        assert!(
            mir.diagnostics
                .iter()
                .any(|d| d.message.contains("opaque bound")),
            "{corruption}: {:?}",
            mir.diagnostics
        );
    }
}

#[test]
fn opaque_distinct_owner_domains_remain_explicit() {
    let root =
        std::env::temp_dir().join(format!("flux-opaque-owner-domains-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let prefix = "trait Read<'a>{type Out;fn read(self,x:&'a u32)->Self::Out;}struct R;impl<'a>Read<'a>for R{type Out=&'a u32;fn read(self,x:&'a u32)->Self::Out{x}}";
    let use_ = "fn use_r<'a,T:Read<'a,Out=&'a u32>>(r:T,x:&'a u32)->&'a u32{r.read(x)}fn main(){let anchor=2;let short=3;assert_eq!(*use_r(make(&anchor),&short),3);}";
    for (name, source, rust_accepts) in [
        (
            "static_first",
            format!(
                "{prefix}fn make<'a>(_:&'a u32)->impl Read<'static,Out=&'static u32>+Read<'a,Out=&'a u32>{{R}}{use_}"
            ),
            true,
        ),
        (
            "static_last",
            format!(
                "{prefix}fn make<'a>(_:&'a u32)->impl Read<'a,Out=&'a u32>+Read<'static,Out=&'static u32>{{R}}{use_}"
            ),
            false,
        ),
        (
            "distinct_captures",
            format!(
                "{prefix}fn make<'a,'b>(_:&'a u32,_:&'b u32)->impl Read<'a,Out=&'a u32>+Read<'b,Out=&'b u32>{{R}}fn need<'a,T:Read<'a,Out=&'a u32>>(_:T,_:&'a u32){{}}fn f<'a,'b>(a:&'a u32,b:&'b u32){{need(make(a,b),b)}}fn main(){{}}"
            ),
            true,
        ),
        (
            "distinct_captures_reversed",
            format!(
                "{prefix}fn make<'a,'b>(_:&'a u32,_:&'b u32)->impl Read<'b,Out=&'b u32>+Read<'a,Out=&'a u32>{{R}}fn need<'a,T:Read<'a,Out=&'a u32>>(_:T,_:&'a u32){{}}fn f<'a,'b>(a:&'a u32,b:&'b u32){{need(make(a,b),a)}}fn main(){{}}"
            ),
            true,
        ),
    ] {
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, &source).unwrap();
        let oracle = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("oracle.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            oracle.status.success(),
            rust_accepts,
            "{name}: {}",
            String::from_utf8_lossy(&oracle.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text(&path, &source);
        assert!(hir.diagnostics.is_empty(), "{name}: {:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        assert!(
            typed.diagnostics.is_empty(),
            "{name}: {:?}",
            typed.diagnostics
        );
        let mir = flux_mir::build_file(&hir, &typed);
        assert!(
            mir.diagnostics
                .iter()
                .any(|d| d.message == "opaque associated output occurrence is ambiguous"),
            "{name}: {:?}",
            mir.diagnostics
        );
    }
}

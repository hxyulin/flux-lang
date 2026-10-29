//! Associated declaration equalities retain the caller's item lifetimes.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;
#[test]
fn caller_object_bounds_match_rustc() {
    let root =
        std::env::temp_dir().join(format!("flux-caller-object-bounds-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "associated_error_closure",
            "trait Convert{type Error;}fn f<T:Convert>(value:Result<u32,T::Error>)->Result<u32,Box<dyn std::error::Error+Send+Sync>>where T::Error:Into<Box<dyn std::error::Error+Send+Sync>>{value.map_err(|err|err.into())}fn main(){}",
            true,
        ),
        (
            "direct_error_closure",
            "fn f<E:Into<Box<dyn std::error::Error+Send+Sync>>>(value:Result<u32,E>)->Result<u32,Box<dyn std::error::Error+Send+Sync>>{value.map_err(|err|err.into())}fn main(){}",
            true,
        ),
        (
            "impl_error_closure",
            "trait Convert{type Error;}struct Parse<T>(T);impl<T:Convert>Parse<T>where T::Error:Into<Box<dyn std::error::Error+Send+Sync>>{fn f(&self,value:Result<u32,T::Error>)->Result<u32,Box<dyn std::error::Error+Send+Sync>>{value.map_err(|err|err.into())}}fn main(){}",
            true,
        ),
        (
            "try_into_error",
            "fn f<T:TryFrom<i64>>(value:i64)->Result<T,Box<dyn std::error::Error+Send+Sync>>where T::Error:std::error::Error+Send+Sync+'static{let value:Result<T,_>=value.try_into();value.map_err(|err|err.into())}fn main(){}",
            true,
        ),
        (
            "try_from_error",
            "fn f<T:TryFrom<i64>>(value:i64)->Result<T,Box<dyn std::error::Error+Send+Sync>>where T::Error:std::error::Error+Send+Sync+'static{let value:Result<T,_>=T::try_from(value);value.map_err(|err|err.into())}fn main(){}",
            true,
        ),
        (
            "try_into_impl",
            "struct P<T>(core::marker::PhantomData<T>);impl<T:TryFrom<i64>>P<T>where T::Error:std::error::Error+Send+Sync+'static{fn parse(&self,value:i64)->Result<T,Box<dyn std::error::Error+Send+Sync>>{let value:Result<T,_>=value.try_into();value.map_err(|err|err.into())}}fn main(){}",
            true,
        ),
        (
            "missing_static",
            "fn f<T:TryFrom<i64>>(value:i64)->Result<T,Box<dyn std::error::Error+Send+Sync>>where T::Error:std::error::Error+Send+Sync{let value:Result<T,_>=value.try_into();value.map_err(|err|err.into())}fn main(){}",
            false,
        ),
        (
            "missing_send",
            "fn f<T:TryFrom<i64>>(value:i64)->Result<T,Box<dyn std::error::Error+Send+Sync>>where T::Error:std::error::Error+Sync+'static{let value:Result<T,_>=value.try_into();value.map_err(|err|err.into())}fn main(){}",
            false,
        ),
        (
            "missing_sync",
            "fn f<T:TryFrom<i64>>(value:i64)->Result<T,Box<dyn std::error::Error+Send+Sync>>where T::Error:std::error::Error+Send+'static{let value:Result<T,_>=value.try_into();value.map_err(|err|err.into())}fn main(){}",
            false,
        ),
        (
            "missing_error",
            "fn f<T:TryFrom<i64>>(value:i64)->Result<T,Box<dyn std::error::Error+Send+Sync>>where T::Error:Send+Sync+'static{let value:Result<T,_>=value.try_into();value.map_err(|err|err.into())}fn main(){}",
            false,
        ),
        (
            "wrong_owner_argument",
            "fn f<T:TryFrom<i64>+TryFrom<i32>>(value:i32)->Result<T,Box<dyn std::error::Error+Send+Sync>>where <T as TryFrom<i64>>::Error:std::error::Error+Send+Sync+'static{let value:Result<T,_>=value.try_into();value.map_err(|err|err.into())}fn main(){}",
            false,
        ),
        (
            "unknown_owner",
            "trait Owner{type Error;}fn f<T:Owner>(value:Result<u32,T::Error>)->Result<u32,Box<dyn std::error::Error+Send+Sync>>{value.map_err(|err|err.into())}fn main(){}",
            false,
        ),
        (
            "inherited_parser",
            "trait Parser<I,O>{fn parse(&mut self,input:&mut I)->O;}trait Modal<I,O>:Parser<I,O>{}impl<I,O,P:Parser<I,O>>Modal<I,O>for P{}fn wrap<'a,O>(mut p:impl Modal<&'a str,O>)->impl FnMut(&mut &'a str)->O{move|input:&mut &'a str|p.parse(input)}fn main(){}",
            true,
        ),
        (
            "inherited_parser_direct",
            "trait Parser<I,O>{fn parse(&mut self,input:&mut I)->O;}trait Modal<I,O>:Parser<I,O>{}impl<I,O,P:Parser<I,O>>Modal<I,O>for P{}fn parse<'a,O,P:Modal<&'a str,O>>(mut p:P,input:&mut &'a str)->O{p.parse(input)}fn main(){}",
            true,
        ),
        (
            "inherited_parser_by_ref",
            "trait Parser<I,O>{fn parse(&mut self,input:&mut I)->O;fn by_ref(&mut self)->ByRef<'_,Self,I,O>where Self:Sized{ByRef{p:self,marker:core::marker::PhantomData}}}struct ByRef<'a,P,I,O>{p:&'a mut P,marker:core::marker::PhantomData<(I,O)>}impl<P:Parser<I,O>,I,O>Parser<I,O>for ByRef<'_,P,I,O>{fn parse(&mut self,input:&mut I)->O{self.p.parse(input)}}trait Modal<I,O>:Parser<I,O>{}impl<I,O,P:Parser<I,O>>Modal<I,O>for P{}fn wrap<'a,O>(mut p:impl Modal<&'a str,O>)->impl FnMut(&mut &'a str)->O{move|input:&mut &'a str|p.by_ref().parse(input)}fn main(){}",
            true,
        ),
        (
            "wrong_parser_input",
            "trait Parser<I,O>{fn parse(&mut self,input:&mut I)->O;}trait Modal<I,O>:Parser<I,O>{}impl<I,O,P:Parser<I,O>>Modal<I,O>for P{}fn wrap<'a,O>(mut p:impl Modal<&'a str,O>)->impl FnMut(&mut &'static str)->O{move|input:&mut &'static str|p.parse(input)}fn main(){}",
            false,
        ),
        (
            "wrong_parser_output",
            "trait Parser<I,O>{fn parse(&mut self,input:&mut I)->O;}trait Modal<I,O>:Parser<I,O>{}impl<I,O,P:Parser<I,O>>Modal<I,O>for P{}fn wrap<'a>(mut p:impl Modal<&'a str,&'a str>)->impl FnMut(&mut &'a str)->&'static str{move|input:&mut &'a str|p.parse(input)}fn main(){}",
            false,
        ),
        (
            "caller_lifetimes_separate",
            "trait Parser<I,O>{fn parse(&mut self,input:&mut I)->O;}trait Modal<I,O>:Parser<I,O>{}impl<I,O,P:Parser<I,O>>Modal<I,O>for P{}fn parse<'a,'b,P:Modal<&'b str,u32>+Modal<&'a str,u32>>(mut p:P,input:&mut &'a str)->u32{p.parse(input)}fn main(){}",
            false,
        ),
        (
            "generic_static_good",
            "fn require_static<T:'static>(value:T){drop(value);}fn forward<F:FnOnce()>(f:F){f();}fn make<T:'static>(value:T){let f=move||require_static(value);f();}fn main(){}",
            true,
        ),
        (
            "generic_static_missing",
            "fn require_static<T:'static>(value:T){drop(value);}fn forward<F:FnOnce()>(f:F){f();}fn make<T>(value:T){let f=move||require_static(value);f();}fn main(){}",
            false,
        ),
        (
            "unused_static_missing",
            "fn require_static<T:'static>(value:T){drop(value);}fn forward<F:FnOnce()>(f:F){f();}fn make<T>(value:T){let _f=move||require_static(value);}fn main(){}",
            false,
        ),
        (
            "nested_static_good",
            "fn require_static<T:'static>(value:T){drop(value);}fn forward<F:FnOnce()>(f:F){f();}fn make<T:'static>(value:T){let outer=move||{let inner=move||require_static(value);inner();};outer();}fn main(){}",
            true,
        ),
        (
            "nested_static_missing",
            "fn require_static<T:'static>(value:T){drop(value);}fn forward<F:FnOnce()>(f:F){f();}fn make<T>(value:T){let outer=move||{let inner=move||require_static(value);inner();};outer();}fn main(){}",
            false,
        ),
        (
            "forward_static_good",
            "fn require_static<T:'static>(value:T){drop(value);}fn forward<F:FnOnce()>(f:F){f();}fn make<T:'static>(value:T){forward(move||require_static(value));}fn main(){}",
            true,
        ),
        (
            "forward_static_missing",
            "fn require_static<T:'static>(value:T){drop(value);}fn forward<F:FnOnce()>(f:F){f();}fn make<T>(value:T){forward(move||require_static(value));}fn main(){}",
            false,
        ),
        (
            "parent_type_namespace",
            "fn require_static<T:'static>(value:T){drop(value);}fn forward<F:FnOnce()>(f:F){f();}fn make<A:'static,B>(a:A,b:B){forward(move||require_static(a));drop(b);}fn main(){}",
            true,
        ),
        (
            "captured_type_bound_good",
            "use core::marker::PhantomData;fn need<'a,T:'a>(_:PhantomData<&'a mut &'a()>,_:T){}fn make<'a,T:'a>(value:T){let tag:PhantomData<&'a mut &'a()>=PhantomData;let f=move||need(tag,value);f();}fn main(){}",
            true,
        ),
        (
            "captured_type_bound_missing",
            "use core::marker::PhantomData;fn need<'a,T:'a>(_:PhantomData<&'a mut &'a()>,_:T){}fn make<'a,T>(value:T){let tag:PhantomData<&'a mut &'a()>=PhantomData;let f=move||need(tag,value);f();}fn main(){}",
            false,
        ),
        (
            "conditional_impl_good",
            "trait Conditional{fn consume(self);}struct Hold<T>(T);impl<T:'static>Conditional for Hold<T>{fn consume(self){drop(self);}}fn make<T:'static>(value:T){let f=move||Hold(value).consume();f();}fn main(){}",
            true,
        ),
        (
            "conditional_impl_missing",
            "trait Conditional{fn consume(self);}struct Hold<T>(T);impl<T:'static>Conditional for Hold<T>{fn consume(self){drop(self);}}fn make<T>(value:T){let f=move||Hold(value).consume();f();}fn main(){}",
            false,
        ),
        (
            "wrong_parent_type_namespace",
            "fn require_static<T:'static>(value:T){drop(value);}fn forward<F:FnOnce()>(f:F){f();}fn make<A,B:'static>(a:A,b:B){forward(move||require_static(a));drop(b);}fn main(){}",
            false,
        ),
        (
            "captured_parent_namespace",
            "use core::marker::PhantomData;fn need<'a,T:'a>(_:PhantomData<&'a mut &'a()>,_:T){}fn forward<F:FnOnce()>(f:F){f();}fn make<'a,B,A:'a>(b:B,a:A){let tag:PhantomData<&'a mut &'a()>=PhantomData;forward(move||need(tag,a));drop(b);}fn main(){}",
            true,
        ),
        (
            "captured_wrong_parent_namespace",
            "use core::marker::PhantomData;fn need<'a,T:'a>(_:PhantomData<&'a mut &'a()>,_:T){}fn forward<F:FnOnce()>(f:F){f();}fn make<'a,B:'a,A>(b:B,a:A){let tag:PhantomData<&'a mut &'a()>=PhantomData;forward(move||need(tag,a));drop(b);}fn main(){}",
            false,
        ),
        (
            "nested_captured_bound_good",
            "use core::marker::PhantomData;fn need<'a,T:'a>(_:PhantomData<&'a mut &'a()>,_:T){}fn forward<F:FnOnce()>(f:F){f();}fn make<'a,T:'a>(value:T){let tag:PhantomData<&'a mut &'a()>=PhantomData;forward(move||{forward(move||need(tag,value));});}fn main(){}",
            true,
        ),
        (
            "nested_captured_bound_missing",
            "use core::marker::PhantomData;fn need<'a,T:'a>(_:PhantomData<&'a mut &'a()>,_:T){}fn forward<F:FnOnce()>(f:F){f();}fn make<'a,T>(value:T){let tag:PhantomData<&'a mut &'a()>=PhantomData;forward(move||{forward(move||need(tag,value));});}fn main(){}",
            false,
        ),
        (
            "direct_conditional_impl_good",
            "trait Conditional{fn consume(self);}struct Hold<T>(T);impl<T:'static>Conditional for Hold<T>{fn consume(self){drop(self);}}fn make<T:'static>(value:T){Hold(value).consume();}fn main(){}",
            true,
        ),
        (
            "direct_conditional_impl_missing",
            "trait Conditional{fn consume(self);}struct Hold<T>(T);impl<T:'static>Conditional for Hold<T>{fn consume(self){drop(self);}}fn make<T>(value:T){Hold(value).consume();}fn main(){}",
            false,
        ),
        (
            "alias_conditional_impl_missing",
            "trait Conditional{fn consume(self);}struct Hold<T>(T);impl<T:'static>Conditional for Hold<T>{fn consume(self){drop(self);}}type Alias<T>=Hold<T>;fn make<T>(value:T){let hold:Alias<T>=Hold(value);let f=move||hold.consume();f();}fn main(){}",
            false,
        ),
        (
            "nested_conditional_impl_good",
            "trait Conditional{fn consume(self);}struct Hold<T>(T);impl<T:'static>Conditional for Hold<T>{fn consume(self){drop(self);}}trait Outer{fn outer(self);}struct Wrap<P>(P);impl<P:Conditional>Outer for Wrap<P>{fn outer(self){self.0.consume();}}fn make<T:'static>(value:T){let f=move||Wrap(Hold(value)).outer();f();}fn main(){}",
            true,
        ),
        (
            "nested_conditional_impl_missing",
            "trait Conditional{fn consume(self);}struct Hold<T>(T);impl<T:'static>Conditional for Hold<T>{fn consume(self){drop(self);}}trait Outer{fn outer(self);}struct Wrap<P>(P);impl<P:Conditional>Outer for Wrap<P>{fn outer(self){self.0.consume();}}fn make<T>(value:T){let f=move||Wrap(Hold(value)).outer();f();}fn main(){}",
            false,
        ),
        (
            "raw_return_loan",
            "use core::ptr::NonNull;unsafe fn f<'a,T>(p:Option<NonNull<T>>,_:&'a mut T)->Option<&'a mut T>{p.map(|mut p|unsafe{p.as_mut()})}fn main(){}",
            true,
        ),
        (
            "nested_raw_return_loan",
            "use core::ptr::NonNull;unsafe fn f<'a,T>(p:[Option<NonNull<T>>;2],_:&'a mut T)->[Option<&'a mut T>;2]{p.map(|p|p.map(|mut p|unsafe{p.as_mut()}))}fn main(){}",
            true,
        ),
        (
            "raw_return_escapes_type",
            "use core::ptr::NonNull;unsafe fn f<'a>(p:NonNull<&'a u8>)->&'static &'a u8{Some(p).map(|mut p|unsafe{p.as_mut()}).unwrap()}fn main(){let x=1;let r=&x;let p=NonNull::from(&r);let _=unsafe{f(p)};}",
            false,
        ),
        (
            "returned_raw_closure",
            "use core::ptr::NonNull;unsafe fn f<'a,T:'a>(mut p:NonNull<T>)->impl FnOnce()->&'a mut T{move||unsafe{p.as_mut()}}fn main(){}",
            true,
        ),
        (
            "unused_static_raw_closure",
            "use core::ptr::NonNull;fn f<'a>(p:NonNull<&'a u8>){let _c=move||->&'static &'a u8{unsafe{p.as_ref()}};}fn main(){}",
            false,
        ),
        (
            "param_blanket_dynamic",
            "trait A{fn a(&self);}trait D{fn boxed(&self)->Box<dyn D>;}impl<T:A+Clone+'static>D for T{fn boxed(&self)->Box<dyn D>{Box::new(self.clone())}}fn main(){}",
            true,
        ),
        (
            "param_associated_blanket",
            "trait Typed:Clone+Send+Sync+'static{type Value:Clone+Send+Sync+'static;}trait Any{}impl<T:Clone+Send+Sync+'static,P:Typed<Value=T>>Any for P{}fn new<P:Typed>(p:P)->Box<dyn Any>{Box::new(p)}fn main(){}",
            true,
        ),
        (
            "param_blanket_static_missing",
            "trait A{}trait D{}impl<T:A+'static>D for T{}fn f<T:A>(t:T)->Box<dyn D>{Box::new(t)}fn main(){}",
            false,
        ),
        (
            "param_blanket_marker_missing",
            "trait A{}trait D{}impl<T:A+Send>D for T{}fn f<T:A+'static>(t:T)->Box<dyn D>{Box::new(t)}fn main(){}",
            false,
        ),
        (
            "param_blanket_equality_missing",
            "trait A{type Item;}trait D{}impl<T:A<Item=u8>>D for T{}fn f<T:A+'static>(t:T)->Box<dyn D>{Box::new(t)}fn main(){}",
            false,
        ),
        (
            "param_blanket_wrong_arg",
            "trait A<T>{}trait D{}impl<T:A<u8>>D for T{}fn f<T:A<u16>+'static>(t:T)->Box<dyn D>{Box::new(t)}fn main(){}",
            false,
        ),
        (
            "param_blanket_inductive_cycle",
            "trait A{}trait D{}impl<T:A>D for T{}impl<T:D>A for T{}fn f<T:'static>(t:T)->Box<dyn D>{Box::new(t)}fn main(){}",
            false,
        ),
        (
            "param_blanket_ambiguous",
            "trait A{}trait B{}trait D{}impl<T:A>D for T{}impl<T:B>D for T{}fn f<T:A+B+'static>(t:T)->Box<dyn D>{Box::new(t)}fn main(){}",
            false,
        ),
        (
            "param_projection_owner_missing",
            "trait Typed{type Value;}trait Any{}impl<T:Clone,P:Typed<Value=T>>Any for P{}fn new<P:'static>(p:P)->Box<dyn Any>{Box::new(p)}fn main(){}",
            false,
        ),
        (
            "param_projection_nonplain_missing",
            "trait Typed{type Value;}trait Any{}impl<T:Clone,P:Typed<Value=Option<T>>>Any for P{}fn new<P:Typed+'static>(p:P)->Box<dyn Any>{Box::new(p)}fn main(){}",
            false,
        ),
        (
            "param_projection_two_args",
            "trait Typed{type A:Clone;type B:Copy;}trait Any{}impl<A:Clone,B:Copy,P:Typed<A=A,B=B>>Any for P{}fn new<P:Typed+'static>(p:P)->Box<dyn Any>{Box::new(p)}fn main(){}",
            true,
        ),
        (
            "param_projection_marker_missing",
            "trait Typed{type Value;}trait Any{}impl<T:Send,P:Typed<Value=T>>Any for P{}fn new<P:Typed+'static>(p:P)->Box<dyn Any>{Box::new(p)}fn main(){}",
            false,
        ),
        (
            "param_projection_wrong_equality",
            "trait Typed{type Value;}trait Any{}impl<T:Clone,P:Typed<Value=T>>Any for P{}fn new<P:Typed<Value=std::rc::Rc<u32>>+'static>(p:P)->Box<dyn Any+Send>{Box::new(p)}fn main(){}",
            false,
        ),
        (
            "generic_scope_owned_capture",
            "fn run<T:Send>(value:T){std::thread::scope(|scope|{scope.spawn(move||drop(value)).join().unwrap();});}fn main(){let n=3;run(&n);}",
            true,
        ),
        (
            "generic_spawn_static_missing",
            "fn run<T:Send>(value:T){std::thread::spawn(move||drop(value)).join().unwrap();}fn main(){}",
            false,
        ),
        (
            "closure_output_escape",
            "fn f<'a>(v:&'a u32)->&'static u32{let c=move||v;c()}fn main(){}",
            false,
        ),
        (
            "unused_closure_short_reborrow",
            "fn need<'a,T:'a>(_:&'a (),_:T){}fn make<T>(v:T){let c=move|loan:&()|need(loan,v);let _=c;}fn main(){}",
            true,
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
        let mut errors = hir.diagnostics.clone();
        if errors.is_empty() {
            let typed = flux_mir::check_file(&hir);
            if matches!(
                name,
                "missing_static"
                    | "generic_static_missing"
                    | "unused_static_missing"
                    | "nested_static_missing"
                    | "forward_static_missing"
                    | "captured_type_bound_missing"
                    | "conditional_impl_missing"
                    | "wrong_parent_type_namespace"
                    | "captured_wrong_parent_namespace"
                    | "nested_captured_bound_missing"
            ) {
                assert!(
                    typed.diagnostics.is_empty(),
                    "{name}: negative must reach MIR: {:?}",
                    typed.diagnostics
                );
            }
            errors.extend(typed.diagnostics.clone());
            if errors.is_empty() {
                errors.extend(flux_mir::build_file(&hir, &typed).diagnostics);
            }
        }
        assert_eq!(errors.is_empty(), accepted, "{name}: {errors:?}");
    }
}

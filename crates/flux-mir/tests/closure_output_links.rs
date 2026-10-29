//! Closure Output proofs retain the original callable input rows.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code)]
mod case_filter;
#[test]
fn closure_output_links_match_rustc() {
    let root =
        std::env::temp_dir().join(format!("flux-closure-output-links-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "fixed_to_static_reject",
            "fn consume<'a,F:Fn(&'a str)->&'static str>(p:&'a str,f:F)->&'static str{f(p)}fn bad<'a>(s:&'a str)->&'static str{consume(s,|v|v)}fn main(){}",
            false,
        ),
        (
            "fixed_identity",
            "fn consume<'a,F:Fn(&'a str)->&'a str>(p:&'a str,f:F)->&'a str{f(p)}fn good<'a>(s:&'a str)->&'a str{consume(s,|v|v)}fn main(){}",
            true,
        ),
        (
            "fixed_static_value",
            "fn consume<'a,F:Fn(&'a str)->&'static str>(p:&'a str,f:F)->&'static str{f(p)}fn good<'a>(s:&'a str)->&'static str{consume(s,|_|\"fixed\")}fn main(){}",
            true,
        ),
        (
            "fixed_capture_static_reject",
            "fn consume<'a,F:Fn(&'a str)->&'static str>(p:&'a str,f:F)->&'static str{f(p)}fn bad<'a>(s:&'a str)->&'static str{consume(s,|_|s)}fn main(){}",
            false,
        ),
        (
            "two_inputs_wrong_link",
            "fn consume<'a,'b,F:Fn(&'a str,&'b str)->&'b str>(a:&'a str,b:&'b str,f:F)->&'b str{f(a,b)}fn bad<'a,'b>(a:&'a str,b:&'b str)->&'b str{consume(a,b,|x,_|x)}fn main(){}",
            false,
        ),
        (
            "two_inputs_correct_link",
            "fn consume<'a,'b,F:Fn(&'a str,&'b str)->&'b str>(a:&'a str,b:&'b str,f:F)->&'b str{f(a,b)}fn good<'a,'b>(a:&'a str,b:&'b str)->&'b str{consume(a,b,|_,y|y)}fn main(){}",
            true,
        ),
        (
            "hrtb_to_static_reject",
            "fn consume<F:for<'a>Fn(&'a str)->&'static str>(f:F){}fn main(){consume(|v|v);}",
            false,
        ),
        (
            "hrtb_identity",
            "fn consume<F:for<'a>Fn(&'a str)->&'a str>(f:F){}fn main(){consume(|v|v);}",
            true,
        ),
        (
            "hrtb_static_value",
            "fn consume<F:for<'a>Fn(&'a str)->&'static str>(f:F){}fn main(){consume(|_|\"fixed\");}",
            true,
        ),
        (
            "fixed_fnmut_reject",
            "fn consume<'a,F:FnMut(&'a str)->&'static str>(p:&'a str,mut f:F)->&'static str{f(p)}fn bad<'a>(s:&'a str)->&'static str{consume(s,|v|v)}fn main(){}",
            false,
        ),
        (
            "fixed_fnonce_reject",
            "fn consume<'a,F:FnOnce(&'a str)->&'static str>(p:&'a str,f:F)->&'static str{f(p)}fn bad<'a>(s:&'a str)->&'static str{consume(s,|v|v)}fn main(){}",
            false,
        ),
        (
            "nonopaque_first_callback_reject",
            "fn consume<'a,P:Fn()->&'a str,F:Fn(&'a str)->&'static str>(p:P,f:F)->&'static str{f(p())}fn bad<'a>(s:&'a str)->&'static str{consume(move||s,|v|v)}fn main(){}",
            false,
        ),
        (
            "opaque_first_callback_reject",
            "fn make<'a>(s:&'a str)->impl Fn()->&'a str{move||s}fn consume<'a,P:Fn()->&'a str,F:Fn(&'a str)->&'static str>(p:P,f:F)->&'static str{f(p())}fn bad<'a>(s:&'a str)->&'static str{consume(make(s),|v|v)}fn main(){}",
            false,
        ),
        (
            "opaque_first_callback_good",
            "fn make<'a>(s:&'a str)->impl Fn()->&'a str{move||s}fn consume<'a,P:Fn()->&'a str,F:Fn(&'a str)->&'a str>(p:P,f:F)->&'a str{f(p())}fn good<'a>(s:&'a str)->&'a str{consume(make(s),|v|v)}fn main(){}",
            true,
        ),
        (
            "alias_static_reject",
            "type R<'a>=&'a u32;fn consume<'a,F:Fn(R<'a>)->R<'static>>(p:R<'a>,f:F)->R<'static>{f(p)}fn bad<'a>(s:R<'a>)->R<'static>{consume(s,|v|v)}fn main(){}",
            false,
        ),
        (
            "repeated_identical",
            "fn consume<'a,F:Fn(&'a str)->&'a str+Fn(&'a str)->&'a str>(s:&'a str,f:F)->&'a str{f(s)}fn good<'a>(s:&'a str)->&'a str{consume(s,|v|v)}fn main(){}",
            true,
        ),
        (
            "different_fixed_output_reject",
            "fn consume<'a,'b,F:Fn(&'a str)->&'a str+Fn(&'b str)->&'b str>(_:F){}fn main(){consume(|v|v);}",
            false,
        ),
        (
            "nested_capture_output_reject",
            "fn consume<'a,F:Fn(&'a str)->&'static str>(s:&'a str,f:F)->&'static str{f(s)}fn bad<'a>(s:&'a str)->&'static str{let c=||consume(s,|v|v);c()}fn main(){}",
            false,
        ),
        (
            "different_inputs_same_static",
            "fn both<'a,'b,F:Fn(&'a str)->&'static str+Fn(&'b str)->&'static str>(_:F){}fn main(){both(|_|\"x\");}",
            false,
        ),
        (
            "different_inputs_same_fixed",
            "fn both<'a,'b,F:Fn(&'a str)->&'a str+Fn(&'b str)->&'a str>(_:F){}fn main(){both(|_|\"x\");}",
            false,
        ),
        (
            "unused_different_output_rows",
            "fn unused<'a,'b,F:Fn(&'a str)->&'a str+Fn(&'b str)->&'b str>(_:F){}fn main(){}",
            false,
        ),
        (
            "independent_parameter_same_closure",
            "fn consume<'a,'b,P:Fn(&'a str)->&'a str,Q:Fn(&'b str)->&'b str>(p:P,q:Q,a:&'a str,b:&'b str)->(&'a str,&'b str){(p(a),q(b))}fn share<'a,'b,F:for<'r>Fn(&'r str)->&'r str+Copy>(f:F,a:&'a str,b:&'b str)->(&'a str,&'b str){consume(f,f,a,b)}fn main(){let a=String::from(\"a\");let b=String::from(\"b\");assert_eq!(share(|v|v,&a,&b),(\"a\",\"b\"));}",
            true,
        ),
        (
            "unit_output",
            "fn consume<'a,F:Fn(&'a str)>(s:&'a str,f:F){f(s)}fn main(){consume(\"unit\",|_|());}",
            true,
        ),
        (
            "scalar_output",
            "fn consume<'a,F:Fn(&'a str)->usize>(s:&'a str,f:F)->usize{f(s)}fn main(){assert_eq!(consume(\"abc\",|v|v.len()),3);}",
            true,
        ),
        (
            "alias_normalized_no_call",
            "trait Out{type T;}fn f<'a,'b,F,U:Out<T=F>>(_:F)where F:Fn(&'a str)->&'a str,U::T:Fn(&'b str)->&'b str{}fn main(){}",
            false,
        ),
        (
            "alias_normalized_reverse",
            "trait Out{type T;}fn f<'a,'b,F,U:Out<T=F>>(_:F)where U::T:Fn(&'b str)->&'b str,F:Fn(&'a str)->&'a str{}fn main(){}",
            false,
        ),
        (
            "alias_identical_rows",
            "trait Out{type T;}fn f<'a,F,U:Out<T=F>>(_:F)where F:Fn(&'a str)->&'a str,U::T:Fn(&'a str)->&'a str{}fn main(){}",
            true,
        ),
        (
            "alias_independent_original_binders",
            "trait Out{type T;}fn f<'a,'b,F,U:Out>(_:F)where F:Fn(&'a str)->&'a str,U::T:Fn(&'b str)->&'b str{}fn main(){}",
            true,
        ),
        (
            "caller_callee_index_collision",
            "trait Out{type T;}fn callee<'a,'b,U:Out,F>(_:F,_:U::T)where F:Fn(&'a str)->&'a str,U::T:Fn(&'b str)->&'b str{}fn caller<'a,'b,U:Out<T=F>,F>(f:F)where F:for<'r>Fn(&'r str)->&'r str+Copy{callee::<U,F>(f,f);}fn main(){}",
            true,
        ),
        (
            "caller_callee_output_collision_reject",
            "trait Out{type T;}fn callee<'a,'b,U:Out,F>(_:F,_:U::T,_:&'b str)where F:Fn(&'a str)->&'a str,U::T:Fn(&'b str)->&'static str{}fn caller<'a,'b,U:Out<T=F>,F>(f:F,s:&'b str)where F:for<'r>Fn(&'r str)->&'r str+Copy{callee::<U,F>(f,f,s);}fn main(){}",
            false,
        ),
        (
            "alias_supertrait_normalized_no_call",
            "trait Out{type T;}trait Has<F>:Out<T=F>{}fn f<'a,'b,F,U:Has<F>>(_:F)where F:Fn(&'a str)->&'a str,U::T:Fn(&'b str)->&'b str{}fn main(){}",
            false,
        ),
        (
            "abstract_param_static_reject",
            "fn need<'a,F:Fn(&'a str)->&'static str>(f:F,s:&'a str)->&'static str{f(s)}fn caller<'a,F:for<'r>Fn(&'r str)->&'r str>(f:F,s:&'a str)->&'static str{need(f,s)}fn main(){}",
            false,
        ),
        (
            "abstract_param_same_fixed",
            "fn need<'a,F:Fn(&'a str)->&'a str>(f:F,s:&'a str)->&'a str{f(s)}fn caller<'a,F:Fn(&'a str)->&'a str>(f:F,s:&'a str)->&'a str{need(f,s)}fn main(){}",
            true,
        ),
        (
            "abstract_fixed_not_universal",
            "fn need<F:for<'r>Fn(&'r str)->&'r str>(_:F){}fn caller<'a,F:Fn(&'a str)->&'a str>(f:F){need(f)}fn main(){}",
            false,
        ),
        (
            "function_item_static_reject",
            "fn id(s:&str)->&str{s}fn need<'a,F:Fn(&'a str)->&'static str>(f:F,s:&'a str)->&'static str{f(s)}fn caller<'a>(s:&'a str)->&'static str{need(id,s)}fn main(){}",
            false,
        ),
        (
            "function_pointer_static_reject",
            "fn id(s:&str)->&str{s}fn need<'a,F:Fn(&'a str)->&'static str>(f:F,s:&'a str)->&'static str{f(s)}fn caller<'a>(s:&'a str)->&'static str{let p:for<'r>fn(&'r str)->&'r str=id;need(p,s)}fn main(){}",
            false,
        ),
        (
            "function_pointer_static_good",
            "fn fixed(_:&str)->&'static str{\"fixed\"}fn need<'a,F:Fn(&'a str)->&'static str>(f:F,s:&'a str)->&'static str{f(s)}fn caller<'a>(s:&'a str)->&'static str{let p:fn(&str)->&'static str=fixed;need(p,s)}fn main(){}",
            true,
        ),
        (
            "dynamic_scalar_forwarding",
            "fn need<F:Fn(&u64)->u64>(f:F)->u64{f(&3)}fn pass(f:&dyn Fn(&u64)->u64)->u64{need(f)}fn main(){assert_eq!(pass(&|v|*v+2),5);}",
            true,
        ),
        (
            "pointer_unsafe_remains_rejected",
            "unsafe fn id(s:&str)->&str{s}fn need<F:Fn(&str)->&str>(_:F){}fn main(){let p:unsafe fn(&str)->&str=id;need(p);}",
            false,
        ),
        (
            "item_static_output_not_tied",
            "fn get(_: &u32)->&'static u32{&7}\nfn need<F:for<'a>Fn(&'a u32)->&'a u32>(_:F){}\nfn main(){need(get);}\n",
            false,
        ),
        (
            "item_independent_output_not_tied",
            "fn get<'a>(_:&u32)->&'a u32{&7}\nfn need<F:for<'r>Fn(&'r u32)->&'r u32>(_:F){}\nfn main(){need(get);}\n",
            false,
        ),
        (
            "pointer_independent_output_covariance",
            "fn get<'a>(_:&u32)->&'a u32{&7}\nfn main(){let _:for<'r>fn(&'r u32)->&'r u32=get;}\n",
            true,
        ),
        (
            "abstract_static_output_not_tied",
            "fn need<F:for<'a>Fn(&'a u32)->&'a u32>(_:F){}fn caller<F:for<'a>Fn(&'a u32)->&'static u32>(f:F){need(f)}fn main(){}",
            false,
        ),
        (
            "abstract_wrong_universal_output",
            "fn need<F:for<'a,'b>Fn(&'a u32,&'b u32)->&'a u32>(_:F){}fn caller<F:for<'r,'s>Fn(&'r u32,&'s u32)->&'s u32>(f:F){need(f)}fn main(){}",
            false,
        ),
        (
            "abstract_shared_unit_inputs_reject",
            "fn need<F:for<'a,'b>Fn(&'a u32,&'b u32)->()>(_:F){}fn caller<F:for<'r>Fn(&'r u32,&'r u32)->()>(f:F){need(f)}fn main(){}",
            false,
        ),
        (
            "abstract_shared_return_inputs_reject",
            "fn need<F:for<'a,'b>Fn(&'a u32,&'b u32)->&'a u32>(_:F){}fn caller<F:for<'r>Fn(&'r u32,&'r u32)->&'r u32>(f:F){need(f)}fn main(){}",
            false,
        ),
        (
            "abstract_independent_input_binders",
            "fn need<F:for<'a,'b>Fn(&'a u32,&'b u32)->&'a u32>(_:F){}fn caller<F:for<'r,'s>Fn(&'r u32,&'s u32)->&'r u32>(f:F){need(f)}fn main(){}",
            true,
        ),
        (
            "pointer_static_output_covariance",
            "fn get(_:&u32)->&'static u32{&7}fn main(){let _:for<'r>fn(&'r u32)->&'r u32=get;}",
            true,
        ),
        (
            "pointer_captured_region_not_universal",
            "fn need<F:for<'r>Fn(&'r u32)->&'r u32>(_:F){}fn caller<'a>(p:fn(&'a u32)->&'a u32){need(p)}fn main(){}",
            false,
        ),
        (
            "generic_identity_type_argument_not_universal",
            "fn id<T>(x:T)->T{x}\nfn need<F:for<'r>Fn(&'r u32)->&'r u32>(_:F){}\nfn main(){need(id);}\n",
            false,
        ),
        (
            "generic_borrow_parameter_universal",
            "fn get<T>(x:&T)->&T{x}fn need<F:for<'r>Fn(&'r u32)->&'r u32>(_:F){}fn main(){need(get);}",
            true,
        ),
        (
            "generic_borrow_nested_fixed_type",
            "fn get<T>(x:&T)->&T{x}fn need<'a,F:for<'r>Fn(&'r &'a u32)->&'r &'a u32>(_:&'a u32,_:F){}fn caller<'a>(data:&'a u32){need(data,get)}fn main(){}",
            true,
        ),
        (
            "pointer_nested_fixed_data",
            "fn need<'a,F:for<'r>Fn(&'r &'a u32)->&'r &'a u32>(_:&'a u32,_:F){}fn caller<'a>(data:&'a u32,p:for<'r>fn(&'r &'a u32)->&'r &'a u32){need(data,p)}fn main(){}",
            true,
        ),
        (
            "borrowed_identity_closure",
            "fn typed<F:for<'r>Fn(&'r u32)->&'r u32>(f:F)->F{f}fn need<F:for<'r>Fn(&'r u32)->&'r u32>(f:F)->u32{let n=3;*f(&n)}fn main(){let c=typed(|v|v);assert_eq!(need(&c),3);}",
            true,
        ),
        (
            "borrowed_static_body_closure",
            "fn typed<F:for<'r>Fn(&'r u32)->&'r u32>(f:F)->F{f}fn need<F:for<'r>Fn(&'r u32)->&'r u32>(f:F)->u32{let n=3;*f(&n)}fn main(){let c=typed(|_|&7);assert_eq!(need(&c),7);}",
            true,
        ),
        (
            "borrowed_captured_closure_escape",
            "fn typed<F:for<'r>Fn(&'r u32)->&'r u32>(f:F)->F{f}fn need<F:for<'r>Fn(&'r u32)->&'r u32>(_:F){}fn caller<'a>(p:&'a u32){let c=typed(|_|p);need(&c)}fn main(){}",
            false,
        ),
        (
            "nested_fixed_data_not_static",
            "fn get<T>(x:&T)->&T{x}fn need<'a,F:for<'r>Fn(&'r &'a u32)->&'static &'a u32>(_:&'a u32,_:F){}fn caller<'a>(data:&'a u32){need(data,get)}fn main(){}",
            false,
        ),
        (
            "nested_fixed_data_not_other_hole",
            "fn get<'r,T>(x:&'r T,_:&())->&'r T{x}fn need<'a,F:for<'r,'s>Fn(&'r &'a u32,&'s())->&'s &'a u32>(_:&'a u32,_:F){}fn caller<'a>(data:&'a u32){need(data,get)}fn main(){}",
            false,
        ),
        (
            "unused_closure_static_promise_reject",
            "fn f<'a>(s:&'a str){let _f=||->&'static str{s};}fn main(){}",
            false,
        ),
        (
            "ordinary_reference_argument_static_reject",
            "trait Read<A>{type Out;}struct S;impl<'a> Read<&'a u32> for S{type Out=&'a u32;}fn need<'a,T:Read<&'a u32,Out=&'static u32>>(_:T,_:&'a u32){}fn caller<'a>(x:&'a u32){need(S,x)}fn main(){}",
            false,
        ),
        (
            "ordinary_reference_argument_link",
            "trait Read<A>{type Out;}struct S;impl<'a> Read<&'a u32> for S{type Out=&'a u32;}fn need<'a,T:Read<&'a u32,Out=&'a u32>>(_:T,_:&'a u32){}fn caller<'a>(x:&'a u32){need(S,x)}fn main(){}",
            true,
        ),
        (
            "ordinary_reference_self_static_reject",
            "trait Read{type Out;}impl<'a> Read for &'a u32{type Out=&'a u32;}fn need<T:Read<Out=&'static u32>>(_:T){}fn caller<'a>(x:&'a u32){need(x)}fn main(){}",
            false,
        ),
        (
            "ordinary_reference_self_link",
            "trait Read{type Out;}impl<'a> Read for &'a u32{type Out=&'a u32;}fn need<'a,T:Read<Out=&'a u32>>(_:T){}fn caller<'a>(x:&'a u32){need:: <&'a u32>(x)}fn main(){}",
            true,
        ),
        (
            "ordinary_array_argument_static_reject",
            "trait Read<A>{type Out;}struct S;impl<'a> Read<[&'a u32;2]> for S{type Out=&'a u32;}fn need<'a,T:Read<[&'a u32;2],Out=&'static u32>>(_:T,_:&'a u32){}fn caller<'a>(x:&'a u32){need(S,x)}fn main(){}",
            false,
        ),
        (
            "ordinary_owner_lifetime_static_reject",
            "trait Read<'a>{type Out;}struct S;impl<'a> Read<'a> for S{type Out=&'a u32;}fn need<'a,T:Read<'a,Out=&'static u32>>(_:T,_:&'a u32){}fn caller<'a>(x:&'a u32){need(S,x)}fn main(){}",
            false,
        ),
        (
            "ordinary_owner_lifetime_link",
            "trait Read<'a>{type Out;}struct S;impl<'a> Read<'a> for S{type Out=&'a u32;}fn need<'a,T:Read<'a,Out=&'a u32>>(_:T,_:&'a u32){}fn caller<'a>(x:&'a u32){need(S,x)}fn main(){}",
            true,
        ),
        (
            "ordinary_super_argument_link",
            "trait Read<A>{type Out;}trait Super<A>:Read<A>{}struct S;impl<'a> Read<&'a u32> for S{type Out=&'a u32;}impl<'a> Super<&'a u32> for S{}fn need<'a,T:Super<&'a u32,Out=&'a u32>>(_:T,_:&'a u32){}fn caller<'a>(x:&'a u32){need(S,x)}fn main(){}",
            true,
        ),
        (
            "ordinary_super_argument_static_reject",
            "trait Read<A>{type Out;}trait Super<A>:Read<A>{}struct S;impl<'a> Read<&'a u32> for S{type Out=&'a u32;}impl<'a> Super<&'a u32> for S{}fn need<'a,T:Super<&'a u32,Out=&'static u32>>(_:T,_:&'a u32){}fn caller<'a>(x:&'a u32){need(S,x)}fn main(){}",
            false,
        ),
        (
            "ordinary_mutable_argument_link",
            "trait Read<A>{type Out;}struct S;impl<'a> Read<&'a mut u32> for S{type Out=&'a mut u32;}fn need<'a,T:Read<&'a mut u32,Out=&'a mut u32>>(_:T,_:&'a mut u32){}fn caller<'a>(x:&'a mut u32){need(S,x)}fn main(){}",
            true,
        ),
        (
            "ordinary_mutable_argument_static_reject",
            "trait Read<A>{type Out;}struct S;impl<'a> Read<&'a mut u32> for S{type Out=&'a mut u32;}fn need<'a,T:Read<&'a mut u32,Out=&'static mut u32>>(_:T,_:&'a mut u32){}fn caller<'a>(x:&'a mut u32){need(S,x)}fn main(){}",
            false,
        ),
        (
            "callable_and_ordinary_output_distinct",
            "trait Other<A>{type Output;}impl<A,F>Other<A> for F{type Output=();}fn need<'a,F:Fn(&'a u32)->&'a u32+Other<(&'a u32,),Output=()>>(_:F){}fn main(){need(|v|v);}",
            true,
        ),
        (
            "callable_and_ordinary_output_static_reject",
            "trait Other<A>{type Output;}impl<A,F>Other<A> for F{type Output=();}fn need<'a,F:Fn(&'a u32)->&'static u32+Other<(&'a u32,),Output=()>>(_:F,_:&'a u32){}fn caller<'a>(x:&'a u32){need(|v|v,x)}fn main(){}",
            false,
        ),
        (
            "scopeguard_adt_callback",
            "struct Guard<T,F:FnMut(&mut T)>{value:T,f:F}impl<T,F:FnMut(&mut T)>Guard<T,F>{fn new(value:T,f:F)->Self{Self{value,f}}}impl<T,F:FnMut(&mut T)>Drop for Guard<T,F>{fn drop(&mut self){(self.f)(&mut self.value)}}fn main(){let _=Guard::new(7,|x|*x+=1);}",
            true,
        ),
        (
            "opaque_chained_iterator_bound",
            "fn bounds<'a>(items:&'a[u32],more:impl Iterator<Item=&'a u32>){for value in items.iter().map(|v|v).chain(more){let _=value;}}fn main(){}",
            true,
        ),
        (
            "scopeguard_static_input_reject",
            "struct Guard<F:FnMut(&mut u32)>(F);impl<F:FnMut(&'static mut u32)>Guard<F>{fn f(&self){}}fn main(){}",
            false,
        ),
        (
            "scopeguard_borrowed_data",
            "struct Guard<T,F:FnMut(&mut T)>{value:T,f:F}impl<T,F:FnMut(&mut T)>Drop for Guard<T,F>{fn drop(&mut self){(self.f)(&mut self.value)}}fn make<'a>(value:&'a u32){let _g=Guard{value,f:|v:&mut &'a u32|{let _=**v;}};}fn main(){}",
            true,
        ),
        (
            "opaque_chained_iterator_escape",
            "fn bounds<'a>(items:&'a[u32],more:impl Iterator<Item=&'a u32>)->impl Iterator<Item=&'static u32>{items.iter().map(|v|v).chain(more)}fn main(){}",
            false,
        ),
        (
            "opaque_chained_mut_iterator",
            "fn bounds<'a>(items:&'a mut[u32],more:impl Iterator<Item=&'a mut u32>){for value in items.iter_mut().map(|v|v).chain(more){*value+=1;}}fn main(){}",
            true,
        ),
        (
            "conditional_projection_static_reject",
            "trait Source{type Item;}trait Convert{type Item;}impl<T:Source+'static>Convert for T{type Item=T::Item;}fn need<'a,X:Convert<Item=&'a u32>>(_:X){}fn caller<'a,T:Source<Item=&'a u32>>(x:T){need(x)}fn main(){}",
            false,
        ),
        (
            "conditional_projection_static",
            "trait Source{type Item;}trait Convert{type Item;}impl<T:Source+'static>Convert for T{type Item=T::Item;}fn need<'a,X:Convert<Item=&'a u32>>(_:X){}fn caller<'a,T:Source<Item=&'a u32>+'static>(x:T){need(x)}fn main(){}",
            true,
        ),
        (
            "conditional_projection_marker_reject",
            "trait Source{type Item;}trait Marker{}trait Convert{type Item;}impl<T:Source+Marker>Convert for T{type Item=T::Item;}fn need<'a,X:Convert<Item=&'a u32>>(_:X){}fn caller<'a,T:Source<Item=&'a u32>>(x:T){need(x)}fn main(){}",
            false,
        ),
        (
            "parser_nested_input_function",
            "trait Parser<I,O>{fn next(&mut self,i:&mut I)->O;}impl<I,O,F:FnMut(&mut I)->O>Parser<I,O>for F{fn next(&mut self,i:&mut I)->O{self(i)}}fn cut<I,O,P:Parser<I,O>>(p:P)->impl Parser<I,O>{p}struct Input<'a>(&'a str);fn parse(input:&mut Input<'_>)->u32{input.0.len()as u32}fn f<'a>(input:&mut Input<'a>)->u32{cut(parse).next(input)}fn main(){}",
            true,
        ),
        (
            "parser_nested_input_closure",
            "trait Parser<I,O>{fn next(&mut self,i:&mut I)->O;}impl<I,O,F:FnMut(&mut I)->O>Parser<I,O>for F{fn next(&mut self,i:&mut I)->O{self(i)}}fn cut<I,O,P:Parser<I,O>>(p:P)->impl Parser<I,O>{p}struct Input<'a>(&'a str);fn f<'a>(input:&mut Input<'a>)->u32{cut(|input:&mut Input<'a>|input.0.len()as u32).next(input)}fn main(){}",
            true,
        ),
        (
            "parser_nested_input_function_late",
            "trait Parser<I,O>{fn next(&mut self,i:&mut I)->O;}impl<I,O,F:FnMut(&mut I)->O>Parser<I,O>for F{fn next(&mut self,i:&mut I)->O{self(i)}}fn cut<I,O,P:Parser<I,O>>(p:P)->impl Parser<I,O>{p}struct Input<'a>(&'a str);fn parse<'a>(input:&mut Input<'a>)->u32{input.0.len()as u32}fn f<'a>(input:&mut Input<'a>)->u32{cut(parse).next(input)}fn main(){}",
            true,
        ),
        (
            "parser_nested_borrowed_output",
            "trait Parser<I,O>{fn next(&mut self,i:&mut I)->O;}impl<I,O,F:FnMut(&mut I)->O>Parser<I,O>for F{fn next(&mut self,i:&mut I)->O{self(i)}}fn cut<I,O,P:Parser<I,O>>(p:P)->impl Parser<I,O>{p}struct Input<'a>(&'a str);fn parse<'a>(input:&mut Input<'a>)->&'a str{input.0}fn f<'a>(input:&mut Input<'a>)->&'a str{cut(parse).next(input)}fn main(){}",
            true,
        ),
        (
            "parser_nested_borrowed_static_reject",
            "trait Parser<I,O>{fn next(&mut self,i:&mut I)->O;}impl<I,O,F:FnMut(&mut I)->O>Parser<I,O>for F{fn next(&mut self,i:&mut I)->O{self(i)}}fn cut<I,O,P:Parser<I,O>>(p:P)->impl Parser<I,O>{p}struct Input<'a>(&'a str);fn parse<'a>(input:&mut Input<'a>)->&'a str{input.0}fn f<'a>(input:&mut Input<'a>)->&'static str{cut(parse).next(input)}fn main(){}",
            false,
        ),
        (
            "parser_nested_two_calls",
            "trait Parser<I,O>{fn next(&mut self,i:&mut I)->O;}impl<I,O,F:FnMut(&mut I)->O>Parser<I,O>for F{fn next(&mut self,i:&mut I)->O{self(i)}}fn cut<I,O,P:Parser<I,O>>(p:P)->impl Parser<I,O>{p}struct Input<'a>(&'a str);fn parse(input:&mut Input<'_>)->u32{input.0.len()as u32}fn f<'a,'b>(first:&mut Input<'a>,second:&mut Input<'b>)->(u32,u32){(cut(parse).next(first),cut(parse).next(second))}fn main(){}",
            true,
        ),
        (
            "parser_nested_two_output_links",
            "trait Parser<I,O>{fn next(&mut self,i:&mut I)->O;}impl<I,O,F:FnMut(&mut I)->O>Parser<I,O>for F{fn next(&mut self,i:&mut I)->O{self(i)}}fn cut<I,O,P:Parser<I,O>>(p:P)->impl Parser<I,O>{p}struct Input<'a>(&'a str);fn parse<'a>(input:&mut Input<'a>)->&'a str{input.0}fn f<'a,'b>(first:&mut Input<'a>,second:&mut Input<'b>)->(&'a str,&'b str){(cut(parse).next(first),cut(parse).next(second))}fn main(){}",
            true,
        ),
        (
            "parser_nested_two_wrong_links",
            "trait Parser<I,O>{fn next(&mut self,i:&mut I)->O;}impl<I,O,F:FnMut(&mut I)->O>Parser<I,O>for F{fn next(&mut self,i:&mut I)->O{self(i)}}fn cut<I,O,P:Parser<I,O>>(p:P)->impl Parser<I,O>{p}struct Input<'a>(&'a str);fn parse<'a>(input:&mut Input<'a>)->&'a str{input.0}fn f<'a,'b>(first:&mut Input<'a>,second:&mut Input<'b>)->(&'b str,&'a str){(cut(parse).next(first),cut(parse).next(second))}fn main(){}",
            false,
        ),
        (
            "parser_nested_static_input_reject",
            "trait Parser<I,O>{fn next(&mut self,i:&mut I)->O;}impl<I,O,F:FnMut(&mut I)->O>Parser<I,O>for F{fn next(&mut self,i:&mut I)->O{self(i)}}fn cut<I,O,P:Parser<I,O>>(p:P)->impl Parser<I,O>{p}struct Input<'a>(&'a str);fn parse(input:&mut Input<'static>)->u32{input.0.len()as u32}fn f<'a>(input:&mut Input<'a>)->u32{cut(parse).next(input)}fn main(){}",
            false,
        ),
        (
            "parser_nested_captured_output_reject",
            "trait Parser<I,O>{fn next(&mut self,i:&mut I)->O;}impl<I,O,F:FnMut(&mut I)->O>Parser<I,O>for F{fn next(&mut self,i:&mut I)->O{self(i)}}fn cut<I,O,P:Parser<I,O>>(p:P)->impl Parser<I,O>{p}struct Input<'a>(&'a str);fn f<'a>(input:&mut Input<'a>)->&'static str{cut(|input:&mut Input<'a>|input.0).next(input)}fn main(){}",
            false,
        ),
        (
            "parser_nested_mutable_invariance_reject",
            "trait Parser<I,O>{fn next(&mut self,i:&mut I)->O;}impl<I,O,F:FnMut(&mut I)->O>Parser<I,O>for F{fn next(&mut self,i:&mut I)->O{self(i)}}fn cut<I,O,P:Parser<I,O>>(p:P)->impl Parser<I,O>{p}struct Input<'a>(&'a str);fn parse<'a>(input:&'a mut Input<'a>)->&'a str{input.0}fn f<'a>(input:&mut Input<'a>)->&'a str{cut(parse).next(input)}fn main(){}",
            false,
        ),
        (
            "parser_shared_data_two_callbacks",
            "trait Parser<I,O>{fn next(&mut self,i:&mut I)->O;}impl<I,O,F:FnMut(&mut I)->O>Parser<I,O>for F{fn next(&mut self,i:&mut I)->O{self(i)}}fn both<I,O,P:Parser<I,O>,Q:Parser<I,O>>(p:P,q:Q)->(P,Q){(p,q)}struct Input<'a>(&'a str);fn parse(i:&mut Input<'_>)->u32{i.0.len()as u32}fn f<'a>(i:&mut Input<'a>)->(u32,u32){let(mut p,mut q)=both(parse,parse);(p.next(i),q.next(i))}fn main(){}",
            true,
        ),
        (
            "parser_shared_data_output_links",
            "trait Parser<I,O>{fn next(&mut self,i:&mut I)->O;}impl<I,O,F:FnMut(&mut I)->O>Parser<I,O>for F{fn next(&mut self,i:&mut I)->O{self(i)}}fn both<I,O,P:Parser<I,O>,Q:Parser<I,O>>(p:P,q:Q)->(P,Q){(p,q)}struct Input<'a>(&'a str);fn parse<'a>(i:&mut Input<'a>)->&'a str{i.0}fn f<'a>(i:&mut Input<'a>)->(&'a str,&'a str){let(mut p,mut q)=both(parse,parse);(p.next(i),q.next(i))}fn main(){}",
            true,
        ),
        (
            "parser_shared_data_output_wf_static_reject",
            "trait Parser<I,O>{fn next(&mut self,i:&mut I)->O;}impl<I,O,F:FnMut(&mut I)->O>Parser<I,O>for F{fn next(&mut self,i:&mut I)->O{self(i)}}fn both<I,O,P:Parser<I,O>,Q:Parser<I,O>>(p:P,q:Q)->(P,Q){(p,q)}struct Input<'a>(&'a str);fn parse<'a>(i:&mut Input<'a>)->&'a str{i.0}fn fixed(_: &mut Input<'_>)->&'static str{\"fixed\"}fn f<'a>(i:&mut Input<'a>)->&'static str{let(mut p,mut q)=both(parse,fixed);let _=q.next(i);p.next(i)}fn main(){}",
            false,
        ),
        (
            "parser_shared_data_static_input_reject",
            "trait Parser<I,O>{fn next(&mut self,i:&mut I)->O;}impl<I,O,F:FnMut(&mut I)->O>Parser<I,O>for F{fn next(&mut self,i:&mut I)->O{self(i)}}fn both<I,O,P:Parser<I,O>,Q:Parser<I,O>>(p:P,q:Q)->(P,Q){(p,q)}struct Input<'a>(&'a str);fn parse(i:&mut Input<'_>)->u32{i.0.len()as u32}fn fixed(i:&mut Input<'static>)->u32{i.0.len()as u32}fn f<'a>(i:&mut Input<'a>)->u32{let(mut p,mut q)=both(parse,fixed);let _=q.next(i);p.next(i)}fn main(){}",
            false,
        ),
        (
            "parser_nested_generic_alias",
            "trait Parser<I,O>{fn next(&mut self,i:&mut I)->O;}impl<I,O,F:FnMut(&mut I)->O>Parser<I,O>for F{fn next(&mut self,i:&mut I)->O{self(i)}}fn cut<I,O,P:Parser<I,O>>(p:P)->impl Parser<I,O>{p}struct Locating<I>(I,I);struct Stateful<I,S>(I,S);type Input<'a>=Stateful<Locating<&'a str>,()>;fn parse(_: &mut Input<'_>)->u32{7}fn f<'a>(i:&mut Input<'a>)->u32{cut(parse).next(i)}fn main(){}",
            true,
        ),
        (
            "parser_nested_generic_direct",
            "trait Parser<I,O>{fn next(&mut self,i:&mut I)->O;}impl<I,O,F:FnMut(&mut I)->O>Parser<I,O>for F{fn next(&mut self,i:&mut I)->O{self(i)}}fn cut<I,O,P:Parser<I,O>>(p:P)->impl Parser<I,O>{p}struct Locating<I>(I,I);struct Stateful<I,S>(I,S);fn parse(_: &mut Stateful<Locating<&str>,()>)->u32{7}fn f<'a>(i:&mut Stateful<Locating<&'a str>,()>)->u32{cut(parse).next(i)}fn main(){}",
            true,
        ),
        (
            "parser_nested_result_alias",
            "trait Parser<I,O,E>{fn next(&mut self,i:&mut I)->Result<O,E>;}impl<I,O,E,F:FnMut(&mut I)->Result<O,E>>Parser<I,O,E>for F{fn next(&mut self,i:&mut I)->Result<O,E>{self(i)}}fn cut<I,O,E,P:Parser<I,O,E>>(p:P)->impl Parser<I,O,E>{p}struct Locating<I>(I,I);struct Stateful<I,S>(I,S);type Input<'a>=Stateful<Locating<&'a str>,()>;fn parse(_: &mut Input<'_>)->Result<u32,()>{Ok(7)}fn f<'a>(i:&mut Input<'a>)->Result<u32,()>{cut(parse).next(i)}fn main(){}",
            true,
        ),
        (
            "parser_inside_closure_alias",
            "trait Parser<I,O,E>{fn next(&mut self,i:&mut I)->Result<O,E>;}impl<I,O,E,F:FnMut(&mut I)->Result<O,E>>Parser<I,O,E>for F{fn next(&mut self,i:&mut I)->Result<O,E>{self(i)}}fn cut<I,O,E,P:Parser<I,O,E>>(p:P)->impl Parser<I,O,E>{p}fn trace<I,O,E,P:Parser<I,O,E>>(p:P)->impl Parser<I,O,E>{p}struct Locating<I>(I,I);struct Stateful<I,S>(I,S);type Input<'a>=Stateful<Locating<&'a str>,()>;fn parse(_: &mut Input<'_>)->Result<u32,()>{Ok(7)}fn f<'a>(i:&mut Input<'a>)->Result<u32,()>{trace(move|i:&mut Input<'a>|cut(parse).next(i)).next(i)}fn main(){}",
            true,
        ),
        (
            "parser_returned_closure_alias",
            "trait Parser<I,O,E>{fn next(&mut self,i:&mut I)->Result<O,E>;}impl<I,O,E,F:FnMut(&mut I)->Result<O,E>>Parser<I,O,E>for F{fn next(&mut self,i:&mut I)->Result<O,E>{self(i)}}fn cut<I,O,E,P:Parser<I,O,E>>(p:P)->impl Parser<I,O,E>{p}fn trace<I,O,E,P:Parser<I,O,E>>(p:P)->impl Parser<I,O,E>{p}struct Locating<I>(I,I);struct Stateful<I,S>(I,S);type Input<'a>=Stateful<Locating<&'a str>,()>;fn parse(_: &mut Input<'_>)->Result<u32,()>{Ok(7)}fn f<'s,'a>(state:&'s u32)->impl Parser<Input<'a>,u32,()>+'s{move|i:&mut Input<'a>|{let _=state;cut(parse).next(i)}}fn main(){}",
            true,
        ),
        (
            "parser_signature_only_closure",
            "trait Parser<I,O,E>{fn next(&mut self,i:&mut I)->Result<O,E>;}impl<I,O,E,F:FnMut(&mut I)->Result<O,E>>Parser<I,O,E>for F{fn next(&mut self,i:&mut I)->Result<O,E>{self(i)}}fn cut<I,O,E,P:Parser<I,O,E>>(p:P)->impl Parser<I,O,E>{p}fn trace<I,O,E,P:Parser<I,O,E>>(p:P)->impl Parser<I,O,E>{p}struct Locating<I>(I,I);struct Stateful<I,S>(I,S);type Input<'a>=Stateful<Locating<&'a str>,()>;fn parse(_: &mut Input<'_>)->Result<u32,()>{Ok(7)}fn document<'a>()->impl Parser<Input<'a>,u32,()>{move|i:&mut Input<'a>|cut(parse).next(i)}fn main(){}",
            true,
        ),
        (
            "parser_independent_state_closure",
            "trait Parser<I,O,E>{fn next(&mut self,i:&mut I)->Result<O,E>;}impl<I,O,E,F:FnMut(&mut I)->Result<O,E>>Parser<I,O,E>for F{fn next(&mut self,i:&mut I)->Result<O,E>{self(i)}}fn cut<I,O,E,P:Parser<I,O,E>>(p:P)->impl Parser<I,O,E>{p}fn trace<I,O,E,P:Parser<I,O,E>>(p:P)->impl Parser<I,O,E>{p}struct Locating<I>(I,I);struct Stateful<I,S>(I,S);type Input<'a>=Stateful<Locating<&'a str>,()>;fn parse(_: &mut Input<'_>)->Result<u32,()>{Ok(7)}fn document<'s,'a>(state:&'s std::cell::RefCell<u32>)->impl Parser<Input<'a>,u32,()>+'s{move|i:&mut Input<'a>|{*state.borrow_mut()+=1;cut(parse).next(i)}}fn main(){}",
            true,
        ),
        (
            "parser_independent_state_escape",
            "trait Parser<I,O,E>{fn next(&mut self,i:&mut I)->Result<O,E>;}impl<I,O,E,F:FnMut(&mut I)->Result<O,E>>Parser<I,O,E>for F{fn next(&mut self,i:&mut I)->Result<O,E>{self(i)}}fn cut<I,O,E,P:Parser<I,O,E>>(p:P)->impl Parser<I,O,E>{p}fn trace<I,O,E,P:Parser<I,O,E>>(p:P)->impl Parser<I,O,E>{p}struct Locating<I>(I,I);struct Stateful<I,S>(I,S);type Input<'a>=Stateful<Locating<&'a str>,()>;fn parse(_: &mut Input<'_>)->Result<u32,()>{Ok(7)}fn document<'s,'a>(state:&std::cell::RefCell<u32>)->impl Parser<Input<'a>,u32,()>+'s{move|i:&mut Input<'a>|{*state.borrow_mut()+=1;cut(parse).next(i)}}fn main(){}",
            false,
        ),
        (
            "stored_signature_static_promise_reject",
            "fn demand(value:&'static u32)->&'static u32{value}fn f<'a>(_:&'a()){let callback=|value:&'a u32|demand(value);let _=callback;}fn main(){}",
            false,
        ),
        (
            "discarded_signature_static_promise_reject",
            "fn demand(value:&'static u32)->&'static u32{value}fn f<'a>(_:&'a()){let _=|value:&'a u32|demand(value);}fn main(){}",
            false,
        ),
        (
            "returned_signature_static_promise_reject",
            "fn demand(value:&'static u32)->&'static u32{value}fn f<'a>()->impl Fn(&'a u32)->&'static u32{|value:&'a u32|demand(value)}fn main(){}",
            false,
        ),
        (
            "stored_signature_static_literal",
            "fn demand(value:&'static u32)->&'static u32{value}fn f<'a>(_:&'a()){let callback=|value:&'a u32|{let _=value;demand(&17)};let _=callback;}fn main(){}",
            true,
        ),
        (
            "discarded_signature_static_literal",
            "fn demand(value:&'static u32)->&'static u32{value}fn f<'a>(_:&'a()){let _=|value:&'a u32|{let _=value;demand(&17)};}fn main(){}",
            true,
        ),
        (
            "discarded_independent_input_static_reject",
            "fn demand(value:&'static u32)->&'static u32{value}fn f(){let _=|value:&u32|demand(value);}fn main(){}",
            false,
        ),
        (
            "parser_inside_closure_borrowed_result",
            "trait Parser<I,O,E>{fn next(&mut self,i:&mut I)->Result<O,E>;}impl<I,O,E,F:FnMut(&mut I)->Result<O,E>>Parser<I,O,E>for F{fn next(&mut self,i:&mut I)->Result<O,E>{self(i)}}fn cut<I,O,E,P:Parser<I,O,E>>(p:P)->impl Parser<I,O,E>{p}struct Locating<I>(I,I);struct Stateful<I,S>(I,S);type Input<'a>=Stateful<Locating<&'a str>,()>;fn parse<'a>(input:&mut Input<'a>)->Result<&'a str,()>{Ok(input.0.1)}fn f<'a>(i:&mut Input<'a>)->Result<&'a str,()>{let mut p=move|i:&mut Input<'a>|cut(parse).next(i);p.next(i)}fn main(){}",
            true,
        ),
        (
            "parser_inside_closure_borrowed_result_escape",
            "trait Parser<I,O,E>{fn next(&mut self,i:&mut I)->Result<O,E>;}impl<I,O,E,F:FnMut(&mut I)->Result<O,E>>Parser<I,O,E>for F{fn next(&mut self,i:&mut I)->Result<O,E>{self(i)}}fn cut<I,O,E,P:Parser<I,O,E>>(p:P)->impl Parser<I,O,E>{p}struct Locating<I>(I,I);struct Stateful<I,S>(I,S);type Input<'a>=Stateful<Locating<&'a str>,()>;fn parse<'a>(input:&mut Input<'a>)->Result<&'a str,()>{Ok(input.0.1)}fn f<'a>(i:&mut Input<'a>)->Result<&'static str,()>{let mut p=move|i:&mut Input<'a>|cut(parse).next(i);p.next(i)}fn main(){}",
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
        if name.contains("signature_static") || name == "discarded_independent_input_static_reject"
        {
            assert!(
                typed.diagnostics.is_empty(),
                "{name}: static demand must reach MIR: {:?}",
                typed.diagnostics
            );
        }
        let mut errors = typed.diagnostics.clone();
        if errors.is_empty() {
            errors.extend(flux_mir::build_file(&hir, &typed).diagnostics);
        }
        assert_eq!(errors.is_empty(), accepted, "{name}: {errors:?}");
    }
}

#[test]
fn closure_output_metadata_fails_closed() {
    use flux_ty::regions::Region;
    let source = "fn require_closure_output<'a,F:Fn(&'a str)->&'static str>(p:&'a str,f:F)->&'static str{f(p)}fn good<'a>(s:&'a str)->&'static str{require_closure_output(s,|_|\"fixed\")}fn main(){}";
    for mutation in 0..5 {
        let (_, _, hir) = flux_hir::lower_text("main.rs", source);
        assert!(hir.diagnostics.is_empty());
        let mut typed = flux_mir::check_file(&hir);
        assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
        let function = hir
            .items
            .functions
            .iter()
            .find(|(_, function)| function.name.as_str() == "require_closure_output")
            .unwrap()
            .0;
        let mut regions = typed.sigs.regions.fns[function].clone();
        let mut projections = typed.sigs.fn_projections[function].to_vec();
        assert_eq!(projections.len(), 1);
        match mutation {
            0 => regions.projections = Box::default(),
            1 => {
                regions.projections = Box::default();
                projections.clear();
            }
            2 => regions.projections[0] = Box::default(),
            3 | 4 => {
                projections.push(projections[0]);
                let mut rows = regions.projections.to_vec();
                rows.push(if mutation == 3 {
                    vec![Region::Param(0)].into()
                } else {
                    rows[0].clone()
                });
                regions.projections = rows.into();
            }
            _ => unreachable!(),
        }
        typed.sigs.regions.fns.insert(function, regions);
        typed
            .sigs
            .fn_projections
            .insert(function, projections.into());
        let mir = flux_mir::build_file(&hir, &typed);
        assert_eq!(
            mir.diagnostics.is_empty(),
            mutation == 4,
            "mutation {mutation}: {:?}",
            mir.diagnostics
        );
    }
}

#[test]
fn ordinary_output_metadata_fails_closed() {
    let source = "trait Read<A>{type Out;}struct S;impl<'a>Read<&'a u32> for S{type Out=&'a u32;}fn require_read<'a,T:Read<&'a u32,Out=&'a u32>>(_:T,_:&'a u32){}fn caller<'a>(x:&'a u32){require_read(S,x)}fn main(){}";
    for mutation in 0..4 {
        let (_, _, hir) = flux_hir::lower_text("main.rs", source);
        assert!(hir.diagnostics.is_empty());
        let mut typed = flux_mir::check_file(&hir);
        assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
        let function = hir
            .items
            .functions
            .iter()
            .find(|(_, f)| f.name.as_str() == "require_read")
            .unwrap()
            .0;
        let mut metadata = typed.sigs.regions.fns[function].clone();
        match mutation {
            0 => {
                typed.sigs.fn_projections.insert(function, Box::default());
                metadata.projections = Box::default();
            }
            1 => metadata.bound_traits[0].args = Box::default(),
            2 => metadata.projections[0] = Box::default(),
            3 => metadata.bound_traits = Box::default(),
            _ => unreachable!(),
        }
        typed.sigs.regions.fns.insert(function, metadata);
        let mir = flux_mir::build_file(&hir, &typed);
        assert!(
            !mir.diagnostics.is_empty(),
            "mutation {mutation} was accepted"
        );
    }
}

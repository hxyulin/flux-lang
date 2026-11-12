//! Rustc controls for the selected registry crate compatibility gaps.
fn compare(name: &str, source: &str, accepted: bool) {
    let root = std::env::temp_dir().join(format!("flux-itertools-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("case.rs");
    let source = format!("{source}\nfn main() {{}}\n");
    std::fs::write(&path, &source).unwrap();
    let rustc = std::process::Command::new("rustc")
        .args(["--edition=2024", "--emit=metadata", "--out-dir"])
        .arg(&root)
        .arg(&path)
        .output()
        .unwrap();
    assert_eq!(
        rustc.status.success(),
        accepted,
        "rustc {name}: {}",
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
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(errors.is_empty(), accepted, "flux {name}: {errors:?}");
}
#[test]
fn unsized_callback_place() {
    compare(
        "unsized_callback_place",
        "fn call<A,K,F:FnMut(A)->K+?Sized>(f:&mut F,arg:A)->K{(*f)(arg)}",
        true,
    );
    compare(
        "unsized_callback_move",
        "fn call<F:FnOnce()+?Sized>(f:&mut F){(*f)()}",
        false,
    );
}

#[test]
fn ordinary_const_arguments_during_legacy_collection() {
    compare(
        "ordinary_const_arguments_during_legacy_collection",
        "fn size<const N:usize>()->usize{N} fn call()->usize{size::<{1+2}>()} macro_rules! call{()=>{size::<{2+3}>()}} fn expanded()->usize{call!()}",
        true,
    );
}
#[test]
fn comparator_key_forwarding() {
    compare(
        "comparator_key_forwarding",
        r#"
use std::cmp::Ordering;
fn helper<I,K,F,C>(_:I,_:F,_:C)where I:Iterator,F:FnMut(&I::Item)->K,C:FnMut(&I::Item,&I::Item,&K,&K)->Ordering{}
fn outer<I:Iterator,K:Ord,F:FnMut(&I::Item)->K>(it:I,key:F){helper(it,key,|_,_,a,b|a.cmp(b));}
"#,
        true,
    );
}
#[test]
fn iterator_output_only_parameter() {
    compare(
        "iterator_output_only_parameter",
        r#"
use std::iter::Fuse;
trait Has<I:Iterator>{type Combination:Iterator<Item=Self>;}
struct Pair<I:Iterator>(I);
impl<I,A>Iterator for Pair<I>where I:Iterator<Item=A>+Clone,A:Clone{type Item=(A,A);fn next(&mut self)->Option<Self::Item>{None}}
impl<I,A>Has<I>for(A,A)where I:Iterator<Item=A>+Clone,I::Item:Clone{type Combination=Pair<Fuse<I>>;}
"#,
        true,
    );
}

#[test]
fn associated_output_reference_contract() {
    compare(
        "associated_output_reference_contract",
        r#"
trait Family<'a>: 'static { type Output: 'a; }
struct Wrap<Y>(Y);
impl<Y:for<'a>Family<'a>>Wrap<Y>{fn get<'a>(&'a self)->&'a <Y as Family<'a>>::Output{loop{}}}
impl<T:?Sized,Y>AsRef<T>for Wrap<Y>where Y:for<'a>Family<'a>,for<'a><Y as Family<'a>>::Output:AsRef<T>{fn as_ref(&self)->&T{self.get().as_ref()}}
"#,
        true,
    );
}

#[test]
fn bound_dependent_pointer_output() {
    compare(
        "bound_dependent_pointer_output",
        r#"
trait Family<'a>: 'static { type Output: 'a; }
fn forward<'a,Y:for<'s>Family<'s>>(f:for<'s>fn(&'s u32)-><Y as Family<'s>>::Output, value:&'a u32)-><Y as Family<'a>>::Output{f(value)}
"#,
        true,
    );
    compare(
        "unconstrained_pointer_output",
        r#"
trait Family<'a> { type Output; }
type Bad<Y:for<'a>Family<'a>>=for<'a>fn()-><Y as Family<'a>>::Output;
"#,
        false,
    );
    compare(
        "noninjective_pointer_input",
        r#"
trait Family<'a> { type Output; }
type Bad<Y:for<'a>Family<'a>>=for<'a>fn(<Y as Family<'a>>::Output)->&'a ();
"#,
        false,
    );
}

#[test]
fn mutable_associated_reference_cast() {
    compare(
        "mutable_reference_cast",
        r#"
fn change<'a,T:'static,F:for<'b>FnOnce(&'b mut &'a T)>(value:&'a mut &'static T,f:F){let out=unsafe{&mut *(value as *mut &'static T as *mut &'a T)};f(out)}
"#,
        true,
    );
    compare(
        "mutable_associated_reference_cast",
        r#"
trait Family<'a>: 'static{type Output:'a;fn change<F:for<'b>FnOnce(&'b mut Self::Output)>(&'a mut self,f:F);}
impl<'a,T:'static>Family<'a>for &'static T{type Output=&'a T;fn change<F:for<'b>FnOnce(&'b mut Self::Output)>(&'a mut self,f:F){let out=unsafe{&mut *(self as *mut Self as *mut Self::Output)};f(out)}}
"#,
        true,
    );
}
#[test]
fn inferred_projection_owner_on_associated_call() {
    compare(
        "inferred_projection_owner_on_associated_call",
        r#"
trait Family<'a>: 'static {type Output:'a;}
trait FromBorrow<'a,T:?Sized>{fn borrow(value:&'a T)->Self;}
fn build<Y:for<'a>Family<'a>,T:?Sized,F>(_:&T,_:F)where F:for<'a>FnOnce(&'a T)-><Y as Family<'a>>::Output{}
fn outer<Y,T:?Sized>(value:&T)where Y:for<'a>Family<'a>,for<'a><Y as Family<'a>>::Output:FromBorrow<'a,T>{build::<Y,T,_>(value,|x|<Y as Family>::Output::borrow(x))}
"#,
        true,
    );
}

#[test]
fn aggregate_family_output_bounds() {
    compare(
        "aggregate_family_output_bounds",
        r#"
trait Family<'a>:'static{type Output:'a;}
impl<'a,T:for<'b>Family<'b>>Family<'a>for Option<T>{type Output=Option<<T as Family<'a>>::Output>;}
impl<'a,T:for<'b>Family<'b>,U:for<'b>Family<'b>>Family<'a>for(T,U){type Output=(<T as Family<'a>>::Output,<U as Family<'a>>::Output);}
"#,
        true,
    );
    compare(
        "aggregate_static_output",
        r#"
trait Family<'a>{type Output:'static;}
impl<'a,T:for<'b>Family<'b>>Family<'a>for Option<T>{type Output=Option<<T as Family<'a>>::Output>;}
"#,
        true,
    );
}
#[test]
fn ranked_family_impl_headers() {
    compare(
        "ranked_family_impl_headers",
        r#"
trait Family<'a>:'static{type Output:'a;}
struct Wrap<Y:for<'a>Family<'a>>(Y);
impl<Y:for<'a>Family<'a>>PartialEq for Wrap<Y>where for<'a><Y as Family<'a>>::Output:PartialEq{fn eq(&self,_:&Self)->bool{true}}
impl<Y:for<'a>Family<'a>>Eq for Wrap<Y>where for<'a><Y as Family<'a>>::Output:Eq{}
impl<Y:for<'a>Family<'a>>PartialOrd for Wrap<Y>where for<'a><Y as Family<'a>>::Output:PartialOrd{fn partial_cmp(&self,_:&Self)->Option<std::cmp::Ordering>{None}}
impl<Y:for<'a>Family<'a>>Ord for Wrap<Y>where for<'a><Y as Family<'a>>::Output:Ord{fn cmp(&self,_:&Self)->std::cmp::Ordering{std::cmp::Ordering::Equal}}
trait Borrow<'a>:'a{}
impl<Y>Wrap<Y>where Y:for<'a>Family<'a>,for<'a><Y as Family<'a>>::Output:Borrow<'a>{}
"#,
        true,
    );
}

#[test]
fn const_arguments_do_not_store_borrows() {
    compare(
        "const_arguments_do_not_store_borrows",
        r#"
trait Static: 'static {}
struct Bytes<const N:usize>([u8;N]);
impl<const N:usize>Static for Bytes<N>{}
fn keep<const N:usize>(b:Bytes<N>)->Box<dyn std::any::Any>{Box::new(b)}
"#,
        true,
    );
    compare(
        "type_arguments_still_store_borrows",
        r#"
trait Static: 'static {}
struct Bytes<T,const N:usize>([T;N]);
impl<T,const N:usize>Static for Bytes<T,N>{}
"#,
        false,
    );
}
#[test]
fn union_field_implied_copy() {
    compare(
        "union_field_implied_copy",
        "trait Bits where Self:Copy{} union Storage<T:Bits>{value:T,byte:u8}",
        true,
    );
    compare(
        "union_field_missing_copy",
        "trait Bits{} union Storage<T:Bits>{value:T,byte:u8}",
        false,
    );
}

#[test]
fn associated_storage_static_bound() {
    compare(
        "associated_storage_static_bound",
        r#"
trait Bytes:Copy+'static{}
trait Convert:Copy{type Stored:Bytes;}
struct Slice<T:Convert>([T::Stored]);
trait Static:'static{}
impl<T:Convert+'static>Static for Slice<T>{}
"#,
        true,
    );
    compare(
        "associated_storage_missing_static",
        r#"
trait Convert{type Stored;}
struct Slice<T:Convert>([T::Stored]);
trait Static:'static{}
impl<T:Convert>Static for Slice<T>{}
"#,
        false,
    );
}

#[test]
fn fieldless_enum_const_transmutes() {
    compare(
        "enum_const_transmute",
        r#"
#[repr(u8)]#[derive(Copy,Clone)]enum Byte{A=0xEF,B=0xBD}
const X:Byte=unsafe{std::mem::transmute(0xEFu8)};
const Y:u8=unsafe{std::mem::transmute(X)};
const Z:[Byte;2]=unsafe{std::mem::transmute([0xEFu8,0xBDu8])};
const _:()={assert!(Y==0xEF);assert!(Z[1]as u8==0xBD);};
#[repr(i16)]enum Signed{A=-7,B=513}
const S:Signed=unsafe{std::mem::transmute(-7i16)};
const _:()=assert!(S as i16 == -7);
"#,
        true,
    );
    compare(
        "invalid_enum_const_transmute",
        r#"
#[repr(u8)]enum Byte{A=1,B=3}const X:Byte=unsafe{std::mem::transmute(2u8)};
"#,
        false,
    );
}

#[test]
fn explicit_associated_body_type_arguments() {
    compare(
        "explicit_associated_body_type_arguments",
        r#"
trait Family<'a>:'static{type Output:'a;fn view(&'a self)->&'a Self::Output;}
struct View<'a>(&'a str);
impl<'a>Family<'a>for View<'static>{type Output=View<'a>;fn view(&'a self)->&'a Self::Output{unsafe{std::mem::transmute::<&'a Self,&'a Self::Output>(self)}}}
"#,
        true,
    );
}

#[test]
fn trait_parameter_static_contract_in_default_method() {
    compare(
        "trait_parameter_static_contract_in_default_method",
        r#"
trait Value:Copy+'static{}
struct View<'a,T:Value>(&'a [T]);
enum Kind{Fast,Small}
trait Seal{}
trait Access<'a,T:Value>:Seal{const KIND:Kind;fn view(&self)->&View<'a,T>;fn first(&self)->T{self.view().0[0]}}
"#,
        true,
    );
    compare(
        "missing_trait_parameter_static_contract",
        r#"
trait Value:Copy{}
struct View<'a,T:Value>(&'a [T]);
trait Access<'a,T:Value>{fn view(&self)->&View<'a,T>;fn boxed(&self)->Box<dyn std::any::Any>{Box::new(self.view().0[0])}}
"#,
        false,
    );
}

#[test]
fn cast_lifetime_annotations() {
    compare(
        "aggregate_cast_lifetimes",
        "struct View<'a>(&'a u8);fn cast<'a>(v:View<'a>)->View<'a>{v as View<'a>}",
        true,
    );
    compare(
        "aggregate_cast_lifetime_escape",
        "struct View<'a>(&'a u8);fn cast<'a>(v:View<'a>)->View<'static>{v as View<'static>}",
        false,
    );
    compare(
        "reference_cast_lifetime_escape",
        "fn cast<'a>(v:&'a u8)->&'static u8{v as &'static u8}",
        false,
    );
    compare(
        "box_cannot_add_auto_traits",
        "fn cast(v:Box<dyn std::any::Any>)->Box<dyn std::any::Any+Send>{v as Box<dyn std::any::Any+Send>}",
        false,
    );
}

#[test]
fn explicit_trait_type_argument_regions() {
    compare(
        "explicit_trait_type_argument_regions",
        r#"
struct Wrap<'a>(&'a u8);
fn forward<'a>(v:Wrap<'a>)->Wrap<'a>{<Wrap<'a> as From<Wrap<'a>>>::from(v)}
"#,
        true,
    );
    compare(
        "explicit_trait_type_argument_escape",
        r#"
struct Wrap<'a>(&'a u8);
fn forward<'a>(v:Wrap<'a>)->Wrap<'static>{<Wrap<'static> as From<Wrap<'static>>>::from(v)}
"#,
        false,
    );
}

#[test]
fn borrowed_stored_family_output() {
    compare(
        "borrowed_stored_family_output",
        r#"
trait Data<'a>{fn text(&self)->&str;}
trait Lookup<'a>{type Data:Data<'a>;}
struct Span<'a,R:Lookup<'a>>{owner:&'a R,data:R::Data}
impl<'a,R:Lookup<'a>>Span<'a,R>{fn text(&self)->&str{self.data.text()}}
"#,
        true,
    );
    compare(
        "borrowed_stored_family_output_escape",
        r#"
trait Data<'a>{fn text(&self)->&str;}
trait Lookup<'a>{type Data:Data<'a>;}
struct Span<'a,R:Lookup<'a>>{owner:&'a R,data:R::Data}
impl<'a,R:Lookup<'a>>Span<'a,R>{fn text(&self)->&'static str{self.data.text()}}
"#,
        false,
    );
}

#[test]
fn family_reference_forwarding() {
    compare(
        "family_reference_forwarding",
        r#"
trait Family<'a>:'static{type Output:'a;fn transform(&'a self)->&'a Self::Output;}
struct Wrap<Y:for<'a>Family<'a>>(Y);
impl<Y:for<'a>Family<'a>>Wrap<Y>{fn get<'a>(&'a self)->&'a <Y as Family<'a>>::Output{self.0.transform()}}
"#,
        true,
    );
}

#[test]
fn stored_family_facts_keep_owner_lifetimes() {
    compare(
        "stored_family_facts_keep_owner_lifetimes",
        r#"
trait Lookup<'a>{type Data;}
trait Tag{} impl<T>Tag for T{}
fn bad<'a,'b,'s,R>(_:&'s <R as Lookup<'a>>::Data,other:<R as Lookup<'b>>::Data)->Box<dyn Tag+'s>
where R:Lookup<'a>+Lookup<'b>{Box::new(other)}
"#,
        false,
    );
}

#[test]
fn target_proc_macro_auto_traits_and_surface() {
    for name in [
        "TokenStream",
        "Span",
        "Group",
        "Ident",
        "Punct",
        "Literal",
        "LexError",
        "token_stream::IntoIter",
    ] {
        compare(
            &format!("proc_macro_send_{}", name.replace("::", "_")),
            &format!(
                "extern crate proc_macro;fn require<T:Send>(){{}}fn check(){{require::<proc_macro::{name}>();}}"
            ),
            false,
        );
        compare(
            &format!("proc_macro_sync_{}", name.replace("::", "_")),
            &format!(
                "extern crate proc_macro;fn require<T:Sync>(){{}}fn check(){{require::<proc_macro::{name}>();}}"
            ),
            false,
        );
    }
    compare(
        "target_proc_macro_no_host_bridge",
        "extern crate proc_macro;fn check(){proc_macro::__flux_enter_host();}",
        false,
    );
    compare(
        "target_proc_macro_no_default_span",
        "extern crate proc_macro;fn check(){let _:proc_macro::Span=Default::default();}",
        false,
    );
    compare(
        "target_proc_macro_span_queries",
        "extern crate proc_macro;fn check(s:proc_macro::Span){let _:usize=s.line();let _:usize=s.column();let _:String=s.file();let _:Option<std::path::PathBuf>=s.local_file();let _:proc_macro::Span=s.start();let _:proc_macro::Span=s.end();}",
        true,
    );
}

#[test]
fn closed_inline_constant_frames() {
    compare(
        "closed_inline_constant_frames",
        r#"
const fn double(x:u32)->u32{x*2}
fn values()->&'static[u32]{const{&[double(7),double(11)]}}
fn generic<T>()->&'static[u32]{const{&[double(13)]}}
fn nested()->u32{const{let mut values=[double(2),const{double(3)}];values[0]+=1;values[0]+values[1]}}
fn generic_value<const N:usize>()->usize{const{N+1}}
"#,
        true,
    );
    compare(
        "inline_constant_local_escape",
        "fn bad()->&'static u32{const{let x=7;&x}}",
        false,
    );
    compare(
        "inline_constant_overflow",
        "fn bad()->u8{const{let x=255u8;x+1}}",
        true,
    );
    compare(
        "inline_constant_panic",
        "fn bad()->u8{const{panic!(\"bad\")}}",
        true,
    );
    compare(
        "inline_constant_interior_borrow",
        "fn bad()->&'static std::cell::Cell<u8>{const{&std::cell::Cell::new(7)}}",
        false,
    );
}

#[test]
fn borrowed_iterator_family_contract() {
    compare(
        "borrowed_iterator_family_contract",
        r#"
trait Store<'a,K:'a,V:'a>{type Iter:Iterator<Item=(&'a K,&'a V)>+'a;}
struct Map<K,V,S>(K,V,S);
impl<'a,K,V,S>IntoIterator for &'a Map<K,V,S>where S:Store<'a,K,V>{type Item=(&'a K,&'a V);type IntoIter=S::Iter;fn into_iter(self)->Self::IntoIter{loop{}}}
"#,
        true,
    );
}

#[test]
fn borrowed_iterator_family_cannot_strengthen_output() {
    compare(
        "borrowed_iterator_family_static",
        r#"
trait Store<'a>{type Iter:Iterator<Item=&'a u8>+'a;}
struct Map<S>(S);
impl<'a,S>IntoIterator for &'a Map<S>where S:Store<'a>{type Item=&'static u8;type IntoIter=S::Iter;fn into_iter(self)->Self::IntoIter{loop{}}}
"#,
        false,
    );
    compare(
        "borrowed_iterator_family_other_owner",
        r#"
trait Store<'a>{type Iter:Iterator<Item=&'a u8>+'a;}
struct Map<'b,S>(&'b (),S);
impl<'a,'b,S>IntoIterator for &'a Map<'b,S>where S:Store<'b>{type Item=&'a u8;type IntoIter=S::Iter;fn into_iter(self)->Self::IntoIter{loop{}}}
"#,
        false,
    );
}

#[test]
fn associated_enum_unit_variant() {
    compare(
        "associated_enum_unit_variant",
        r#"
enum Error<T>{Invalid,Data(T)}trait Convert{type Error;fn convert()->Self::Error;}
struct Value;impl Convert for Value{type Error=Error<u32>;fn convert()->Self::Error{Self::Error::Invalid}}
"#,
        true,
    );
    compare(
        "associated_enum_missing_variant",
        r#"
enum Error{Invalid}trait Convert{type Error;fn convert()->Self::Error;}
struct Value;impl Convert for Value{type Error=Error;fn convert()->Self::Error{Self::Error::Missing}}
"#,
        false,
    );
}

#[test]
fn tuple_assignment_has_no_pattern_ergonomics() {
    compare(
        "tuple_assignment_borrowed",
        "fn bad(){let(mut a,mut b)=(&0u8,&1u8);(a,b)=&(2,3);}",
        false,
    );
    compare(
        "tuple_assignment_nested_borrowed",
        "fn bad(){let(mut a,mut b)=(0u8,&1u8);(a,(b,))=(2,&(3,));}",
        false,
    );
    compare(
        "tuple_assignment_immutable",
        "fn bad(){let(a,mut b)=(0,1);(a,b)=(2,3);}",
        false,
    );
    compare(
        "tuple_assignment_move",
        "fn bad(){let mut a=String::new();let x=String::from(\"x\");(a,_)=(x,0);let _=x.len();}",
        false,
    );
    compare(
        "tuple_assignment_swap",
        "fn swap(){let(mut a,mut b)=(String::from(\"a\"),String::from(\"b\"));(a,b)=(b,a);}",
        true,
    );
}

#[test]
fn higher_ranked_associated_output_bound() {
    compare(
        "higher_ranked_associated_output_bound",
        r#"
trait Family<'a>:'static{type Output:'a;fn view(&'a self)->&'a Self::Output;}
trait Marker:'static{type Data:for<'a>Family<'a>;}
struct Wrap<M:Marker>(M::Data);
impl<M:Marker>Wrap<M>{fn get<'a>(&'a self)->&'a <M::Data as Family<'a>>::Output{self.0.view()}}
"#,
        true,
    );
    compare(
        "higher_ranked_output_impl",
        r#"
trait Family<'a>{}trait Marker{type Data:for<'a>Family<'a>;}
struct All;impl<'a>Family<'a>for All{}impl Marker for(){type Data=All;}
"#,
        true,
    );
    compare(
        "higher_ranked_output_static_impl",
        r#"
trait Family<'a>{}trait Marker{type Data:for<'a>Family<'a>;}
struct OnlyStatic;impl Family<'static>for OnlyStatic{}impl Marker for(){type Data=OnlyStatic;}
"#,
        false,
    );
}

#[test]
fn higher_ranked_output_keeps_each_bound_occurrence() {
    compare(
        "higher_ranked_output_after_static",
        r#"
trait Family<'a>{}trait Marker{type Data:Family<'static>+for<'a>Family<'a>;}
struct OnlyStatic;impl Family<'static>for OnlyStatic{}impl Marker for(){type Data=OnlyStatic;}
"#,
        false,
    );
}

#[test]
fn nongeneric_constant_projection_type_is_normalized() {
    compare(
        "nongeneric_constant_projection_type",
        r#"
trait Marker{type Data;}struct Data{field:u32}struct M;impl Marker for M{type Data=Data;}
struct Baked;impl Baked{const DATA:&'static <M as Marker>::Data=&Data{field:7};}
const DATA:&'static <M as Marker>::Data=&Data{field:11};
fn fields()->u32{Baked::DATA.field+DATA.field}
"#,
        true,
    );
}

#[test]
fn concrete_associated_family_debug_bound() {
    compare(
        "concrete_associated_family_debug_bound",
        r#"
use std::marker::PhantomData;
trait Family<'a>:'static{type Output:'a;}
trait Marker:'static{type Data:for<'a>Family<'a>;}
#[derive(Debug)]struct Data<'a>(&'a str);
impl<'a>Family<'a> for Data<'static>{type Output=Data<'a>;}
struct M;impl Marker for M{type Data=Data<'static>;}
struct Wrap<M:Marker>(PhantomData<M>);
impl<M:Marker>std::fmt::Debug for Wrap<M>where for<'a>&'a <M::Data as Family<'a>>::Output:std::fmt::Debug{fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{f.write_str("wrap")}}
#[derive(Debug)]struct Holder{data:Wrap<M>}
"#,
        true,
    );
}

#[test]
fn higher_ranked_associated_output_equality() {
    compare(
        "higher_ranked_associated_output_equality",
        r#"
trait Family<'a>:'static{type Output:'a;fn view(&'a self)->&'a Self::Output;}
trait Marker:'static{type Data:for<'a>Family<'a,Output=Self::Borrowed<'a>>;type Borrowed<'a>:'a;}
struct Wrap<M:Marker>(M::Data);
impl<M:Marker>Wrap<M>{fn get<'a>(&'a self)->&'a M::Borrowed<'a>{self.0.view()}}
"#,
        true,
    );
    compare(
        "higher_ranked_associated_equality_static_escape",
        r#"
trait Family<'a>:'static{type Output:'a;fn view(&'a self)->&'a Self::Output;}
trait Marker:'static{type Data:for<'a>Family<'a,Output=Self::Borrowed<'a>>;type Borrowed<'a>:'a;}
struct Wrap<M:Marker>(M::Data);
impl<M:Marker>Wrap<M>{fn get<'a>(&'a self)->&'static M::Borrowed<'a>{self.0.view()}}
"#,
        false,
    );
}

#[test]
fn higher_ranked_associated_equality_impl_contract() {
    compare(
        "higher_ranked_associated_equality_impl",
        r#"
trait Family<'a>{type Output;}
trait Marker{type Data:for<'a>Family<'a,Output=Self::Borrowed<'a>>;type Borrowed<'a>;}
struct Data;impl<'a>Family<'a>for Data{type Output=&'a u32;}
impl Marker for(){type Data=Data;type Borrowed<'a>=&'a u32;}
"#,
        true,
    );
    compare(
        "higher_ranked_associated_equality_wrong_impl",
        r#"
trait Family<'a>{type Output;}
trait Marker{type Data:for<'a>Family<'a,Output=Self::Borrowed<'a>>;type Borrowed<'a>;}
struct Data;impl<'a>Family<'a>for Data{type Output=&'static u32;}
impl Marker for(){type Data=Data;type Borrowed<'a>=&'a u32;}
"#,
        false,
    );
}

#[test]
fn associated_equality_through_supertrait_output() {
    compare(
        "associated_equality_through_supertrait_output",
        r#"
trait Data{type Value;}trait Marker:Data{}
trait Property{type Marker:Marker<Value=View<'static>>;}
struct View<'a>(&'a str);
fn accept<M:Data<Value=View<'static>>>(){}
fn forward<P:Property>(){accept::<P::Marker>();}
"#,
        true,
    );
}
#[test]
fn static_associated_output_is_independent_of_owner() {
    compare(
        "static_associated_output_is_independent_of_owner",
        r#"
trait Value:'static{}impl<T:'static>Value for T{}
trait Family<'a>{type Output:Value;}
fn boxed<'a,T:Family<'a>>(x:<T as Family<'a>>::Output)->Box<dyn Value>{Box::new(x)}
"#,
        true,
    );
}

#[test]
fn gat_output_can_outlive_its_complete_inputs() {
    compare(
        "gat_output_can_outlive_inputs",
        r#"
trait Marker:'static{type Borrowed<'a>;fn get<'a>(&'a self)->&'a Self::Borrowed<'a>;}
fn forward<'a,T:Marker>(x:&'a T)->&'a T::Borrowed<'a>{x.get()}
"#,
        true,
    );
    compare(
        "gat_output_cannot_escape_inputs",
        r#"
trait Marker:'static{type Borrowed<'a>;fn get<'a>(&'a self)->&'a Self::Borrowed<'a>;}
fn forward<'a,T:Marker>(x:&'a T)->&'static T::Borrowed<'a>{x.get()}
"#,
        false,
    );
}

#[test]
fn static_gat_default_constructor() {
    let source = "trait Marker:'static{type Borrowed<'a>;}struct View<'a,M:Marker>{data:&'a M::Borrowed<'a>}impl<M:Marker>View<'static,M>{fn new()->Self{loop{}}}impl<M:Marker>Default for View<'static,M>{fn default()->Self{Self::new()}}";
    compare("static_gat_default_constructor", source, true);
}

#[test]
fn explicit_early_function_lifetimes() {
    compare(
        "explicit_early_function_lifetimes",
        "fn first<'a:'a>(x:&'a u8)->&'a u8{x}fn forward<'s>(x:&'s u8)->&'s u8{first::<'s>(x)}",
        true,
    );
    compare(
        "explicit_early_function_static_escape",
        "fn first<'a:'a>(x:&'a u8)->&'a u8{x}fn bad<'s>(x:&'s u8)->&'static u8{first::<'static>(x)}",
        false,
    );
    compare(
        "explicit_late_function_lifetime",
        "fn first<'a>(x:&'a u8)->&'a u8{x}fn bad<'s>(x:&'s u8)->&'s u8{first::<'s>(x)}",
        false,
    );
    compare(
        "explicit_early_lifetime_count",
        "fn first<'a:'a>(x:&'a u8)->&'a u8{x}fn bad<'s>(x:&'s u8)->&'s u8{first::<'s,'s>(x)}",
        false,
    );
}

#[test]
fn associated_constant_selected_storage() {
    compare(
        "associated_constant_selected_storage",
        r#"
trait Family<'a>:'static{type Output:'a;}
struct Data<'a>(&'a u8);
impl<'a>Family<'a>for Data<'static>{type Output=Data<'a>;}
trait Named{
 type Owned:'static+for<'a>Family<'a,Output=Self::Borrowed<'a>>;
 type Borrowed<'a>:'a;
 const SINGLETON:&'static Self::Borrowed<'static>;
 fn identity<'a>(value:&'a <Self::Owned as Family<'a>>::Output)->&'a Self::Borrowed<'a>;
}
struct Concrete;
impl Named for Concrete{
 type Owned=Data<'static>;
 type Borrowed<'a>=Data<'a>;
 const SINGLETON:&'static Self::Owned=&Data(&7);
 fn identity<'a>(value:&'a Data<'a>)->&'a Self::Borrowed<'a>{value}
}
"#,
        true,
    );
}

#[test]
fn static_associated_storage_keeps_sync_requirement() {
    compare(
        "static_associated_storage_sync",
        "trait M{type Data;}struct Marker;impl M for Marker{type Data=std::cell::Cell<u8>;}static S:<Marker as M>::Data=std::cell::Cell::new(7);",
        false,
    );
}

#[test]
fn quantified_object_output() {
    compare(
        "quantified_object_output",
        r#"
trait Lending<'a>{type Output:'a;fn get(&'a self)->Self::Output;}
struct Value(u8);
impl<'a>Lending<'a>for Value{type Output=&'a u8;fn get(&'a self)->&'a u8{&self.0}}
fn erase(value:&Value)->&dyn for<'a>Lending<'a,Output=&'a u8>{value}
fn read<'s>(value:&'s dyn for<'a>Lending<'a,Output=&'a u8>)->&'s u8{value.get()}
"#,
        true,
    );
    compare(
        "quantified_object_static_impl",
        r#"
trait Lending<'a>{type Output:'a;fn get(&'a self)->Self::Output;}
struct Value;
impl Lending<'static>for Value{type Output=&'static u8;fn get(&'static self)->&'static u8{&7}}
fn erase(value:&Value)->&dyn for<'a>Lending<'a,Output=&'a u8>{value}
"#,
        false,
    );
    compare(
        "quantified_object_static_equality",
        r#"
trait Lending<'a>{type Output:'a;fn get(&'a self)->Self::Output;}
struct Value;
impl<'a>Lending<'a>for Value{type Output=&'static u8;fn get(&'a self)->&'static u8{&7}}
fn erase(value:&Value)->&dyn for<'a>Lending<'a,Output=&'a u8>{value}
"#,
        false,
    );
}

#[test]
fn quantified_object_nested_writer_and_capture() {
    compare(
        "quantified_object_nested_writer",
        r#"
use std::io::Write;
trait Maker<'a>{type Writer:Write+'a;fn make(&'a self)->Self::Writer;}
struct Value;
impl<'a>Maker<'a>for Value{type Writer=Box<dyn Write+'a>;fn make(&'a self)->Self::Writer{Box::new(Vec::<u8>::new())}}
fn erase(value:&Value)->&dyn for<'a>Maker<'a,Writer=Box<dyn Write+'a>>{value}
fn write(value:&dyn for<'a>Maker<'a,Writer=Box<dyn Write+'a>>){value.make().write_all(b"x").unwrap();}
"#,
        true,
    );
    compare(
        "quantified_object_capture",
        r#"
trait Pair<'a>{type Output;fn pair(&'a self)->Self::Output;}
fn useit<'s>(value:&(dyn for<'a>Pair<'a,Output=(&'a u8,&'s u8)>+'s))->&'s u8{value.pair().1}
"#,
        true,
    );
    compare(
        "quantified_object_capture_escape",
        r#"
trait Pair<'a>{type Output;fn pair(&'a self)->Self::Output;}
fn useit<'s>(value:&(dyn for<'a>Pair<'a,Output=(&'a u8,&'s u8)>+'s))->&'static u8{value.pair().1}
"#,
        false,
    );
}

#[test]
fn conditional_nested_guard_drop() {
    compare(
        "conditional_nested_guard_drop",
        r#"
struct Resource(std::sync::RwLock<u8>);
struct Guard<'a>(std::sync::RwLockWriteGuard<'a,u8>);
fn check(flag:bool){let value=Resource(std::sync::RwLock::new(7));let guard=Guard(value.0.write().unwrap());if flag{drop(guard);drop(value);}}
"#,
        true,
    );
    compare(
        "conditional_guard_still_borrows",
        r#"
struct Resource(std::sync::RwLock<u8>);
struct Guard<'a>(std::sync::RwLockWriteGuard<'a,u8>);
fn check(flag:bool){let value=Resource(std::sync::RwLock::new(7));let guard=Guard(value.0.write().unwrap());if flag{drop(value);}drop(guard);}
"#,
        false,
    );
}

#[test]
fn auto_trait_projected_field_method() {
    compare(
        "auto_trait_projected_field_method",
        r#"
trait Array{type Item;}impl<T,const N:usize>Array for[T;N]{type Item=T;}
struct Storage<A:Array>(std::marker::PhantomData<A::Item>);
trait Ready{fn ready(&self){}}impl<T:Sync>Ready for T{}
fn check(value:Storage<[u8;8]>){value.ready();}
"#,
        true,
    );
    compare(
        "auto_trait_projected_field_not_sync",
        r#"
trait Array{type Item;}impl<T,const N:usize>Array for[T;N]{type Item=T;}
struct Storage<A:Array>(std::marker::PhantomData<A::Item>);
trait Ready{fn ready(&self){}}impl<T:Sync>Ready for T{}
fn check(value:Storage<[std::cell::Cell<u8>;8]>){value.ready();}
"#,
        false,
    );
}

#[test]
fn lifetime_proof_can_change_finite_impl_path() {
    compare(
        "lifetime_proof_finite_path",
        r#"
trait Mark<'a>{}
struct A;struct B;struct Base;struct Small;
struct Wrap<L,T>(L,T);
impl<'a,L,T:Mark<'a>>Mark<'a>for Wrap<L,T>{}
impl<'a>Mark<'a>for Base{}
impl<'a>Mark<'a>for Small where Wrap<B,Wrap<B,Wrap<B,Base>>>:Mark<'a>{}
fn require<'a,T:Mark<'a>>(_: &'a T){}
fn check<'a>(value:&'a Wrap<A,Small>){require(value);}
"#,
        true,
    );
}

#[test]
fn inherited_binding_for_explicit_self_argument() {
    compare(
        "inherited_self_binding",
        r#"
trait Checked: Sized+std::ops::Add<Self> {}
fn add<T:Checked<Output=T>>(a:T,b:T)->T{a+b}
trait First<A,B>{type Out;}
trait Second<A,B>:First<B,A>{}
fn reordered<T:Second<u8,u16,Out=u32>>(x:<T as First<u16,u8>>::Out)->u32{x}
"#,
        true,
    );
    compare(
        "inherited_wrong_arguments",
        r#"
trait First<A,B>{type Out;}
trait Second<A,B>:First<B,A>{}
fn wrong<T:Second<u8,u16,Out=u32>+First<u8,u16,Out=u64>>(x:<T as First<u16,u8>>::Out)->u64{x}
"#,
        false,
    );
    compare(
        "float_conversion_noninteger",
        "fn bad(x:f32)->bool{unsafe{x.to_int_unchecked()}}",
        false,
    );
}

#[test]
fn quantified_supertrait_region_free_output() {
    compare(
        "quantified_supertrait_output",
        r#"
trait Ops<R=Self,O=Self>:Sized+std::ops::Add<R,Output=O>{}
trait RefOps:Sized+for<'a>Ops<&'a Self>{}
impl<T>RefOps for T where T:for<'a>Ops<&'a T>{}
"#,
        true,
    );
    compare(
        "quantified_supertrait_wrong_output",
        r#"
trait Ops<R=Self,O=Self>:Sized+std::ops::Add<R,Output=O>{}
trait RefOps:Sized+for<'a>Ops<&'a Self,u32>{}
impl<T>RefOps for T where T:for<'a>Ops<&'a T,u64>{}
"#,
        false,
    );
}

#[test]
fn qualified_reference_argument_projection() {
    compare(
        "qualified_input_projection",
        r#"
trait Convert<T>{type Error;}
fn require<'a,T,E>()where T:Convert<&'a u8>,<T as Convert<&'a u8>>::Error:Into<E>{}
trait Output<T>{type Value;fn value(x:T)->Self::Value;}
impl<'a>Output<&'a u8>for(){type Value=&'a u8;fn value(x:&'a u8)->Self::Value{x}}
fn good<'a>(x:&'a u8)-><()as Output<&'a u8>>::Value{<()as Output<&'a u8>>::value(x)}
"#,
        true,
    );
    compare(
        "qualified_input_escape",
        r#"
trait Output<T>{type Value;fn value(x:T)->Self::Value;}
impl<'a>Output<&'a u8>for(){type Value=&'a u8;fn value(x:&'a u8)->Self::Value{x}}
fn bad<'a>(x:&'a u8)-><()as Output<&'static u8>>::Value{<()as Output<&'a u8>>::value(x)}
"#,
        false,
    );
}
#[test]
fn by_value_self_does_not_add_elision_candidates() {
    compare(
        "by_value_self_elision",
        r#"
struct S;
trait Tr{fn get(self,x:&u8)->&u8;}
impl Tr for &'static S{fn get(self,x:&u8)->&u8{x}}
"#,
        true,
    );
    compare(
        "by_value_self_elision_escape",
        r#"
struct S;
trait Tr{fn get(self,x:&u8)->&u8;}
impl Tr for &'static S{fn get(self,x:&u8)->&u8{x}}
fn bad(x:&u8)->&'static u8{(&S).get(x)}
"#,
        false,
    );
}

#[test]
fn qualified_trait_defaults_are_fixed_by_self() {
    compare(
        "qualified_trait_default",
        r#"
struct S;
trait Choice<R=Self>{fn choose(&self,r:&R)->u8;}
impl Choice for S{fn choose(&self,_:&S)->u8{1}}
impl Choice<&S> for S{fn choose(&self,_:&&S)->u8{2}}
fn call(x:&S)->u8{<S as Choice>::choose(x,&x)}
"#,
        true,
    );
}

#[test]
fn inherited_binding_rejects_unrepresented_competitors() {
    for (name, supers) in [
        ("concrete_after", "Base<Self>+Base<u8>"),
        ("concrete_before", "Base<u8>+Base<Self>"),
    ] {
        compare(
            name,
            &format!(
                "trait Base<T:?Sized>{{type Out;}} trait Child:{supers}{{}} fn test<T:Child<Out=u8>>(){{}}"
            ),
            false,
        );
    }
    compare(
        "inherited_competing_nested",
        r#"
trait Base<T:?Sized>{type Out;}
trait Extra:Base<u8>{}
trait Child:Base<Self>+Extra{}
fn test<T:Child<Out=u8>>(){}
"#,
        false,
    );
    compare(
        "inherited_lifetime_competitors",
        r#"
trait Base<'a>{type Out;}
trait Child<'a,'b>:Base<'a>+Base<'b>{}
fn test<'a,'b,T:Child<'a,'b,Out=u8>>(){}
"#,
        false,
    );
}

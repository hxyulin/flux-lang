//! Real consumers exercise distinct namespaces and empty statement expansions.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;

#[test]
fn consumer_lowering_matches_rustc() {
    let root = std::env::temp_dir().join(format!("flux-consumer-lowering-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "generic_borrow_with_region_bound",
            r#"trait Borrow<T>{fn borrow(&self)->&T;} trait TryR{type Error:core::error::Error;}trait R:TryR<Error=core::convert::Infallible>{}trait Sample{type X;fn sample<G:R+?Sized,B:Borrow<Self::X>>(b:B,g:&mut G)->Self::X;}struct S;impl Sample for S{type X=u32;fn sample<G:R+?Sized,B:Borrow<Self::X>>(b:B,_:&mut G)->Self::X{*b.borrow()}}fn main(){}"#,
            true,
        ),
        (
            "pointer_cast_expected_pointee",
            r#"fn cast(p:*mut u32)->*const u8{p as *const _}fn main(){}"#,
            true,
        ),
        (
            "const_cell_accessor",
            r#"const fn ptr<T>(c:&core::cell::UnsafeCell<T>)->*mut T{c.get()}fn main(){}"#,
            true,
        ),
        (
            "generic_borrow_method",
            r#"trait Borrow<T>{fn borrow(&self)->&T;} trait Sample{type X;fn sample<B:Borrow<Self::X>>(b:B)->Self::X;}struct S;impl Sample for S{type X=u32;fn sample<B:Borrow<Self::X>>(b:B)->Self::X{*b.borrow()}}fn main(){}"#,
            true,
        ),
        (
            "generic_unsized_method",
            r#"trait R{}trait Sample{fn sample<T:R+?Sized>(r:&mut T);}struct S;impl Sample for S{fn sample<T:R+?Sized>(_:&mut T){}}fn main(){}"#,
            true,
        ),
        (
            "generic_split_method",
            r#"trait Sample<T>{fn split(&mut self)->(&mut[T],&mut[T]);}impl<T> Sample<T> for [T]{fn split(&mut self)->(&mut[T],&mut[T]){self.split_at_mut(0)}}fn main(){}"#,
            true,
        ),
        (
            "double_pointer_cast",
            r#"fn load(p:*mut u32)->u32{unsafe{let a:&core::sync::atomic::AtomicU32=&*(p as *const _ as *const _);a.load(core::sync::atomic::Ordering::Relaxed)}}fn main(){}"#,
            true,
        ),
        (
            "nested_trait_static_contract",
            r#"trait Make { fn make<'a>(&'a self)->Option<impl Copy+'static>; } struct S(u32); impl Make for S { fn make<'a>(&'a self)->Option<impl Copy+'a>{Some(&self.0)} } fn main(){}"#,
            false,
        ),
        (
            "generic_union_write_escape",
            r#"union U<T:Copy>{value:T} fn leak()->U<&'static u32>{let n=42;let mut u=U{value:&0};u.value=&n;u} fn main(){}"#,
            false,
        ),
        (
            "nested_trait_named_borrow",
            r#"trait Make { fn make<'a>(&'a self)->Option<impl Copy+'a>; } struct S(u32); impl Make for S { fn make<'a>(&'a self)->Option<impl Copy+'a>{Some(&self.0)} } fn main(){}"#,
            true,
        ),
        (
            "nested_trait_equalities",
            r#"trait Make { fn make()->Option<impl Iterator<Item=u32>>; } struct S; impl Make for S { fn make()->Option<std::iter::Once<u32>>{Some(std::iter::once(42))} } fn main(){}"#,
            true,
        ),
        (
            "nested_trait_wrong_equalities",
            r#"trait Make { fn make()->Option<impl Iterator<Item=u32>>; } struct S; impl Make for S { fn make()->Option<std::iter::Once<u8>>{Some(std::iter::once(42))} } fn main(){}"#,
            false,
        ),
        (
            "nested_trait_borrowed",
            r#"trait Make { fn make(&self)->Option<impl Copy+'_>; } struct S(u32); impl Make for S { fn make(&self)->Option<impl Copy+'_>{Some(&self.0)} } fn main(){}"#,
            true,
        ),
        (
            "generic_union_read_escape",
            r#"union U<T:Copy>{value:T} fn leak()->&'static u32{let n=42;let u=U{value:&n};unsafe{u.value}} fn main(){}"#,
            false,
        ),
        (
            "generic_union_manuallydrop",
            r#"union U<T>{value:core::mem::ManuallyDrop<T>} fn main(){let u=U{value:core::mem::ManuallyDrop::new(String::from("hello"))};let s=core::mem::ManuallyDrop::into_inner(unsafe{u.value});assert_eq!(s.len(),5);}"#,
            true,
        ),
        (
            "nested_trait_option",
            r#"trait Make { fn make() -> Option<impl Copy>; } struct S; impl Make for S { fn make()->Option<u32>{Some(42)} } fn main(){let _=S::make();}"#,
            true,
        ),
        (
            "nested_trait_pair",
            r#"trait Make { fn make() -> (impl Copy, Option<impl Clone>); } struct S; impl Make for S { fn make()->(u32,Option<String>){(42,Some(String::new()))} } fn main(){let _=S::make();}"#,
            true,
        ),
        (
            "nested_trait_bad_bound",
            r#"trait Make { fn make() -> Option<impl Copy>; } struct S; impl Make for S { fn make()->Option<String>{None} } fn main(){}"#,
            false,
        ),
        (
            "nested_trait_bad_shape",
            r#"trait Make { fn make() -> Option<impl Copy>; } struct S; impl Make for S { fn make()->Result<u32,()>{Ok(42)} } fn main(){}"#,
            false,
        ),
        (
            "generic_union_copy",
            r#"union U<T:Copy>{value:T} fn main(){let u=U{value:42u32};assert_eq!(unsafe{u.value},42);}"#,
            true,
        ),
        (
            "generic_union_noncopy",
            r#"union U<T>{value:T} fn main(){}"#,
            false,
        ),
        (
            "generic_union_const",
            r#"union U<const N:usize>{bytes:[u8;N]} fn main(){let u=U{bytes:[1u8;4]};assert_eq!(unsafe{u.bytes}[0],1);}"#,
            true,
        ),
        (
            "generic_union_reference_escape",
            r#"union U<T:Copy>{value:T} fn leak()->U<&'static u32>{let n=42;U{value:&n}} fn main(){}"#,
            false,
        ),
        (
            "generic_union_reference",
            r#"union U<T:Copy>{value:T} fn main(){let n=42;let u=U{value:&n};assert_eq!(*unsafe{u.value},42);}"#,
            true,
        ),
        (
            "forwarded_index_output",
            r#"use std::ops::{Index,IndexMut};
struct Array<T>(Vec<T>);
impl<T,I> Index<I> for Array<T> where [T]:Index<I>{type Output=<[T]as Index<I>>::Output;fn index(&self,i:I)->&Self::Output{Index::index(self.0.as_slice(),i)}}
impl<T,I> IndexMut<I> for Array<T> where [T]:IndexMut<I>{fn index_mut(&mut self,i:I)->&mut Self::Output{IndexMut::index_mut(self.0.as_mut_slice(),i)}}
fn main(){let mut x=Array(vec![1,2]);x[0]=42;println!("{}",x[0]);}
"#,
            true,
        ),
        (
            "forwarded_index_output_escape",
            r#"use std::ops::{Index,IndexMut};
struct Array<T>(Vec<T>);
impl<T,I> Index<I> for Array<T> where [T]:Index<I>{type Output=<[T]as Index<I>>::Output;fn index(&self,i:I)->&Self::Output{Index::index(self.0.as_slice(),i)}}
impl<T,I> IndexMut<I> for Array<T> where [T]:IndexMut<I>{fn index_mut(&mut self,i:I)->&mut Self::Output{IndexMut::index_mut(self.0.as_mut_slice(),i)}}
fn leak<T,I>(x:&Array<T>,index:I)->&'static <[T] as Index<I>>::Output where [T]:Index<I>{&x[index]}fn main(){}"#,
            false,
        ),
        (
            "associated_slice_supertrait",
            r#"trait Offset<T>{}
trait Stream: Offset<Self::Checkpoint>{type Checkpoint;type Slice;}
impl<'a,T> Offset<&'a[T]> for &'a[T]{}
impl<'a,T:Clone+std::fmt::Debug> Stream for &'a[T]{type Checkpoint=Self;type Slice=Self;}
trait Update:Stream{fn update(self,inner:Self::Slice)->Self;}
impl<T:Clone+std::fmt::Debug> Update for &[T]{fn update(self,inner:Self::Slice)->Self{inner}}
fn main(){}
"#,
            true,
        ),
        (
            "associated_slice_supertrait_static",
            r#"trait Offset<T>{}
trait Stream: Offset<Self::Checkpoint>{type Checkpoint;type Slice;}
impl<'a:'static,T> Offset<&'a[T]> for &'a[T]{}
impl<'a,T:Clone+std::fmt::Debug> Stream for &'a[T]{type Checkpoint=Self;type Slice=Self;}
trait Update:Stream{fn update(self,inner:Self::Slice)->Self;}
impl<T:Clone+std::fmt::Debug> Update for &[T]{fn update(self,inner:Self::Slice)->Self{inner}}
fn main(){}
"#,
            false,
        ),
        (
            "dynamic_gat_receiver",
            r#"trait Write{}
trait Lock {type Writer<'w>:Write+'w where Self:'w;fn locked(&mut self)->Self::Writer<'_>;}
impl<T:Write+?Sized> Write for &mut T{}
impl Lock for dyn Write {type Writer<'w>=&'w mut Self;fn locked(&mut self)->Self::Writer<'_>{self}}
fn main(){}
"#,
            true,
        ),
        (
            "dynamic_gat_receiver_escape",
            r#"trait Write{}
trait Lock {type Writer<'w>:Write+'w where Self:'w;fn locked(&mut self)->Self::Writer<'_>;}
impl<T:Write+?Sized> Write for &mut T{}
impl Lock for dyn Write {type Writer<'w>=&'w mut Self;fn locked(&mut self)->Self::Writer<'_>{self}}
fn leak(value:&mut(dyn Write+'static))->&'static mut(dyn Write+'static){value.locked()}fn main(){}
"#,
            false,
        ),
        (
            "dynamic_gat_borrowed_receiver",
            r#"trait Write{}
trait Lock {type Writer<'w>:Write+'w where Self:'w;fn locked(&mut self)->Self::Writer<'_>;}
impl<T:Write+?Sized> Write for &mut T{}
impl<'o> Lock for dyn Write+'o {type Writer<'w>=&'w mut Self where Self:'w;fn locked(&mut self)->Self::Writer<'_>{self}}
fn main(){}
"#,
            true,
        ),
        (
            "opaque_into_iterator",
            r#"struct Tool(Vec<(String,String)>);
impl Tool {fn env(&self)->impl IntoIterator<Item=&(String,String)>{&self.0}}
fn convert(tool:Tool)->Vec<(String,String)>{tool.env().into_iter().map(|(k,v)|(k.clone(),v.clone())).collect()}
fn main(){assert_eq!(convert(Tool(vec![("a".into(),"b".into())]))[0].0,"a");}
"#,
            true,
        ),
        (
            "opaque_iterator_blanket",
            r#"fn values(x:&[u32])->impl Iterator<Item=&u32>{x.iter()}fn copy(x:&[u32])->Vec<u32>{values(x).into_iter().copied().collect()}fn main(){}"#,
            true,
        ),
        (
            "opaque_into_iter_escape",
            r#"fn values(x:&[u32])->impl IntoIterator<Item=&u32>{x}fn leak(x:&[u32])->impl Iterator<Item=&'static u32>{values(x).into_iter()}fn main(){}"#,
            false,
        ),
        (
            "opaque_into_iter_not_iterator",
            r#"fn values(x:&[u32])->impl IntoIterator<Item=&u32>{x}fn need<I:Iterator>(_:I){}fn bad(x:&[u32]){need(values(x));}fn main(){}"#,
            false,
        ),
        (
            "pattern_namespaces",
            r#"enum Class { Bytes(u32), Unicode(u32) }
enum Kind { Class(Class), Empty }
fn inspect(kind: Kind) -> u32 {
    use crate::{Class, Kind::*};
    match kind {
        Class(Class::Bytes(value)) => value,
        Class(Class::Unicode(value)) => value + 1,
        Empty => 0,
    }
}
fn main() {
    assert_eq!(inspect(Kind::Class(Class::Bytes(41))), 41);
    println!("{}", inspect(Kind::Class(Class::Unicode(41))));
}
"#,
            true,
        ),
        (
            "record_variant_namespace",
            r#"enum Class{A}enum Kind{Class{value:Class}}fn f(x:Kind){use crate::{Class,Kind::*};match x{Class{value:Class::A}=>()}}fn main(){}"#,
            false,
        ),
        (
            "record_struct_namespace",
            r#"struct Class{value:u32}fn Class()->u32{1}fn f(x:Class)->u32{let Class{value}=x;value}fn main(){}"#,
            true,
        ),
        (
            "unit_variant_namespace",
            r#"enum Class{A}enum Kind{Class}fn f(x:Kind){use crate::{Class,Kind::*};match x{Class=>()}}fn main(){}"#,
            true,
        ),
        (
            "alias_tuple_pattern",
            r#"struct S(u32);type A=S;fn f(x:S){let A(_)=x;}fn main(){}"#,
            false,
        ),
        (
            "private_tuple_pattern",
            r#"mod m{pub struct S(u32);pub fn make()->S{S(42)}}fn main(){let m::S(_)=m::make();}"#,
            false,
        ),
        (
            "direct_empty_tail",
            r#"macro_rules! empty{()=>{}} macro_rules! value{()=>{42u32}} macro_rules! item{()=>{fn local(){}}}
fn f()->u32{value!{} empty!{}}
fn main(){}
"#,
            true,
        ),
        (
            "nested_empty_tail",
            r#"macro_rules! empty{()=>{}} macro_rules! value{()=>{42u32}} macro_rules! item{()=>{fn local(){}}}
macro_rules! outer{()=>{value!{} empty!{}}}fn f()->u32{outer!{}}
fn main(){}
"#,
            true,
        ),
        (
            "block_empty_tail",
            r#"macro_rules! empty{()=>{}} macro_rules! value{()=>{42u32}} macro_rules! item{()=>{fn local(){}}}
fn f()->u32{{42u32} empty!{}}
fn main(){}
"#,
            true,
        ),
        (
            "if_empty_tail",
            r#"macro_rules! empty{()=>{}} macro_rules! value{()=>{42u32}} macro_rules! item{()=>{fn local(){}}}
fn f()->u32{if true{42u32}else{0} empty!{}}
fn main(){}
"#,
            true,
        ),
        (
            "semi_empty_tail",
            r#"macro_rules! empty{()=>{}} macro_rules! value{()=>{42u32}} macro_rules! item{()=>{fn local(){}}}
fn f()->u32{value!{}; empty!{}}
fn main(){}
"#,
            false,
        ),
        (
            "item_tail",
            r#"macro_rules! empty{()=>{}} macro_rules! value{()=>{42u32}} macro_rules! item{()=>{fn local(){}}}
fn f()->u32{value!{} item!{}}
fn main(){}
"#,
            false,
        ),
        (
            "item_before_value",
            r#"macro_rules! empty{()=>{}} macro_rules! value{()=>{42u32}} macro_rules! item{()=>{fn local(){}}}
fn f()->u32{item!{} value!{} empty!{}}
fn main(){}
"#,
            true,
        ),
        (
            "semi_macro_tail",
            r#"macro_rules! empty{()=>{}} macro_rules! value{()=>{42u32}} macro_rules! item{()=>{fn local(){}}}
macro_rules! semi{()=>{;}} fn f()->u32{value!{} semi!{}}
fn main(){}
"#,
            false,
        ),
        (
            "unit_expr_tail",
            r#"macro_rules! empty{()=>{}} macro_rules! value{()=>{42u32}} macro_rules! item{()=>{fn local(){}}}
macro_rules! unit{()=>{()}}fn f()->u32{value!{} unit!{}}
fn main(){}
"#,
            false,
        ),
        (
            "direct_value_then_item",
            r#"macro_rules! empty{()=>{}} macro_rules! value{()=>{42u32}} macro_rules! item{()=>{fn local(){}}}
fn f()->u32{value!{} fn local(){}}
fn main(){}
"#,
            false,
        ),
        (
            "cfg_group_tail",
            r#"macro_rules! empty{()=>{}} macro_rules! value{()=>{42u32}} macro_rules! item{()=>{fn local(){}}}
macro_rules! emit{($($t:tt)*)=>{$($t)*}} macro_rules! group{()=>{#[cfg(all())]emit!{42u32} #[cfg(any())]emit!{missing} empty!{}}}fn f()->u32{group!{}}
fn main(){}
"#,
            true,
        ),
        (
            "non_tail_value",
            r#"macro_rules! value{()=>{42u32}}
fn main(){value!{} let _x=0;}"#,
            false,
        ),
        (
            "value_before_expr",
            r#"macro_rules! value{()=>{42u32}}
fn f()->u32{value!{} 1u32}fn main(){}"#,
            false,
        ),
        (
            "if_before_expr",
            r#"macro_rules! value{()=>{42u32}}
fn f()->u32{if true{42u32}else{0} 1u32}fn main(){}"#,
            false,
        ),
        (
            "factory_object",
            r#"struct Guard<'a,T:Send,F:Fn()->T>(&'a (),std::marker::PhantomData<(T,F)>);type Factory=Box<dyn Fn()->u32+Send>;struct Holder<'a>(Guard<'a,u32,Factory>);fn main(){}"#,
            true,
        ),
        (
            "factory_object_borrowed",
            r#"struct Guard<'a,T:Send,F:Fn()->T>(&'a (),std::marker::PhantomData<(T,F)>);type Factory<'a>=Box<dyn Fn()->u32+Send+'a>;struct Holder<'a>(Guard<'a,u32,Factory<'a>>);fn main(){}"#,
            true,
        ),
        (
            "factory_object_wrong_kind",
            r#"struct Guard<'a,T:Send,F:Fn()->T>(&'a (),std::marker::PhantomData<(T,F)>);type Factory=Box<dyn FnMut()->u32+Send>;struct Holder<'a>(Guard<'a,u32,Factory>);fn main(){}"#,
            false,
        ),
        (
            "factory_object_wrong_output",
            r#"struct Guard<'a,T:Send,F:Fn()->T>(&'a (),std::marker::PhantomData<(T,F)>);type Factory=Box<dyn Fn()->u8+Send>;struct Holder<'a>(Guard<'a,u32,Factory>);fn main(){}"#,
            false,
        ),
        (
            "factory_object_output_not_send",
            r#"struct Guard<'a,T:Send,F:Fn()->T>(&'a (),std::marker::PhantomData<(T,F)>);type Factory=Box<dyn Fn()->std::rc::Rc<u32>+Send>;struct Holder<'a>(Guard<'a,std::rc::Rc<u32>,Factory>);fn main(){}"#,
            false,
        ),
        (
            "ranked_operator_argument",
            r#"fn f<X>(mut x:X,y:&X)where X:for<'a>std::ops::AddAssign<&'a X>{x+=y;}fn main(){f(1u32,&2u32);}"#,
            true,
        ),
        (
            "ranked_argument_not_static",
            r#"trait Add<R>{}fn need<T:'static>(_:T){}fn f<X>(x:X)where X:for<'a>Add<&'a X>{need(x)}fn main(){}"#,
            false,
        ),
        (
            "cast_ty_shift",
            r##"macro_rules! shift{($t:ty)=>{1 as $t << 5}}fn main(){assert_eq!(shift!(u32),32);}"##,
            true,
        ),
        (
            "cast_ident_shift",
            r##"macro_rules! shift{($t:ident)=>{1 as $t << 5}}fn main(){let _=shift!(u32);}"##,
            false,
        ),
        (
            "opaque_type_forward",
            r##"macro_rules! inner{((u32))=>{false};($t:ty)=>{true};}macro_rules! outer{($t:ty)=>{inner!($t)}}fn main(){assert!(outer!(u32));}"##,
            true,
        ),
        (
            "opaque_type_no_destructure",
            r##"macro_rules! inner{((u32))=>{true};}macro_rules! outer{($t:ty)=>{inner!($t)}}fn main(){let _=outer!(u32);}"##,
            false,
        ),
        (
            "opaque_type_stringify",
            r##"macro_rules! outer{($t:ty)=>{stringify!($t)}}fn main(){assert_eq!(outer!(u32),"u32");}"##,
            true,
        ),
    ] {
        if !case_filter::selected(name) {
            continue;
        }
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
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
        let (_, _, hir) = flux_hir::lower_text(&path, source);
        let mut errors = hir.diagnostics.clone();
        if errors.is_empty() {
            let typed = flux_mir::check_file(&hir);
            errors.extend(typed.diagnostics.clone());
            if errors.is_empty() {
                errors.extend(flux_mir::build_file(&hir, &typed).diagnostics);
            }
        }
        assert_eq!(errors.is_empty(), accepted, "Flux {name}: {errors:?}");
    }
    std::fs::remove_dir_all(root).unwrap();
}

//! Coherence follows impossible blanket prerequisites only in closed orphan worlds.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;
#[test]
fn blanket_adapter_intersections_match_rustc() {
    let directory =
        std::env::temp_dir().join(format!("flux-adapter-coherence-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    for (name, source, accepted) in [
        (
            "generic_local_callable_absence",
            "trait Flag<L,R>{}struct Wrap<F,T>(F,core::marker::PhantomData<T>);impl<L,R,F:FnMut(&L,&R)->bool>Flag<L,R>for Wrap<F,bool>{}impl<T,F:FnMut(&T,&T)->bool>Flag<T,T>for F{}",
            true,
        ),
        (
            "boxed_callable_overlap",
            "trait Conv<T>{}impl<T>Conv<T>for T{}impl<T:Fn()>Conv<T>for Box<dyn Fn()>{}",
            false,
        ),
        (
            "generic_local_unconditional_overlap",
            "trait Flag<L,R>{}struct Wrap<F,T>(F,core::marker::PhantomData<T>);impl<L,R,F>Flag<L,R>for Wrap<F,bool>{}impl<T>Flag<T,T>for T{}",
            false,
        ),
        (
            "generic_cow_blanket_conversion",
            "use std::borrow::Cow;struct Paint<'a,S:ToOwned+?Sized>{text:Cow<'a,S>}impl<'a,I,S:'a+ToOwned+?Sized>From<I>for Paint<'a,S>where I:Into<Cow<'a,S>>,S::Owned:std::fmt::Debug{fn from(value:I)->Self{Self{text:value.into()}}}",
            true,
        ),
        (
            "generic_cow_specific_conversion_overlaps",
            "use std::borrow::Cow;struct Paint<'a,S:ToOwned+?Sized>{text:Cow<'a,S>}impl<'a>From<Paint<'a,str>>for Cow<'a,str>{fn from(value:Paint<'a,str>)->Self{value.text}}impl<'a,I,S:'a+ToOwned+?Sized>From<I>for Paint<'a,S>where I:Into<Cow<'a,S>>,S::Owned:std::fmt::Debug{fn from(value:I)->Self{Self{text:value.into()}}}",
            false,
        ),
        (
            "generic_cow_existing_conversion_overlaps",
            "use std::borrow::Cow;struct Paint<'a,S:ToOwned+?Sized>{text:Cow<'a,S>}impl<'a,S:ToOwned+?Sized>From<Paint<'a,S>>for Cow<'a,S>{fn from(value:Paint<'a,S>)->Self{value.text}}impl<'a,I,S:'a+ToOwned+?Sized>From<I>for Paint<'a,S>where I:Into<Cow<'a,S>>,S::Owned:std::fmt::Debug{fn from(value:I)->Self{Self{text:value.into()}}}",
            false,
        ),
        (
            "generic_projection_locating_checkpoint",
            "trait Stream{type Checkpoint;}struct Checkpoint<I,S>(I,core::marker::PhantomData<S>);struct Locating<I>{initial:I,input:I}impl<I:Stream>Stream for Locating<I>{type Checkpoint=Checkpoint<I::Checkpoint,Self>;}trait Offset<Start=Self>{}impl<I>Offset for Locating<I>{}impl<I:Stream>Offset<<Locating<I>as Stream>::Checkpoint>for Locating<I>{}",
            true,
        ),
        (
            "generic_projection_tuple_checkpoint",
            "trait Stream{type Checkpoint;}struct Checkpoint<I,S>(I,core::marker::PhantomData<S>);impl<I:Stream>Stream for(I,usize){type Checkpoint=Checkpoint<(I::Checkpoint,usize),Self>;}trait Offset<Start=Self>{}impl<I>Offset for(I,usize){}impl<I:Stream>Offset<<(I,usize)as Stream>::Checkpoint>for(I,usize){}",
            true,
        ),
        (
            "generic_projection_stateful_checkpoint",
            "trait Stream{type Checkpoint;}struct Checkpoint<I,S>(I,core::marker::PhantomData<S>);struct Stateful<I,S>(I,S);impl<I:Stream,S:core::fmt::Debug>Stream for Stateful<I,S>{type Checkpoint=Checkpoint<I::Checkpoint,Self>;}trait Offset<Start=Self>{}impl<I,S>Offset for Stateful<I,S>{}impl<I:Stream,S:core::fmt::Debug>Offset<<Stateful<I,S>as Stream>::Checkpoint>for Stateful<I,S>{}",
            true,
        ),
        (
            "generic_projection_token_checkpoint",
            "trait Stream{type Checkpoint;}struct Checkpoint<I,S>(I,core::marker::PhantomData<S>);struct TokenSlice<'a,T>(&'a[T]);impl<'a,T:Clone+core::fmt::Debug>Stream for TokenSlice<'a,T>{type Checkpoint=Checkpoint<&'a[T],Self>;}trait Offset<Start=Self>{}impl<T>Offset for TokenSlice<'_,T>{}impl<'a,T:Clone+core::fmt::Debug>Offset<<TokenSlice<'a,T>as Stream>::Checkpoint>for TokenSlice<'a,T>{}",
            true,
        ),
        (
            "generic_unsigned_range_adapter",
            "struct Parser<T>(T);impl<T,B:core::ops::RangeBounds<u64>>From<B>for Parser<T>{fn from(_:B)->Self{panic!()}}",
            true,
        ),
        (
            "generic_projection_competing_candidate_stays_opaque",
            "trait Stream{type Out;}struct W<T>(T);struct Nope(String);impl<T:Copy>Stream for W<T>{type Out=u8;}impl Stream for W<Nope>{type Out=u16;}trait Conv<X>{}impl<T:Copy>Conv<u16>for W<T>{}impl<T:Copy>Conv<<W<T>as Stream>::Out>for W<T>{}",
            false,
        ),
        (
            "generic_projection_rhs_parameter",
            "trait Stream{type Out;}struct W<T>(T);impl<T>Stream for W<T>{type Out=T;}impl<T>From<<W<T>as Stream>::Out>for W<T>{fn from(t:T)->Self{W(t)}}",
            true,
        ),
        (
            "generic_ranged_tryfrom_adapter",
            "struct Parser<T>(core::marker::PhantomData<T>);impl<T:TryFrom<i64>+Clone+Send+Sync,B:core::ops::RangeBounds<i64>>From<B>for Parser<T>{fn from(_:B)->Self{Parser(core::marker::PhantomData)}}",
            true,
        ),
        (
            "generic_range_adapter",
            "struct Parser<T>(T);impl<T,B:core::ops::RangeBounds<i64>>From<B>for Parser<T>{fn from(_:B)->Self{panic!()}}",
            true,
        ),
        (
            "generic_range_adapter_actual_overlap",
            "struct Parser<T>(T);impl<T>core::ops::RangeBounds<i64>for Parser<T>{fn start_bound(&self)->core::ops::Bound<&i64>{core::ops::Bound::Unbounded}fn end_bound(&self)->core::ops::Bound<&i64>{core::ops::Bound::Unbounded}}impl<T,B:core::ops::RangeBounds<i64>>From<B>for Parser<T>{fn from(_:B)->Self{panic!()}}",
            false,
        ),
        (
            "generic_conditional_candidate_stays_open",
            "trait Flag{}trait Bound{}struct Parser<T>(T);impl<T:Flag>Bound for Parser<T>{}impl<T,B:Bound>From<B>for Parser<T>{fn from(_:B)->Self{panic!()}}",
            false,
        ),
        (
            "generic_specialized_candidate_stays_open",
            "trait Bound{}struct Parser<T>(T);impl Bound for Parser<u32>{}impl<T,B:Bound>From<B>for Parser<T>{fn from(_:B)->Self{panic!()}}",
            false,
        ),
        (
            "generic_projection_slice_checkpoint",
            "trait Stream{type Checkpoint;}struct Checkpoint<I,S>(I,core::marker::PhantomData<S>);impl<T>Stream for &[T]{type Checkpoint=Checkpoint<Self,Self>;}trait Offset<Start=Self>{}impl<T>Offset for &[T]{}impl<'a,T>Offset<<&'a[T]as Stream>::Checkpoint>for &'a[T]{}",
            true,
        ),
        (
            "generic_projection_wrapper_checkpoint",
            "trait Stream{type Checkpoint;}struct Checkpoint<I,S>(I,core::marker::PhantomData<S>);struct Partial<I>(I);impl<I:Stream>Stream for Partial<I>{type Checkpoint=Checkpoint<I::Checkpoint,Self>;}trait Offset<Start=Self>{}impl<I>Offset for Partial<I>{}impl<I:Stream>Offset<<Partial<I>as Stream>::Checkpoint>for Partial<I>{}",
            true,
        ),
        (
            "generic_projection_selected_rhs_overlap",
            "trait Stream{type Checkpoint;}struct Wrapper<T>(T);impl<T>Stream for Wrapper<T>{type Checkpoint=Self;}trait Offset<Start=Self>{}impl<T>Offset for Wrapper<T>{}impl<T>Offset<<Wrapper<T>as Stream>::Checkpoint>for Wrapper<T>{}",
            false,
        ),
        (
            "generic_projection_header_remains_abstract",
            "trait Stream{type Checkpoint;}struct Wrapper<T>(T);trait Offset<Start=Self>{}impl<T>Offset for Wrapper<T>{}impl<T:Stream>Offset<T::Checkpoint>for Wrapper<T>{}",
            false,
        ),
        (
            "conditional_concrete_impossible",
            "trait Flag{}trait Bound{}struct Wrap<T>(T);impl<T:Flag>Bound for Wrap<T>{}trait Conv<T>{}impl<T>Conv<T>for T{}impl<T:Bound>Conv<T>for Wrap<u32>{}",
            true,
        ),
        (
            "conditional_concrete_actual_overlap",
            "trait Flag{}impl Flag for u32{}trait Bound{}struct Wrap<T>(T);impl<T:Flag>Bound for Wrap<T>{}trait Conv<T>{}impl<T>Conv<T>for T{}impl<T:Bound>Conv<T>for Wrap<u32>{}",
            false,
        ),
        (
            "one_possible_candidate_keeps_overlap",
            "trait Flag{}trait Bound{}struct Wrap<T>(T);impl<T:Flag>Bound for Wrap<T>{}impl Bound for Wrap<u32>{}trait Conv<T>{}impl<T>Conv<T>for T{}impl<T:Bound>Conv<T>for Wrap<u32>{}",
            false,
        ),
        (
            "unresolved_projection_overlap",
            "trait Family{type Out;}struct Wrap<T>(T);impl<T:Family>From<T::Out>for Wrap<T>{fn from(_:T::Out)->Self{panic!()}}",
            false,
        ),
        (
            "unresolved_projection_equality_overlap",
            "trait Family{type Out;}struct Wrap<T>(T);impl<T:Family<Out=T>>From<T::Out>for Wrap<T>{fn from(t:T::Out)->Self{Wrap(t)}}",
            false,
        ),
        (
            "closed_projection_headers_disjoint",
            "trait Family{type Out;}impl Family for u8{type Out=u8;}impl Family for u16{type Out=u16;}struct Target;impl From<<u8 as Family>::Out>for Target{fn from(_:u8)->Self{Target}}impl From<<u16 as Family>::Out>for Target{fn from(_:u16)->Self{Target}}",
            true,
        ),
        (
            "closed_projection_headers_overlap",
            "trait Family{type Out;}impl Family for u8{type Out=u32;}impl Family for u16{type Out=u32;}struct Target;impl From<<u8 as Family>::Out>for Target{fn from(_:u32)->Self{Target}}impl From<<u16 as Family>::Out>for Target{fn from(_:u32)->Self{Target}}",
            false,
        ),
        (
            "projected_candidate_keeps_overlap",
            "trait Family{type Out;}struct Local;impl Family for Local{type Out=u32;}trait Bound<P>{}struct Wrap<T>(T);impl<T:Family>Bound<T::Out>for Wrap<T>{}trait Conv<S,P>{}impl<S,P>Conv<S,P>for S{}impl<S:Bound<u32>>Conv<S,()>for Wrap<Local>{}",
            false,
        ),
        (
            "into_from_identity",
            "struct Other;enum Target{A}impl<S:Into<Other>>From<S>for Target{fn from(_:S)->Self{Self::A}}",
            true,
        ),
        (
            "into_real_overlap",
            "struct Other;struct Target;impl From<Target>for Other{fn from(_:Target)->Self{Other}}impl<S:Into<Other>>From<S>for Target{fn from(_:S)->Self{Target}}",
            false,
        ),
        (
            "local_blanket_callable",
            "trait Parse{}impl<F:Fn()->u32>Parse for F{}struct Parser;impl<P:Parse>From<P>for Parser{fn from(_:P)->Self{Parser}}",
            true,
        ),
        (
            "local_callable_actual_overlap",
            "trait Parse{}impl<F:Fn()->u32>Parse for F{}struct Parser;impl Parse for Parser{}impl<P:Parse>From<P>for Parser{fn from(_:P)->Self{Parser}}",
            false,
        ),
        (
            "resettable_adapter",
            "enum Reset<T>{Value(T),Empty}trait Conv<T>{}impl<T>Conv<T>for Reset<T>{}struct Value;impl<I:Into<Value>>Conv<Value>for I{}",
            true,
        ),
        (
            "resettable_actual_overlap",
            "enum Reset<T>{Value(T),Empty}trait Conv<T>{}impl<T>Conv<T>for Reset<T>{}struct Value;impl From<Reset<Value>>for Value{fn from(_:Reset<Value>)->Self{Value}}impl<I:Into<Value>>Conv<Value>for I{}",
            false,
        ),
        (
            "iterator_projection_adapter",
            "struct Parser;impl<I,T>From<I>for Parser where I:IntoIterator<Item=T>,T:Into<u32>{fn from(_:I)->Self{Parser}}",
            true,
        ),
        (
            "iterator_projection_actual_overlap",
            "struct Parser;impl Iterator for Parser{type Item=u32;fn next(&mut self)->Option<u32>{None}}impl<I,T>From<I>for Parser where I:IntoIterator<Item=T>,T:Into<u32>{fn from(_:I)->Self{Parser}}",
            false,
        ),
        (
            "callable_projection_adapter",
            "trait Parse{}impl<T,E,F>Parse for F where F:Fn(&str)->Result<T,E>,T:Into<u32>{}struct Parser;impl<P:Parse>From<P>for Parser{fn from(_:P)->Self{Parser}}",
            true,
        ),
        (
            "nested_adapters",
            "trait Bound{}impl<T:Into<Leaf>>Bound for T{}struct Leaf;struct Target;impl<T:Bound>From<T>for Target{fn from(_:T)->Self{Target}}",
            true,
        ),
        (
            "nested_actual_overlap",
            "trait Bound{}impl<T:Into<Leaf>>Bound for T{}struct Leaf;struct Target;impl From<Target>for Leaf{fn from(_:Target)->Self{Leaf}}impl<T:Bound>From<T>for Target{fn from(_:T)->Self{Target}}",
            false,
        ),
        (
            "upstream_unknown",
            "trait Conv<T>{}impl<T>Conv<T>for T{}impl<T:Iterator>Conv<T>for u32{}",
            false,
        ),
        (
            "upstream_blanket_unknown",
            "trait Bound{}impl<T:Iterator>Bound for T{}trait Conv<T>{}impl<T>Conv<T>for T{}impl<T:Bound>Conv<T>for u32{}",
            false,
        ),
        (
            "foreign_constructor_covers_local",
            "struct Local;trait Conv<T>{}impl<T>Conv<T>for T{}impl<T:Iterator>Conv<T>for Vec<Local>{}",
            false,
        ),
        (
            "raw_pointer_does_not_confer_locality",
            "struct Local;trait Conv<T>{}impl<T>Conv<T>for T{}impl<T:Iterator>Conv<T>for *const Local{}",
            false,
        ),
        (
            "reference_confers_locality",
            "struct Local;trait Conv<T>{}impl<T>Conv<T>for T{}impl<T:Iterator>Conv<T>for &Local{}",
            true,
        ),
        (
            "box_confers_locality",
            "struct Local;trait Conv<T>{}impl<T>Conv<T>for T{}impl<T:Iterator>Conv<T>for Box<Local>{}",
            true,
        ),
        (
            "generic_trait_input_stays_open",
            "trait Bound<P>{}trait Conv<S,P>{}impl<S,P>Conv<S,P>for S{}struct W;impl<P,S:Bound<P>>Conv<S,P>for W{}",
            false,
        ),
        (
            "generic_self_stays_open",
            "trait Bound{}trait Conv<S,P>{}impl<S,P>Conv<S,P>for S{}impl<P,S:Bound>Conv<S,P>for Box<P>{}",
            false,
        ),
        (
            "conditional_existing_impl",
            "trait Flag{}trait Bound{}struct W<T>(T);impl<T:Flag>Bound for W<T>{}trait Conv<S,P>{}impl<S,P>Conv<S,P>for S{}impl<P,S:Bound>Conv<S,P>for W<P>{}",
            false,
        ),
        (
            "inductive_cycle_stays_unknown",
            "trait A{}trait B{}impl<T:B>A for T{}impl<T:A>B for T{}struct Target;impl<S:A>From<S>for Target{fn from(_:S)->Self{Target}}",
            false,
        ),
        (
            "builtin_fn_positive",
            "trait Parse{}impl<F:Fn()->u32>Parse for F{}trait Conv<T>{}impl<T>Conv<T>for T{}impl<T:Parse>Conv<T>for fn()->u32{}",
            false,
        ),
        (
            "builtin_copy_positive",
            "trait Bound{}impl<T:Copy>Bound for T{}trait Conv<T>{}impl<T>Conv<T>for T{}impl<T:Bound>Conv<T>for fn(){}",
            false,
        ),
        (
            "builtin_auto_positive",
            "trait Bound{}impl<T:Send>Bound for T{}struct W;trait Conv<T>{}impl<T>Conv<T>for T{}impl<T:Bound>Conv<T>for W{}",
            false,
        ),
        (
            "dynamic_builtin_positive",
            "trait Bound{}trait Obj:Bound{}trait Conv<T:?Sized>{}impl<T:?Sized>Conv<T>for T{}impl<T:Bound+?Sized>Conv<T>for dyn Obj{}",
            false,
        ),
    ] {
        if !case_filter::selected(name) {
            continue;
        }
        let source = format!("{source}fn main(){{}}");
        let path = directory.join(format!("{name}.rs"));
        std::fs::write(&path, &source).unwrap();
        let rustc = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(directory.join("case.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            rustc.status.success(),
            accepted,
            "rustc {name}: {}",
            String::from_utf8_lossy(&rustc.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text(&path, &source);
        let mut diagnostics = hir.diagnostics.clone();
        if diagnostics.is_empty() {
            let (_, errors) = flux_ty::signatures_checked(&hir.items, true, None);
            diagnostics.extend(errors);
        }
        assert_eq!(
            diagnostics.is_empty(),
            accepted,
            "flux {name}: {diagnostics:?}"
        );
    }
}

#[test]
fn upstream_and_downstream_ownership_stays_open() {
    use flux_hir::{SourceCrate, SourceDb};
    use flux_intern::Symbol;
    let root = std::env::temp_dir().join(format!("flux-coherence-worlds-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let upstream = "pub trait Bound{}pub trait ParamBound<T>{}pub struct External;pub struct Generic<T>(pub T);pub struct Checkpoint<T>(pub T);pub trait Family{type Out;}impl<T>Family for Generic<T>{type Out=Checkpoint<T>;}";
    let dep = root.join("upstream.rs");
    std::fs::write(&dep, upstream).unwrap();
    let library = root.join("libupstream.rlib");
    let built = std::process::Command::new("rustc")
        .args([
            "--edition=2024",
            "--crate-type=rlib",
            "--crate-name=upstream",
        ])
        .arg(&dep)
        .arg("-o")
        .arg(&library)
        .output()
        .unwrap();
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    for (name, source, accepted) in [
        (
            "generic_foreign_trait_local_self",
            "struct Local<P>(P);impl<P,T:upstream::Bound>From<T>for Local<P>{fn from(_:T)->Self{panic!()}}",
            true,
        ),
        (
            "generic_foreign_trait_specialized_self",
            "struct Local<P>(P);impl upstream::Bound for Local<u32>{}impl<P,T:upstream::Bound>From<T>for Local<P>{fn from(_:T)->Self{panic!()}}",
            false,
        ),
        (
            "generic_foreign_constructor_stays_open",
            "trait Conv<T>{}impl<T>Conv<T>for T{}impl<P,T:upstream::Bound>Conv<T>for upstream::Generic<P>{}",
            false,
        ),
        (
            "generic_closed_argument_local_trait",
            "trait Bound<P>{}trait Conv<S,P>{}impl<S,P>Conv<S,P>for S{}impl<P,S:Bound<Vec<P>>>Conv<S,P>for upstream::External{}",
            true,
        ),
        (
            "generic_external_projection_checkpoint",
            "trait Offset<S=Self>{}impl<T>Offset for upstream::Generic<T>{}impl<T>Offset<<upstream::Generic<T>as upstream::Family>::Out>for upstream::Generic<T>{}",
            false,
        ),
        (
            "generic_external_projection_unknown_overlap",
            "trait Offset<S=Self>{}struct Wrap<T>(T);impl<T>Offset for Wrap<T>{}impl<T:upstream::Family>Offset<T::Out>for Wrap<T>{}",
            false,
        ),
        (
            "foreign_upstream_can_implement",
            "trait Conv<T>{}impl<T>Conv<T>for T{}impl<T:upstream::Bound>Conv<T>for upstream::External{}",
            false,
        ),
        (
            "foreign_trait_local_self",
            "struct Local;trait Conv<T>{}impl<T>Conv<T>for T{}impl<T:upstream::Bound>Conv<T>for Local{}",
            true,
        ),
        (
            "foreign_trait_local_actual_impl",
            "struct Local;impl upstream::Bound for Local{}trait Conv<T>{}impl<T>Conv<T>for T{}impl<T:upstream::Bound>Conv<T>for Local{}",
            false,
        ),
        (
            "foreign_vec_covers_local",
            "struct Local;trait Conv<T>{}impl<T>Conv<T>for T{}impl<T:upstream::Bound>Conv<T>for Vec<Local>{}",
            false,
        ),
        (
            "foreign_box_local",
            "struct Local;trait Conv<T>{}impl<T>Conv<T>for T{}impl<T:upstream::Bound>Conv<T>for Box<Local>{}",
            true,
        ),
        (
            "foreign_reference_local",
            "struct Local;trait Conv<T>{}impl<T>Conv<T>for T{}impl<T:upstream::Bound>Conv<T>for &Local{}",
            true,
        ),
        (
            "foreign_trait_local_argument",
            "struct Local;trait Conv<T>{}impl<T>Conv<T>for T{}impl<T:upstream::ParamBound<Local>>Conv<T>for upstream::External{}",
            true,
        ),
        (
            "foreign_trait_local_holder_argument",
            "struct Holder<T>(T);trait Conv<T>{}impl<T>Conv<T>for T{}impl<T:upstream::ParamBound<Holder<u32>>>Conv<T>for upstream::External{}",
            true,
        ),
        (
            "foreign_trait_covered_local_argument",
            "struct Local;trait Conv<T>{}impl<T>Conv<T>for T{}impl<T:upstream::ParamBound<Vec<Local>>>Conv<T>for upstream::External{}",
            false,
        ),
        (
            "local_trait_downstream_argument",
            "trait Bound<P>{}trait Conv<T,P>{}impl<T,P>Conv<T,P>for T{}impl<P,T:Bound<P>>Conv<T,P>for upstream::External{}",
            false,
        ),
        (
            "local_trait_downstream_covered_argument",
            "trait Bound<P>{}trait Conv<T,P>{}impl<T,P>Conv<T,P>for T{}impl<P,T:Bound<Vec<P>>>Conv<T,P>for upstream::External{}",
            true,
        ),
        (
            "local_trait_downstream_fundamental_argument",
            "trait Bound<P>{}trait Conv<T,P>{}impl<T,P>Conv<T,P>for T{}impl<P,T:Bound<Box<P>>>Conv<T,P>for upstream::External{}",
            false,
        ),
    ] {
        if !case_filter::selected(name) {
            continue;
        }
        let text = format!("{source}fn main(){{}}");
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, &text).unwrap();
        let rustc = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("--extern")
            .arg(format!("upstream={}", library.display()))
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
        let mut db = SourceDb::new();
        let d = db.add("upstream.rs", upstream);
        let u = db.add("user.rs", text.as_str());
        let hir = flux_hir::lower_sources(
            &mut db,
            &[
                SourceCrate {
                    root: d,
                    name: Symbol::intern("upstream"),
                    dependencies: vec![],
                },
                SourceCrate {
                    root: u,
                    name: Symbol::intern("user"),
                    dependencies: vec![(Symbol::intern("upstream"), 0)],
                },
            ],
        );
        let mut errors = hir.diagnostics.clone();
        if errors.is_empty() {
            let (_, d) = flux_ty::signatures_checked(&hir.items, true, None);
            errors.extend(d);
        }
        assert_eq!(errors.is_empty(), accepted, "flux {name}:{errors:?}");
    }
}

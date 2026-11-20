//! Header intersections retain implicit Sized and orphan-closed object bounds.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;

#[test]
fn fundamental_object_intersections_match_rustc() {
    let directory =
        std::env::temp_dir().join(format!("flux-closed-coherence-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    for (name, source, accepted) in [
        (
            "disjoint_inherent_methods",
            "struct Concrete; trait Edit<'a> {} struct Wrapper<T>(T); impl<'a,T:Edit<'a>> Wrapper<T> { fn width(&self)->u16{17} } impl Wrapper<Concrete> { fn width(&self)->u16{29} } fn probe()->u16 { Wrapper(Concrete).width() }",
            true,
        ),
        (
            "overlapping_inherent_methods",
            "struct Concrete; trait Edit<'a> {} impl<'a> Edit<'a> for Concrete {} struct Wrapper<T>(T); impl<'a,T:Edit<'a>> Wrapper<T> { fn width(&self)->u16{17} } impl Wrapper<Concrete> { fn width(&self)->u16{29} }",
            false,
        ),
        (
            "local_object_closed",
            "trait Bound{}trait Obj{}trait Conv<T>{}impl<T>Conv<T>for T{}impl<E:Bound>Conv<E>for Box<dyn Obj>{}",
            true,
        ),
        (
            "generic_object_argument_does_not_confer_locality",
            "trait Bound{}trait Obj<T>{}trait Conv<T,P>{}impl<T,P>Conv<T,P>for T{}impl<P,E:Bound>Conv<E,P>for Box<dyn Obj<P>>{}",
            true,
        ),
        (
            "generic_object_existing_bound_overlaps",
            "trait Bound{}trait Obj<T>{}impl Bound for Box<dyn Obj<u32>>{}trait Conv<T,P>{}impl<T,P>Conv<T,P>for T{}impl<P,E:Bound>Conv<E,P>for Box<dyn Obj<P>>{}",
            false,
        ),
        (
            "foreign_object_local_bound",
            "trait Bound{}trait Conv<T>{}impl<T>Conv<T>for T{}impl<E:Bound>Conv<E>for Box<dyn std::error::Error>{}",
            true,
        ),
        (
            "foreign_bound_can_gain_upstream_impl",
            "trait Conv<T>{}impl<T>Conv<T>for T{}impl<E:std::error::Error>Conv<E>for Box<dyn std::error::Error>{}",
            false,
        ),
        (
            "foreign_bound_for_local_constructor",
            "struct Local<T>(T); trait Conv<T>{} impl<T:std::fmt::Display>Conv<T>for T{} impl<T>Conv<Local<T>>for Local<T>{}",
            true,
        ),
        (
            "foreign_bound_with_local_impl_overlaps",
            "struct Local<T>(T); trait Conv<T>{} impl<T:std::fmt::Display>Conv<T>for T{} impl<T>Conv<Local<T>>for Local<T>{} impl std::fmt::Display for Local<u8>{fn fmt(&self,_:&mut std::fmt::Formatter<'_>)->std::fmt::Result{Ok(())}}",
            false,
        ),
        (
            "unresolved_projection_has_a_disjoint_outer_constructor",
            "trait Storage{type Item;} impl Storage for u32{type Item=char;} struct Local<T>(T); impl<T:Storage+Default> From<Box<T::Item>> for Local<T>{fn from(_:Box<T::Item>)->Self{Self(T::default())}} impl TryFrom<u8> for Local<u32>{type Error=();fn try_from(_:u8)->Result<Self,()>{Ok(Self(0))}}",
            true,
        ),
        (
            "unresolved_projection_can_match_inside_the_same_constructor",
            "trait Storage{type Item;} impl Storage for u32{type Item=char;} struct Local<T>(T); impl<T:Storage+Default> From<Box<T::Item>> for Local<T>{fn from(_:Box<T::Item>)->Self{Self(T::default())}} impl TryFrom<Box<char>> for Local<u32>{type Error=();fn try_from(_:Box<char>)->Result<Self,()>{Ok(Self(0))}}",
            false,
        ),
        (
            "real_from_local_object",
            "trait Obj{}impl<E:std::error::Error>From<E>for Box<dyn Obj>{fn from(_:E)->Self{panic!()}}",
            true,
        ),
        (
            "existing_object_impl_overlaps",
            "trait Bound{}trait Obj{}impl Bound for Box<dyn Obj>{}trait Conv<T>{}impl<T>Conv<T>for T{}impl<E:Bound>Conv<E>for Box<dyn Obj>{}",
            false,
        ),
        (
            "generic_fundamental_pointee_stays_open",
            "trait Bound{}trait Conv<T>{}impl<T>Conv<T>for T{}impl<P:?Sized,E:Bound>Conv<E>for Box<P>{}",
            false,
        ),
        (
            "generic_reference_stays_open",
            "trait Bound{}trait Conv<T>{}impl<T>Conv<T>for T{}impl<'a,P:?Sized,E:Bound>Conv<E>for &'a P{}",
            false,
        ),
        (
            "generic_box_argument_stays_open",
            "trait Bound{}trait Holder{}trait Conv<T>{}impl<T>Conv<T>for T{}impl<P:Holder,E:Bound>Conv<E>for Box<P>{}",
            false,
        ),
        (
            "reference_object_closed",
            "trait Bound{}trait Obj{}trait Conv<T>{}impl<T>Conv<T>for T{}impl<'a,E:Bound>Conv<E>for &'a dyn Obj{}",
            true,
        ),
        (
            "existing_reference_impl_overlaps",
            "trait Bound{}trait Obj{}impl Bound for &dyn Obj{}trait Conv<T>{}impl<T>Conv<T>for T{}impl<'a,E:Bound>Conv<E>for &'a dyn Obj{}",
            false,
        ),
        (
            "implicit_sized_excludes_object",
            "trait Obj{}trait Conv<T:?Sized>{}impl<T>Conv<T>for Box<T>{}impl<E:Obj+?Sized>Conv<E>for Box<dyn Obj>{}",
            true,
        ),
        (
            "relaxed_sized_object_overlaps",
            "trait Obj{}trait Conv<T:?Sized>{}impl<T:?Sized>Conv<T>for Box<T>{}impl<E:Obj+?Sized>Conv<E>for Box<dyn Obj>{}",
            false,
        ),
        (
            "principal_object_bound_is_not_negative",
            "trait Bound{}trait Conv<T:?Sized>{}impl<T:?Sized>Conv<T>for T{}impl<E:Bound+?Sized>Conv<E>for dyn Bound{}",
            false,
        ),
        (
            "builtin_copy_bound_is_not_negative",
            "trait Conv<T>{}impl<T>Conv<T>for T{}impl<E:Copy>Conv<E>for fn(){}",
            false,
        ),
        (
            "generic_principal_bound_overlaps",
            "trait Bound<T>{}trait Conv<S:?Sized,P>{}impl<S:?Sized,P>Conv<S,P>for S{}impl<T,E:Bound<T>+?Sized>Conv<E,T>for dyn Bound<T>{}",
            false,
        ),
        (
            "permuted_principal_bound_overlaps",
            "trait Bound<A,B>{}trait Conv<S:?Sized,P,Q>{}impl<S:?Sized,P,Q>Conv<S,P,Q>for S{}impl<A,B,E:Bound<B,A>+?Sized>Conv<E,A,B>for dyn Bound<B,A>{}",
            false,
        ),
        (
            "principal_argument_may_become_concrete",
            "trait Bound<T>{}trait Conv<S:?Sized,P>{}impl<S:?Sized,P>Conv<S,P>for S{}impl<T,E:Bound<u32>+?Sized>Conv<E,T>for dyn Bound<T>{}",
            false,
        ),
        (
            "bound_argument_may_become_concrete",
            "trait Bound<T>{}trait Conv<S:?Sized,P>{}impl<S:?Sized,P>Conv<S,P>for S{}impl<T,E:Bound<T>+?Sized>Conv<E,T>for dyn Bound<u32>{}",
            false,
        ),
        (
            "generic_supertrait_bound_overlaps",
            "trait Bound<T>{}trait Obj<T>:Bound<T>{}trait Conv<S:?Sized,P>{}impl<S:?Sized,P>Conv<S,P>for S{}impl<T,E:Bound<T>+?Sized>Conv<E,T>for dyn Obj<T>{}",
            false,
        ),
        (
            "nested_principal_argument_overlaps",
            "trait Bound<T>{}trait Conv<S:?Sized,P>{}impl<S:?Sized,P>Conv<S,P>for S{}impl<T,E:Bound<Option<T>>+?Sized>Conv<E,T>for dyn Bound<Option<T>>{}",
            false,
        ),
        (
            "implicit_sized_excludes_slice",
            "trait Conv<T:?Sized>{}impl<T>Conv<T>for Box<T>{}impl<E:?Sized>Conv<E>for Box<[u8]>{}",
            true,
        ),
        (
            "relaxed_sized_slice_overlaps",
            "trait Conv<T:?Sized>{}impl<T:?Sized>Conv<T>for Box<T>{}impl<E:?Sized>Conv<E>for Box<[u8]>{}",
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
            let (_, signatures) = flux_ty::signatures_checked(&hir.items, true, None);
            diagnostics.extend(signatures);
        }
        assert_eq!(
            diagnostics.is_empty(),
            accepted,
            "Flux {name}: {diagnostics:?}"
        );
    }
    std::fs::remove_dir_all(directory).unwrap();
}

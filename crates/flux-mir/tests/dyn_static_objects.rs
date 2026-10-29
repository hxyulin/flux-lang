//! Static principal contracts constrain occurrences, never erased object types.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;

#[test]
fn static_object_well_formedness_matches_rustc() {
    let root = std::env::temp_dir().join(format!("flux-static-objects-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let prefix =
        "trait Tr:'static{fn number(&self)->u32;}impl Tr for u32{fn number(&self)->u32{*self}}";
    for (name, body, accepted) in [
        (
            "late_bound_fn_item_any_accepts",
            "fn read<'a>(x:&'a u32)->&'a u32{x}fn f(){let x=read;let _:&dyn std::any::Any=&x;}",
            true,
        ),
        (
            "ordinary_generic_fn_item_static_impl_capture",
            "trait Custom{}impl<F:'static>Custom for F{}fn g<T>(){}fn f<T>(){let item=g::<T>;let _:&dyn Custom=&item;}",
            false,
        ),
        (
            "ordinary_generic_fn_item_static_impl_positive",
            "trait Custom{}impl<F:'static>Custom for F{}fn g<T>(){}fn f<T:'static>(){let item=g::<T>;let _:&dyn Custom=&item;}",
            true,
        ),
        (
            "ordinary_generic_closure_static_impl_capture",
            "trait Custom{}impl<F:'static>Custom for F{}fn f<T>(value:T){let closure=move||drop(value);let _:&dyn Custom=&closure;}",
            false,
        ),
        (
            "ordinary_generic_closure_static_impl_positive",
            "trait Custom{}impl<F:'static>Custom for F{}fn f<T:'static>(value:T){let closure=move||drop(value);let _:&dyn Custom=&closure;}",
            true,
        ),
        (
            "ordinary_no_static_impl_captures_short",
            "trait Custom{fn value(&self)->u32;}impl<F:Fn()->u32>Custom for F{fn value(&self)->u32{self()}}fn f<'a>(r:&'a u32){let c=||*r;let _:&dyn Custom=&c;}",
            true,
        ),
        (
            "ordinary_fn_item_static_impl_capture",
            "trait Custom{}impl<F:'static>Custom for F{}fn g<T>(){}fn f<'a>(){let item=g::<&'a u32>;let _:&dyn Custom=&item;}",
            false,
        ),
        (
            "ordinary_fn_item_static_impl_positive",
            "trait Custom{}impl<F:'static>Custom for F{}fn g<T>(){}fn f(){let item=g::<u32>;let _:&dyn Custom=&item;}",
            true,
        ),
        (
            "omitted_object_return_elision",
            "fn f(x:&(dyn std::any::Any+Send))->&dyn std::any::Any{x}",
            true,
        ),
        (
            "explicit_static_object_ambiguous_elision",
            "fn f<'a>(x:&'a(dyn Tr+'static))->&u32{loop{}}",
            false,
        ),
        (
            "explicit_other_object_ambiguous_elision",
            "fn f<'a,'b>(x:&'a(dyn Tr+'b))->&u32{loop{}}",
            false,
        ),
        (
            "explicit_static_reference_not_ignored",
            "fn f<'a>(x:(&'a dyn std::any::Any,&'static u32))->&u32{loop{}}",
            false,
        ),
        (
            "omitted_box_object_no_elision_candidate",
            "fn f(x:Box<dyn Tr>)->&u32{loop{}}",
            false,
        ),
        (
            "explicit_object_elision_candidate",
            "fn f<'a>(x:Box<dyn Tr+'a>)->&u32{loop{}}",
            true,
        ),
        (
            "repeated_alias_argument_elision",
            "type A<T>=(T,T);fn f(x:&A<Box<dyn std::any::Any>>)->&u32{loop{}}",
            true,
        ),
        (
            "fnptr_default_object_return_elision",
            "type F=fn(&dyn std::any::Any)->&u32;",
            true,
        ),
        (
            "fnptr_box_default_return_elision",
            "type F=fn(&Box<dyn std::any::Any>)->&u32;",
            true,
        ),
        (
            "fnptr_explicit_object_ambiguous_elision",
            "type F=fn(&(dyn std::any::Any+'static))->&u32;",
            false,
        ),
        (
            "ordinary_chosen_impl_static_obligation",
            "trait Custom{fn value(&self)->u32;}impl<F:Fn()->u32+'static>Custom for F{fn value(&self)->u32{self()}}fn f<'a>(r:&'a u32){let c=||*r;let _:&dyn Custom=&c;}",
            false,
        ),
        (
            "builtin_callable_static_input_rejects",
            "fn fixed(_: &'static u32)->u32{0}fn f(){let p:fn(&'static u32)->u32=fixed;let _:&dyn Fn(&u32)->u32=&p;}",
            false,
        ),
        (
            "default_outer_ref_escape",
            "fn f<'a>(x:&'a dyn std::any::Any)->&'static dyn std::any::Any{x}",
            false,
        ),
        (
            "default_downcast_escape",
            "fn f<'a>(x:&'a dyn std::any::Any)->&'static u32{x.downcast_ref::<u32>().unwrap()}",
            false,
        ),
        (
            "default_field_borrow",
            "struct S<'a>{x:&'a dyn std::any::Any}",
            true,
        ),
        (
            "explicit_field_borrow",
            "struct S<'a>{x:&'a(dyn std::any::Any+'a)}",
            false,
        ),
        (
            "explicit_placeholder_borrow_escape",
            "fn f<'a>(x:&'a(dyn std::any::Any+'_))->&'static dyn std::any::Any{x}",
            false,
        ),
        (
            "default_fnptr_any",
            "fn read(_: &dyn std::any::Any){}fn f(){let _:fn(&dyn std::any::Any)=read;}",
            true,
        ),
        ("box", "fn f<'a>(x:Box<dyn Tr+'a>)->Box<dyn Tr>{x}", true),
        (
            "arc",
            "fn f<'a>(x:std::sync::Arc<dyn Tr+Send+Sync+'a>)->std::sync::Arc<dyn Tr+Send+Sync>{x}",
            true,
        ),
        (
            "any_box",
            "fn f<'a>(x:Box<dyn std::any::Any+'a>)->Box<dyn std::any::Any>{x}",
            true,
        ),
        (
            "any_arc",
            "fn f<'a>(x:std::sync::Arc<dyn std::any::Any+Send+Sync+'a>)->std::sync::Arc<dyn std::any::Any+Send+Sync>{x}",
            true,
        ),
        (
            "invariant_mut_box",
            "fn f<'a,'b>(x:&'b mut Box<dyn Tr+'a>)->&'b mut Box<dyn Tr>{x}",
            true,
        ),
        (
            "invariant_cell",
            "fn f<'a>(x:std::cell::Cell<Box<dyn Tr+'a>>)->std::cell::Cell<Box<dyn Tr>>{x}",
            true,
        ),
        (
            "invariant_owner",
            "struct W<T>(core::marker::PhantomData<fn(T)->T>);fn f<'a>(x:W<Box<dyn Tr+'a>>)->W<Box<dyn Tr>>{x}",
            true,
        ),
        (
            "nested_owner",
            "struct W<T>(T);fn f<'a>(x:W<Box<dyn Tr+'a>>)->W<Box<dyn Tr>>{x}",
            true,
        ),
        (
            "tuple",
            "fn f<'a>(x:(Box<dyn Tr+'a>,u32))->(Box<dyn Tr>,u32){x}",
            true,
        ),
        (
            "array",
            "fn f<'a>(x:[Box<dyn Tr+'a>;2])->[Box<dyn Tr>;2]{x}",
            true,
        ),
        (
            "shared_object",
            "fn f<'a,'b>(x:&'b(dyn Tr+'a))->&'b(dyn Tr+'static){x}",
            true,
        ),
        (
            "mutable_object",
            "fn f<'a,'b>(x:&'b mut(dyn Tr+'a))->&'b mut(dyn Tr+'static){x}",
            true,
        ),
        (
            "raw_const",
            "fn f<'a>(x:*const(dyn Tr+'a))->*const dyn Tr{x}",
            true,
        ),
        (
            "raw_mut",
            "fn f<'a>(x:*mut(dyn Tr+'a))->*mut dyn Tr{x}",
            true,
        ),
        (
            "signature_implies_static",
            "fn f<'a>(x:Box<dyn Tr+'a>,r:&'a u32)->&'static u32{r}",
            true,
        ),
        (
            "result_signature_implies_static",
            "fn f<'a>(r:&'a u32)->(Box<dyn Tr+'a>,&'static u32){(Box::new(7u32),r)}",
            true,
        ),
        (
            "input_proves_type_id",
            "fn f<'a>(x:Box<dyn Tr+'a>)->std::any::TypeId{std::any::TypeId::of::<dyn Tr+'a>()}",
            true,
        ),
        (
            "no_input_type_id_rejects",
            "fn f<'a>()->std::any::TypeId{std::any::TypeId::of::<dyn Tr+'a>()}",
            false,
        ),
        (
            "no_input_any_type_id_rejects",
            "fn f<'a>()->std::any::TypeId{std::any::TypeId::of::<dyn std::any::Any+'a>()}",
            false,
        ),
        (
            "body_type_argument_obligation",
            "fn f<'a>(){fn noop<T:?Sized>(){}noop::<dyn Tr+'a>();}",
            false,
        ),
        (
            "outer_shared_borrow_is_not_static",
            "fn f<'a,'b>(x:&'b(dyn Tr+'a))->&'static(dyn Tr+'static){x}",
            false,
        ),
        (
            "outer_mutable_borrow_is_not_static",
            "fn f<'a,'b>(x:&'b mut Box<dyn Tr+'a>)->&'static mut Box<dyn Tr>{x}",
            false,
        ),
        (
            "unused_body_formation_obligation",
            "fn f<'a>(){let _:Option<Box<dyn Tr+'a>>=None;}",
            false,
        ),
        (
            "body_formation_obligation",
            "fn f<'a>(r:&'a u32)->&'static u32{let _:Option<Box<dyn Tr+'a>>=None;r}",
            false,
        ),
        (
            "struct_must_declare_static",
            "struct S<'a>{x:Box<dyn Tr+'a>}",
            false,
        ),
        (
            "enum_must_declare_static",
            "enum S<'a>{Value(Box<dyn Tr+'a>)}",
            false,
        ),
        (
            "declared_struct_static",
            "struct S<'a:'static>{x:Box<dyn Tr+'a>}",
            true,
        ),
        (
            "transitive_declared_static",
            "struct S<'a:'b,'b:'static>{x:Box<dyn Tr+'a>,r:&'b u32}",
            true,
        ),
        (
            "nested_declared_requirement",
            "struct Inner<'a:'static>{x:Box<dyn Tr+'a>}struct Outer<'a>{x:Inner<'a>}",
            false,
        ),
        (
            "alias_use",
            "type A<'a>=Box<dyn Tr+'a>;fn f<'a>(x:A<'a>)->Box<dyn Tr>{x}",
            true,
        ),
        (
            "transitive_principal",
            "trait Child:Tr{}fn f<'a>(x:Box<dyn Child+'a>)->Box<dyn Child>{x}",
            true,
        ),
        (
            "marker_erasure",
            "fn f<'a>(x:Box<dyn std::any::Any+Send+Sync+'a>)->Box<dyn std::any::Any>{x}",
            true,
        ),
        (
            "principal_upcast",
            "trait Child:Tr{}fn f<'a>(x:Box<dyn Child+'a>)->Box<dyn Tr>{x}",
            true,
        ),
        (
            "no_static_superbound",
            "trait Plain{}fn f<'a>(x:Box<dyn Plain+'a>)->Box<dyn Plain>{x}",
            false,
        ),
        (
            "static_other_parameter_is_not_self",
            "trait Plain<T:'static>{}fn f<'a>(x:Box<dyn Plain<u32>+'a>)->Box<dyn Plain<u32>>{x}",
            false,
        ),
        (
            "escaped_borrowed_payload",
            "struct Borrow<'a>(&'a u32);impl<'a>Tr for Borrow<'a>{fn number(&self)->u32{*self.0}}",
            false,
        ),
        (
            "short_caller_rejects",
            "fn f<'a>(x:Box<dyn Tr+'a>,r:&'a u32)->&'static u32{r}fn use_it(){let r=7;let _=f(Box::new(3u32),&r);}",
            false,
        ),
        (
            "short_result_caller_rejects",
            "fn f<'a>(r:&'a u32)->(Box<dyn Tr+'a>,&'static u32){(Box::new(3u32),r)}fn use_it(){let r=7;let _=f(&r);}",
            false,
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
                let mir = flux_mir::build_file(&hir, &typed);
                errors.extend(mir.diagnostics);
            }
        }
        assert_eq!(errors.is_empty(), accepted, "flux {name}: {errors:?}");
    }
}

/// Expression lifetime arguments retain their existing explicit boundary.
#[test]
fn early_owner_function_item_remains_explicitly_unsupported() {
    let source = "fn g<'a:'a>(){}fn f<'a>(){let x=g::<'a>;let _:&dyn std::any::Any=&x;}fn main(){}";
    let root =
        std::env::temp_dir().join(format!("flux-static-item-boundary-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("case.rs");
    std::fs::write(&path, source).unwrap();
    let rustc = std::process::Command::new("rustc")
        .args(["--edition=2024", "--emit=metadata"])
        .arg(&path)
        .arg("-o")
        .arg(root.join("case.rmeta"))
        .output()
        .unwrap();
    assert!(
        rustc.status.success(),
        "{}",
        String::from_utf8_lossy(&rustc.stderr)
    );
    let (_, _, hir) = flux_hir::lower_text(&path, source);
    assert!(hir.diagnostics.iter().any(|error|
        error.message == "lifetime arguments in expressions are not supported yet"),
        "{:?}", hir.diagnostics);
}

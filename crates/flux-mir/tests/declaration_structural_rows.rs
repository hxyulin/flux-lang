//! Declaration proofs retain each stored lifetime and exact object contract.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;
#[test]
fn structural_declarations_match_rustc() {
    let root = std::env::temp_dir().join(format!(
        "flux-structural-declarations-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "object_generic_argument_rows",
            "trait Get<T>{}trait Raw<T>:Get<T>{}impl<T>Raw<T>for dyn Get<T>{}fn need<T:Raw<U>+?Sized,U>(_:&T){}fn f<'a>(x:&(dyn Get<&'a u32>+'static)){need::<dyn Get<&'a u32>,&'a u32>(x);}",
            true,
        ),
        (
            "any_object_argument_static_bound",
            "trait Foo<T>:std::any::Any{}trait Raw<T>:Foo<T>{}impl<'a:'static>Raw<&'a str>for dyn Foo<&'a str>{}",
            true,
        ),
        (
            "static_fields",
            "struct Job{label:&'static str,run:fn(&mut u32)->bool}trait Pass:Sync{}impl Pass for Job{}",
            true,
        ),
        (
            "borrowed_shared_send",
            "struct W<'a,T>(&'a T);trait Pass:Send{}impl<'a,T:Sync>Pass for W<'a,T>{}",
            true,
        ),
        (
            "borrowed_shared_wrong_bound",
            "struct W<'a,T>(&'a T);trait Pass:Send{}impl<'a,T:Send>Pass for W<'a,T>{}",
            false,
        ),
        (
            "borrowed_mut_send",
            "struct W<'a,T>(&'a mut T);trait Pass:Send{}impl<'a,T:Send>Pass for W<'a,T>{}",
            true,
        ),
        (
            "borrowed_mut_sync",
            "struct W<'a,T>(&'a mut T);trait Pass:Sync{}impl<'a,T:Sync>Pass for W<'a,T>{}",
            true,
        ),
        (
            "two_borrowed_fields",
            "struct W<'a,'b>(&'a u32,&'b u32);trait Pass:Send+Sync{}impl<'a,'b>Pass for W<'a,'b>{}",
            true,
        ),
        (
            "nested_borrowed_fields",
            "struct Inner<'a,T>(&'a T);struct W<'a,T>((Inner<'a,T>,[&'a T;2]));trait Pass:Send+Sync{}impl<'a,T:Sync>Pass for W<'a,T>{}",
            true,
        ),
        (
            "static_reference_requirement",
            "struct W<'a>(&'a u32);trait Pass:Sync+'static{}impl<'a>Pass for W<'a>{}",
            false,
        ),
        (
            "partial_static_requirement",
            "struct W<'a,'b>(&'a u32,&'b u32);trait Pass:Sync+'static{}impl<'a:'static,'b>Pass for W<'a,'b>{}",
            false,
        ),
        (
            "raw_pointer_leaf",
            "struct W<'a,T>(&'a T,*const T);trait Pass:Sync{}impl<'a,T:Sync>Pass for W<'a,T>{}",
            false,
        ),
        (
            "unsafe_cell_leaf",
            "struct W<'a,T>(&'a T,core::cell::UnsafeCell<T>);trait Pass:Sync{}impl<'a,T:Sync>Pass for W<'a,T>{}",
            false,
        ),
        (
            "rc_leaf",
            "struct W<'a,T>(&'a T,std::rc::Rc<T>);trait Pass:Send{}impl<'a,T:Sync+Send>Pass for W<'a,T>{}",
            false,
        ),
        (
            "object_principal",
            "trait W{fn write(&mut self);}trait Term{}impl Term for dyn W{}trait Raw:W+Term{}impl Raw for dyn W{}",
            true,
        ),
        (
            "object_explicit_lifetime",
            "trait W{fn write(&mut self);}trait Term{}impl<'a>Term for dyn W+'a{}trait Raw:W+Term{}impl<'a>Raw for dyn W+'a{}",
            true,
        ),
        (
            "object_static_impl_condition",
            "trait W{fn write(&mut self);}trait Term{}impl Term for dyn W{}trait Raw:W+Term{}impl<'a>Raw for dyn W+'a{}",
            false,
        ),
        (
            "object_unrelated_trait",
            "trait W{}trait Other{}trait Raw:W+Other{}impl Raw for dyn W{}",
            false,
        ),
        (
            "object_super_marker",
            "trait W:Send{}trait Raw:W{}impl Raw for dyn W{}",
            true,
        ),
        (
            "object_scalar_argument",
            "trait W<T>{}trait Raw:W<u8>{}impl Raw for dyn W<u8>{}",
            true,
        ),
        (
            "object_wrong_argument",
            "trait W<T>{}trait Raw:W<u8>{}impl Raw for dyn W<u16>{}",
            false,
        ),
        (
            "any_object_argument_formation",
            "trait Foo<T>:std::any::Any{}fn f<'a>(_:&'a str){let _:Option<Box<dyn Foo<&'a str>>>=None;}",
            true,
        ),
        (
            "any_object_argument_principal_requirement",
            "trait Foo<T>:std::any::Any{}fn need<'a,T:Foo<&'a str>+?Sized>(){}fn f<'a>(){need::<dyn Foo<&'a str>>();}",
            false,
        ),
        (
            "any_object_argument_membership",
            "trait Foo<T>:std::any::Any{}fn need<T:std::any::Any+?Sized>(){}fn f<'a>(){need::<dyn Foo<&'a str>>();}",
            false,
        ),
        (
            "any_object_argument_type_id",
            "trait Foo<T>:std::any::Any{}fn f<'a>(){let _=std::any::TypeId::of::<dyn Foo<&'a str>>();}",
            false,
        ),
        (
            "any_object_argument_impl",
            "trait Foo<T>:std::any::Any{}trait Raw<T>:Foo<T>{}impl<'a>Raw<&'a str>for dyn Foo<&'a str>{}",
            false,
        ),
        (
            "any_object_argument_static",
            "trait Foo<T>:std::any::Any{}trait Raw<T>:Foo<T>{}impl Raw<&'static str>for dyn Foo<&'static str>{}",
            true,
        ),
        (
            "recursive_auto_declaration",
            "struct Node<'a>{value:&'a u32,next:Option<Box<Node<'a>>>}trait Pass:Send+Sync{}impl<'a>Pass for Node<'a>{}",
            true,
        ),
        (
            "recursive_auto_cell_negative",
            "struct Node<'a>{value:&'a u32,next:Option<Box<Node<'a>>>,bad:core::cell::UnsafeCell<u32>}trait Pass:Sync{}impl<'a>Pass for Node<'a>{}",
            false,
        ),
        (
            "recursive_auto_rc_negative",
            "struct Node<'a>{value:&'a u32,next:Option<Box<Node<'a>>>,bad:std::rc::Rc<u32>}trait Pass:Send{}impl<'a>Pass for Node<'a>{}",
            false,
        ),
        (
            "recursive_auto_static_negative",
            "struct Node<'a>{value:&'a u32,next:Option<Box<Node<'a>>>}trait Pass:Send+Sync+'static{}impl<'a>Pass for Node<'a>{}",
            false,
        ),
        (
            "recursive_mixed_inductive_negative",
            "struct Node<'a>{value:&'a u32,next:Option<Box<Node<'a>>>}trait Gate{}impl<'a>Gate for Node<'a>where Node<'a>:Send{}unsafe impl<'a>Send for Node<'a>where Node<'a>:Gate{}trait Pass:Send{}impl<'a>Pass for Node<'a>{}",
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
            errors.extend(typed.diagnostics.clone());
            if errors.is_empty() {
                errors.extend(flux_mir::build_file(&hir, &typed).diagnostics);
            }
        }
        assert_eq!(errors.is_empty(), accepted, "flux {name}:{errors:?}");
    }
}

//! Runtime identity and checked trait-object downcasts.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;

#[test]
fn any_type_id_matches_rustc_off_and_on() {
    support::oracle::compare_fixture("any_type_id");
}

#[test]
fn any_marker_and_lifetime_contracts_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-any-contracts-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "duplicate_principal",
            "trait Tr{}type X=dyn Tr+Tr;fn main(){}",
            false,
        ),
        (
            "different_principal_args",
            "trait Tr<T>{}type X=dyn Tr<u8>+Tr<u16>;fn main(){}",
            false,
        ),
        (
            "static_type_id",
            "fn f<'a:'static>()->std::any::TypeId{std::any::TypeId::of::<&'a u32>()}fn main(){}",
            true,
        ),
        (
            "borrowed_type_id",
            "fn f<'a>()->std::any::TypeId{std::any::TypeId::of::<&'a u32>()}fn main(){}",
            false,
        ),
        (
            "short_any_value",
            "fn main(){let x=1;let r=&x;let _: &dyn std::any::Any=&r;}",
            false,
        ),
        (
            "short_any_borrow",
            "fn f<'a>(x:&'a u32)->&'a dyn std::any::Any{x}fn main(){}",
            true,
        ),
        (
            "closure_any_capture",
            "fn main(){let x=1;let c=||x;let _:&dyn std::any::Any=&c;}",
            false,
        ),
        (
            "move_closure_any_capture",
            "fn main(){let x=1;let r=&x;let c=move||*r;let _:&dyn std::any::Any=&c;}",
            false,
        ),
        (
            "function_item_any_capture",
            "fn g<T>(){}fn main(){let x=1;fn use_it<'a>(_: &'a u32){let f=g::<&'a u32>;let _:&dyn std::any::Any=&f;}use_it(&x);}",
            false,
        ),
        (
            "downcast_escape",
            "fn f(x:&dyn std::any::Any)->&'static u32{x.downcast_ref::<u32>().unwrap()}fn main(){}",
            false,
        ),
        (
            "downcast_mut_alias",
            "fn main(){let mut x=1u32;let a:&mut dyn std::any::Any=&mut x;let p=a.downcast_mut::<u32>().unwrap();let q=a.downcast_mut::<u32>().unwrap();*p+=*q;}",
            false,
        ),
        (
            "send_sync_object",
            "fn yes<T:?Sized+Send+Sync>(){}fn main(){yes::<dyn std::any::Any+Send+Sync>();}",
            true,
        ),
        (
            "unmarked_not_send",
            "fn yes<T:?Sized+Send>(){}fn main(){yes::<dyn std::any::Any>();}",
            false,
        ),
        (
            "send_not_sync",
            "fn yes<T:?Sized+Sync>(){}fn main(){yes::<dyn std::any::Any+Send>();}",
            false,
        ),
        (
            "reject_rc_send",
            "fn main(){let v=std::rc::Rc::new(1);let _:&(dyn std::any::Any+Send)=&v;}",
            false,
        ),
        (
            "reject_cell_sync",
            "fn main(){let v=std::cell::Cell::new(1);let _:&(dyn std::any::Any+Sync)=&v;}",
            false,
        ),
        (
            "marker_erasure",
            "fn f(x:&(dyn std::any::Any+Send+Sync))->&dyn std::any::Any{x}fn main(){}",
            true,
        ),
        (
            "marker_addition",
            "fn f(x:&dyn std::any::Any)->&(dyn std::any::Any+Send){x}fn main(){}",
            false,
        ),
        (
            "raw_marker_addition",
            "fn f(x:*const dyn std::any::Any)->*const(dyn std::any::Any+Send){x as *const(dyn std::any::Any+Send)}fn main(){}",
            false,
        ),
        (
            "object_raw_lifetime",
            "trait A{}fn f<'a>(x:&'a(dyn A+'a))->*const(dyn A+'a){x}fn main(){}",
            true,
        ),
        (
            "object_raw_static_escape",
            "trait A{}fn f<'a>(x:&'a(dyn A+'a))->*const dyn A{x}fn main(){}",
            false,
        ),
        (
            "object_raw_cast_static_escape",
            "trait A{}fn f<'a>(x:&'a(dyn A+'a))->*const dyn A{x as *const dyn A}fn main(){}",
            false,
        ),
        (
            "arc_marker_addition",
            "fn f(x:std::sync::Arc<dyn std::any::Any>)->std::sync::Arc<dyn std::any::Any+Send+Sync>{x}fn main(){}",
            false,
        ),
        (
            "arc_downcast_send",
            "fn f(x:std::sync::Arc<dyn std::any::Any+Send+Sync>){let _=x.downcast::<std::rc::Rc<u32>>();}fn main(){}",
            false,
        ),
        (
            "private_type_id",
            "fn main(){let _=std::any::TypeId{value:1};}",
            false,
        ),
        (
            "type_id_not_structural",
            "const ID:std::any::TypeId=std::any::TypeId::of::<u32>();fn f(x:std::any::TypeId){match x{ID=>{},_=>{}}}fn main(){}",
            false,
        ),
        (
            "arc_unwind_mut_ref",
            "fn yes<T:std::panic::UnwindSafe>(){}fn main(){yes::<std::sync::Arc<&mut u8>>();yes::<std::sync::Weak<&mut u8>>();}",
            true,
        ),
        (
            "duplicate_marker_inherent",
            "trait A{}impl dyn A+Send{fn f(&self){}}impl dyn A+Send{fn f(&self){}}fn main(){}",
            false,
        ),
        (
            "distinct_marker_inherent",
            "trait A{}impl dyn A{fn f(&self){}}impl dyn A+Send{fn f(&self){}}fn main(){}",
            true,
        ),
        (
            "object_marker_order",
            "fn f(x:&(dyn Send+std::any::Any+Sync))->&(dyn std::any::Any+Sync+Send){x}fn main(){}",
            true,
        ),
        (
            "unrelated_extra_trait",
            "trait A{}trait B{}fn f(_: &(dyn A+B)){}fn main(){}",
            false,
        ),
    ] {
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let reference = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("oracle.rmeta"))
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

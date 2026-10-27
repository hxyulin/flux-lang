//! Slice view lifetimes and checked constant slicing match Rust.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;

#[test]
fn slice_view_contracts_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-slice-contracts-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "iter_mut_send_sync",
            "fn need<T:Send+Sync>(_:T){}fn main(){let mut a=[1u32,2];need(a.iter_mut());}",
            true,
        ),
        (
            "iter_mut_cell_not_sync",
            "fn need<T:Sync>(_:T){}fn main(){let mut a=[std::cell::Cell::new(1)];need(a.iter_mut());}",
            false,
        ),
        (
            "iter_mut_rc_not_send",
            "fn need<T:Send>(_:T){}fn main(){let mut a=[std::rc::Rc::new(1)];need(a.iter_mut());}",
            false,
        ),
        (
            "remaining_shared_lifetime",
            "fn f<'a>(mut i:std::slice::IterMut<'a,u32>)->&'a[u32]{i.next();i.into_slice()}fn main(){}",
            true,
        ),
        (
            "shared_view_borrows_iterator",
            "fn f<'a>(i:std::slice::IterMut<'a,u32>)->&'a[u32]{i.as_slice()}fn main(){}",
            false,
        ),
        (
            "no_next_with_shared_view",
            "fn main(){let mut a=[1,2];let mut i=a.iter_mut();let s=i.as_slice();i.next();println!(\"{}\",s[0]);}",
            false,
        ),
        (
            "yielded_element_disjoint",
            "fn main(){let mut a=[1,2];let mut i=a.iter_mut();let x=i.next().unwrap();let s=i.into_slice();*x=3;s[0]=4;}",
            true,
        ),
        (
            "remaining_escape",
            "fn f()->&'static mut[u32]{let mut a=[1,2];a.iter_mut().into_slice()}fn main(){}",
            false,
        ),
        (
            "const_checked_success",
            "const P:(&[u8],&[u8])=[1,2,3].split_at(1);const Q:Option<(&[u8],&[u8])>=[1,2].split_at_checked(3);fn main(){}",
            true,
        ),
        (
            "const_out_of_bounds",
            "const P:(&[u8],&[u8])=[1,2].split_at(3);fn main(){}",
            false,
        ),
        (
            "const_local_escape",
            "const fn f()->&'static[u8]{let a=[1,2];a.split_at(1).1}const X:&[u8]=f();fn main(){}",
            false,
        ),
        (
            "retained_mut_borrow",
            "fn main(){let mut a=vec![1,2];let old=&a[0];a.retain_mut(|n|{*n+=1;true});println!(\"{}\",old);}",
            false,
        ),
        (
            "intoiter_requires_clone",
            "struct T;fn main(){let i=vec![T].into_iter();let _=i.clone();}",
            false,
        ),
        (
            "tuple_bound_slice",
            "fn main(){use std::ops::Bound::*;let mut a=[1,2,3];a[(Included(1),Unbounded)][0]=4;assert_eq!(a,[1,4,3]);}",
            true,
        ),
        (
            "split_receiver_escape",
            "fn f()->&'static[u8]{let a=[1,2];a.split(|n|*n==2).next().unwrap()}fn main(){}",
            false,
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
        let mut errors: Vec<_> = hir.diagnostics.iter().map(|d| d.message.clone()).collect();
        if errors.is_empty() {
            let typed = flux_mir::check_file(&hir);
            errors.extend(typed.diagnostics.iter().map(|d| d.message.clone()));
            if errors.is_empty() {
                errors.extend(
                    flux_mir::build_file(&hir, &typed)
                        .diagnostics
                        .into_iter()
                        .map(|d| d.message),
                );
            }
        }
        assert_eq!(errors.is_empty(), accepted, "flux {name}: {errors:?}");
    }
    std::fs::remove_dir_all(root).unwrap();
}

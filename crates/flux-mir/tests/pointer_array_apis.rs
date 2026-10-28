//! Constant raw slices retain typed allocation bounds and frame provenance.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod cases;
#[test]
fn pointer_array_contracts_match_rustc() {
    let root = std::env::temp_dir().join(format!(
        "flux-pointer-array-contracts-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "array_index_bound",
            r###"fn need<A:core::ops::Index<usize,Output=u8>>(a:&A)->u8{a[0]}fn main(){assert_eq!(need(&[7u8,8]),7);}"###,
            true,
        ),
        (
            "array_index_mut_bound",
            r###"fn need<A:core::ops::IndexMut<usize,Output=u8>>(a:&mut A){a[0]=9;}fn main(){let mut a=[7u8,8];need(&mut a);assert_eq!(a[0],9);}"###,
            true,
        ),
        (
            "array_range_bound",
            r###"fn need<A:core::ops::Index<core::ops::Range<usize>,Output=[u8]>>(a:&A)->&[u8]{&a[0..1]}fn main(){assert_eq!(need(&[7u8,8]),&[7]);}"###,
            true,
        ),
        (
            "array_wrong_output",
            r###"fn need<A:core::ops::Index<usize,Output=u16>>(_:A){}fn main(){need([7u8,8]);}"###,
            false,
        ),
        (
            "borrowed_noncopy",
            r###"fn f(x:&[String])->&[String;2]{x.try_into().unwrap()}fn main(){let a=[String::new(),String::new()];let _=f(&a);}"###,
            true,
        ),
        (
            "mutable_noncopy",
            r###"fn f(x:&mut[String])->&mut[String;2]{x.try_into().unwrap()}fn main(){let mut a=[String::new(),String::new()];let _=f(&mut a);}"###,
            true,
        ),
        (
            "borrowed_escape",
            r###"fn f(x:&[u8])->&'static[u8;2]{x.try_into().unwrap()}fn main(){}"###,
            false,
        ),
        (
            "mutable_escape",
            r###"fn f(x:&mut[u8])->&'static mut[u8;2]{x.try_into().unwrap()}fn main(){}"###,
            false,
        ),
        (
            "mutable_conflict",
            r###"fn main(){let mut a=[1u8,2];let x:&mut[u8;2]=a.as_mut_slice().try_into().unwrap();let y=&a;println!("{} {}",x[0],y[0]);}"###,
            false,
        ),
        (
            "slice_error_contract",
            r###"fn need<E:core::error::Error>(){}fn main(){need::<core::array::TryFromSliceError>();}"###,
            true,
        ),
        (
            "static_shared",
            r###"static A:[u16;3]=[2,3,5];const S:&[u16]=unsafe{core::slice::from_raw_parts(A.as_ptr(),2)};fn main(){assert_eq!(S,&[2,3]);}"###,
            true,
        ),
        (
            "static_raw",
            r###"static A:[u16;3]=[2,3,5];const S:*const[u16]=core::ptr::slice_from_raw_parts(A.as_ptr(),2);fn main(){assert_eq!(unsafe{&*S},&[2,3]);}"###,
            true,
        ),
        (
            "promoted_shared",
            r###"const S:&[u8]=unsafe{core::slice::from_raw_parts([2u8,3,5].as_ptr(),2)};fn main(){assert_eq!(S,&[2,3]);}"###,
            true,
        ),
        (
            "const_fn_shared",
            r###"const fn prefix(a:&[u8])->&[u8]{unsafe{core::slice::from_raw_parts(a.as_ptr(),2)}}const S:&[u8]=prefix(&[3,4,5]);fn main(){let _=S;}"###,
            true,
        ),
        (
            "empty_shared",
            r###"static A:[u32;0]=[];const S:&[u32]=unsafe{core::slice::from_raw_parts(A.as_ptr(),0)};fn main(){let _=S;}"###,
            true,
        ),
        (
            "initialized_value",
            r###"const V:u32=unsafe{core::mem::MaybeUninit::new(9u32).assume_init()};fn main(){assert_eq!(V,9);}"###,
            true,
        ),
        (
            "initialized_array",
            r###"const V:[u8;3]=unsafe{core::mem::MaybeUninit::new([3,5,8]).assume_init()};fn main(){assert_eq!(V[2],8);}"###,
            true,
        ),
        (
            "out_of_bounds",
            r###"static A:[u8;2]=[1,2];const S:&[u8]=unsafe{core::slice::from_raw_parts(A.as_ptr(),3)};fn main(){let _=S;}"###,
            false,
        ),
        (
            "null_even_empty",
            r###"const S:&[u8]=unsafe{core::slice::from_raw_parts(core::ptr::null(),0)};fn main(){let _=S;}"###,
            false,
        ),
        (
            "discard_null",
            r###"const V:()={let _=unsafe{core::slice::from_raw_parts::<u8>(core::ptr::null(),0)};};fn main(){let _=V;}"###,
            false,
        ),
        (
            "uninitialized_value",
            r###"const V:u32=unsafe{core::mem::MaybeUninit::uninit().assume_init()};fn main(){let _=V;}"###,
            false,
        ),
        (
            "local_escape",
            r###"const S:&[u8]={let a=[1u8,2];unsafe{core::slice::from_raw_parts(a.as_ptr(),2)}};fn main(){let _=S;}"###,
            false,
        ),
        (
            "dynamic_temp_escape",
            r###"const fn f(n:u8)->&'static[u8]{unsafe{core::slice::from_raw_parts([n,2].as_ptr(),2)}}const S:&[u8]=f(7);fn main(){let _=S;}"###,
            false,
        ),
        (
            "alias_mutable_shared",
            r###"fn main(){let mut a=[1u8,2,3];let(x,y)=a.as_chunks_mut::<2>();let z=&a;println!("{} {} {}",x[0][0],y[0],z[0]);}"###,
            false,
        ),
        (
            "array_borrow",
            r###"use core::borrow::{Borrow,BorrowMut};fn take<T:Borrow<[u8]>>(x:&T)->usize{x.borrow().len()}fn main(){let mut a=[1u8,2];let s:&mut[u8]=a.borrow_mut();s[0]=3;assert_eq!(take(&a),2);}"###,
            true,
        ),
    ] {
        if !cases::selected(name) {
            continue;
        }
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let rustc = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join(format!("{name}.rmeta")))
            .output()
            .unwrap();
        assert_eq!(
            rustc.status.success(),
            accepted,
            "{name}: {}",
            String::from_utf8_lossy(&rustc.stderr)
        );
        let (db, _, hir) = flux_hir::lower_text(&path, source);
        let mut diagnostics = db.syntax_errors();
        diagnostics.extend(hir.diagnostics.clone());
        if diagnostics.is_empty() {
            let typed = flux_mir::check_file(&hir);
            diagnostics.extend(typed.diagnostics.clone());
            if diagnostics.is_empty() {
                diagnostics.extend(flux_mir::build_file(&hir, &typed).diagnostics);
            }
        }
        assert_eq!(diagnostics.is_empty(), accepted, "{name}: {diagnostics:?}");
    }
}

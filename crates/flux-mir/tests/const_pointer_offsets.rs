//! Constant pointer offsets retain allocation bounds, identity and validity.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;
use std::process::Command;

#[test]
fn const_pointer_offsets_match_rustc() {
    let dir = std::env::temp_dir().join(format!("flux-const-offsets-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, accepted, text) in [
        (
            "array_add",
            true,
            r#"const A:[u32;4]=[7,11,13,17];const X:u32=unsafe{*A.as_ptr().add(2)};fn main(){assert_eq!(X,13);}"#,
        ),
        (
            "static_identity",
            true,
            r#"static A:[u32;4]=[7,11,13,17];const P:*const u32=unsafe{A.as_ptr().add(2)};fn main(){assert_eq!(P,unsafe{A.as_ptr().add(2)});}"#,
        ),
        (
            "one_past",
            true,
            r#"static A:[u32;4]=[7,11,13,17];const P:*const u32=unsafe{A.as_ptr().add(4)};fn main(){}"#,
        ),
        (
            "one_past_back",
            true,
            r#"const A:[u32;4]=[7,11,13,17];const X:u32=unsafe{*A.as_ptr().add(4).sub(3)};fn main(){assert_eq!(X,11);}"#,
        ),
        (
            "signed_back",
            true,
            r#"const A:[u32;4]=[7,11,13,17];const X:u32=unsafe{*A.as_ptr().add(3).offset(-2)};fn main(){assert_eq!(X,11);}"#,
        ),
        (
            "raw_mut",
            true,
            r#"static A:[u32;2]=[7,11];const X:u32=unsafe{*(A.as_ptr() as *mut u32).add(1)};fn main(){assert_eq!(X,11);}"#,
        ),
        (
            "slice_ref",
            true,
            r#"static A:[u32;3]=[7,11,13];const S:&[u32]=unsafe{std::slice::from_raw_parts(A.as_ptr().add(1),2)};fn main(){assert_eq!(S,&[11,13]);}"#,
        ),
        (
            "empty_at_end",
            true,
            r#"static A:[u32;3]=[7,11,13];const S:&[u32]=unsafe{std::slice::from_raw_parts(A.as_ptr().add(3),0)};fn main(){assert_eq!(S.len(),0);}"#,
        ),
        (
            "null_zero",
            true,
            r#"const P:*const u32=unsafe{std::ptr::null::<u32>().add(0)};fn main(){}"#,
        ),
        (
            "zst_large",
            true,
            r#"const P:*const ()=unsafe{(1usize as *const ()).add(usize::MAX)};fn main(){}"#,
        ),
        (
            "out_of_bounds",
            false,
            r#"const A:[u32;2]=[7,11];const P:*const u32=unsafe{A.as_ptr().add(3)};fn main(){}"#,
        ),
        (
            "before_start",
            false,
            r#"const A:[u32;2]=[7,11];const P:*const u32=unsafe{A.as_ptr().sub(1)};fn main(){}"#,
        ),
        (
            "unsigned_overflow",
            false,
            r#"const A:[u8;2]=[7,11];const P:*const u8=unsafe{A.as_ptr().add(1).add(usize::MAX)};fn main(){}"#,
        ),
        (
            "multiplication_overflow",
            false,
            r#"const A:[u64;2]=[7,11];const P:*const u64=unsafe{A.as_ptr().add(usize::MAX/2)};fn main(){}"#,
        ),
        (
            "null_nonzero",
            false,
            r#"const P:*const u32=unsafe{std::ptr::null::<u32>().add(1)};fn main(){}"#,
        ),
        (
            "unbacked",
            false,
            r#"const P:*const u32=unsafe{(8usize as *const u32).add(1)};fn main(){}"#,
        ),
        (
            "end_read",
            false,
            r#"const A:[u32;2]=[7,11];const X:u32=unsafe{*A.as_ptr().add(2)};fn main(){}"#,
        ),
        (
            "end_reference",
            false,
            r#"static A:[u32;2]=[7,11];const X:&u32=unsafe{&*A.as_ptr().add(2)};fn main(){}"#,
        ),
        (
            "unaligned",
            false,
            r#"const A:[u32;2]=[7,11];const X:u32=unsafe{*((A.as_ptr() as *const u8).add(1) as *const u32)};fn main(){}"#,
        ),
        (
            "integer_exposure",
            false,
            r#"static A:[u32;2]=[7,11];const X:usize=unsafe{A.as_ptr().add(1) as usize};fn main(){}"#,
        ),
        (
            "local_escape",
            false,
            r#"const fn bad()->*const u32{let a=[7u32,11];unsafe{a.as_ptr().add(1)}}const P:*const u32=bad();fn main(){}"#,
        ),
        (
            "promoted_subslice_rewind",
            true,
            r#"const A:[u32;3]=[7,11,13];const S:&[u32]=match &A{[_,rest@..]=>rest};const X:u32=unsafe{*S.as_ptr().sub(1)};fn main(){assert_eq!(X,7);}"#,
        ),
        (
            "static_subslice_rewind",
            true,
            r#"static A:[u32;3]=[7,11,13];const S:&[u32]=match &A{[_,rest@..]=>rest};const X:u32=unsafe{*S.as_ptr().sub(1)};fn main(){assert_eq!(X,7);}"#,
        ),
        (
            "field_allocation",
            true,
            r#"#[repr(C)]struct S{a:[u32;2],b:[u32;2]}static A:S=S{a:[7,11],b:[13,17]};const X:u32=unsafe{*A.b.as_ptr().sub(1)};fn main(){assert_eq!(X,11);}"#,
        ),
        (
            "offset_ref",
            true,
            r#"static A:[u32;3]=[7,11,13];const R:&u32=unsafe{&*A.as_ptr().add(1)};fn main(){assert_eq!(*R,11);}"#,
        ),
        (
            "invalid_bool",
            false,
            r#"static A:[u8;3]=[0,2,1];const R:&bool=unsafe{&*(A.as_ptr().add(1) as *const bool)};fn main(){}"#,
        ),
        (
            "transient_end_ref",
            false,
            r#"static A:[u32;2]=[7,11];const X:u32=unsafe{let r=&*A.as_ptr().add(2);*r};fn main(){}"#,
        ),
        (
            "slice_end_overflow",
            false,
            r#"static A:[u32;3]=[7,11,13];const R:&[u32]=unsafe{std::slice::from_raw_parts(A.as_ptr().add(1),3)};fn main(){}"#,
        ),
        (
            "array_unsize_offset",
            true,
            r#"static A:[[u32;2];2]=[[7,11],[13,17]];const P:*const [u32]=unsafe{A.as_ptr().add(1) as *const[u32]};const X:u32=unsafe{(&*P)[1]};fn main(){assert_eq!(X,17);}"#,
        ),
        (
            "static_mut_address",
            true,
            r#"static mut A:[u32;2]=[7,11];const P:*mut u32=unsafe{(&raw mut A).cast::<u32>().add(1)};fn main(){}"#,
        ),
        (
            "static_mut_read",
            false,
            r#"static mut A:[u32;2]=[7,11];const X:u32=unsafe{*(&raw const A).cast::<u32>().add(1)};fn main(){}"#,
        ),
        (
            "cell_metadata",
            true,
            r#"use std::sync::atomic::AtomicU32;static A:[AtomicU32;2]=[AtomicU32::new(1),AtomicU32::new(2)];const P:*const[AtomicU32]=unsafe{std::ptr::slice_from_raw_parts(A.as_ptr().add(1),1)};const N:usize=P.len();fn main(){assert_eq!(N,1);}"#,
        ),
        (
            "raw_reinterpreted_end",
            true,
            r#"static A:[u32;2]=[7,11];const P:*const u64=unsafe{&raw const*(A.as_ptr().add(2)as*const u64)};fn main(){}"#,
        ),
        (
            "nonnull_add",
            true,
            r#"static A:[u32;2]=[7,11];const P:std::ptr::NonNull<u32>=unsafe{std::ptr::NonNull::new_unchecked(A.as_ptr()as*mut u32).add(1)};const X:u32=unsafe{*P.as_ptr()};fn main(){assert_eq!(X,11);}"#,
        ),
        (
            "nonnull_unbacked",
            false,
            r#"const P:std::ptr::NonNull<u32>=unsafe{std::ptr::NonNull::<u32>::dangling().add(1)};fn main(){}"#,
        ),
        (
            "end_field_zero",
            true,
            r#"#[repr(C)]struct S{x:u32,y:u32}static A:[S;1]=[S{x:7,y:11}];const P:*const u32=unsafe{&raw const (*A.as_ptr().add(1)).x};fn main(){}"#,
        ),
        (
            "end_field_nonzero",
            false,
            r#"#[repr(C)]struct S{x:u32,y:u32}static A:[S;1]=[S{x:7,y:11}];const P:*const u32=unsafe{&raw const (*A.as_ptr().add(1)).y};fn main(){}"#,
        ),
    ] {
        if !case_filter::selected(name) {
            continue;
        }
        let path = dir.join(format!("{name}.rs"));
        std::fs::write(&path, text).unwrap();
        let rustc = Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata", "--out-dir"])
            .arg(&dir)
            .arg(&path)
            .output()
            .unwrap();
        assert_eq!(
            rustc.status.success(),
            accepted,
            "rustc {name}: {}",
            String::from_utf8_lossy(&rustc.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text(&path, text);
        let mut errors = hir.diagnostics.clone();
        if errors.is_empty() {
            let typed = flux_mir::check_file(&hir);
            errors.extend(typed.diagnostics.clone());
            if errors.is_empty() {
                errors.extend(flux_mir::build_file(&hir, &typed).diagnostics);
            }
        }
        assert_eq!(errors.is_empty(), accepted, "flux {name}: {errors:?}");
    }
    std::fs::remove_dir_all(dir).unwrap();
}

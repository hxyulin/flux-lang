//! Escaping interior-mutable temporary borrows reject without losing static identity.
use std::process::Command;

#[test]
fn const_interior_borrows_match_rustc() {
    let dir = std::env::temp_dir().join(format!("flux-const-interior-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, accepted, text) in [
        (
            "array",
            false,
            r#"use std::cell::Cell; const C:&[Cell<u8>;1]=&[Cell::new(1)]; fn main(){let _=C;}"#,
        ),
        (
            "branch",
            false,
            r#"use std::cell::Cell; const C:&Cell<u8>=if false { &Cell::new(1) } else { &Cell::new(2) }; fn main(){let _=C;}"#,
        ),
        (
            "cell_ref_arg",
            false,
            r#"use std::cell::Cell; const fn same(c:&Cell<u8>)->&Cell<u8>{c} const C:&Cell<u8>=same(&Cell::new(1)); fn main(){let _=C;}"#,
        ),
        (
            "cell_value_arg",
            true,
            r#"use std::cell::Cell; const fn same(c:&Cell<u8>)->u8{let _=c;7} const C:u8=same(&Cell::new(1)); fn main(){let _=C;}"#,
        ),
        (
            "const_mut_read",
            false,
            r#"use std::cell::Cell; static mut M:u8=1; const X:u8=unsafe{M}; fn main(){let _=X;}"#,
        ),
        (
            "direct",
            false,
            r#"use std::cell::Cell; const C:&Cell<u8>=&Cell::new(1); fn main(){let _=C;}"#,
        ),
        (
            "discarded",
            true,
            r#"use std::cell::Cell; const X:u8={let _=&Cell::new(1); 3}; fn main(){let _=X;}"#,
        ),
        (
            "empty_array",
            true,
            r#"use std::cell::Cell; const C:&[Cell<u8>;0]=&[]; fn main(){let _=C;}"#,
        ),
        (
            "empty_cell",
            false,
            r#"use std::cell::Cell; const C:&Cell<()>=&Cell::new(()); fn main(){let _=C;}"#,
        ),
        (
            "empty_vec",
            true,
            r#"use std::cell::Cell; const C:&Vec<Cell<u8>>=&Vec::new(); fn main(){let _=C;}"#,
        ),
        (
            "escape_local",
            false,
            r#"use std::cell::Cell; const C:&Cell<u8>={let c=Cell::new(1); &c}; fn main(){let _=C;}"#,
        ),
        (
            "local",
            true,
            r#"use std::cell::Cell; const X:u8={let c=Cell::new(1); let _r=&c; 3}; fn main(){let _=X;}"#,
        ),
        (
            "local_temp",
            true,
            r#"use std::cell::Cell; const X:u8={let _r=&Cell::new(1); 3}; fn main(){let _=X;}"#,
        ),
        (
            "nested_array",
            true,
            r#"use std::cell::Cell; static X:[u8;3]=[1,2,3]; const Y:&u8=&X[1]; fn main(){let _=Y;}"#,
        ),
        (
            "none",
            true,
            r#"use std::cell::Cell; const C:&Option<Cell<u8>>=&None; fn main(){let _=C;}"#,
        ),
        (
            "owned",
            true,
            r#"use std::cell::Cell; const C:Cell<u8>=Cell::new(1); fn main(){let _=C;}"#,
        ),
        (
            "raw",
            false,
            r#"use std::cell::Cell; const C:*const Cell<u8>=&Cell::new(1); fn main(){let _=C;}"#,
        ),
        (
            "ref_tuple",
            false,
            r#"use std::cell::Cell; const C:(&Cell<u8>,u8)=(&Cell::new(1),3); fn main(){let _=C;}"#,
        ),
        (
            "some",
            false,
            r#"use std::cell::Cell; const C:&Option<Cell<u8>>=&Some(Cell::new(1)); fn main(){let _=C;}"#,
        ),
        (
            "static",
            true,
            r#"use std::cell::Cell; struct W(Cell<u8>); unsafe impl Sync for W {} static W1:W=W(Cell::new(1)); const C:&W=&W1; fn main(){let _=C;}"#,
        ),
        (
            "static_cell_get",
            false,
            r#"use std::cell::Cell; struct W(Cell<u8>); unsafe impl Sync for W{} static S:W=W(Cell::new(1)); const X:u8=S.0.get(); fn main(){let _=X;}"#,
        ),
        (
            "static_field",
            true,
            r#"use std::cell::Cell; struct W(Cell<u8>); unsafe impl Sync for W {} static W1:W=W(Cell::new(1)); const C:&Cell<u8>=&W1.0; fn main(){let _=C;}"#,
        ),
        (
            "static_mut_init",
            true,
            r#"use std::cell::Cell; static mut M:u8=1; static X:u8=unsafe{M}; fn main(){let _=X;}"#,
        ),
        (
            "static_pure_read",
            false,
            r#"use std::cell::Cell; struct W{c:Cell<u8>,x:u8} unsafe impl Sync for W{} static S:W=W{c:Cell::new(1),x:2}; const X:u8=S.x; fn main(){let _=X;}"#,
        ),
        (
            "static_pure_ref",
            true,
            r#"use std::cell::Cell; struct W{c:Cell<u8>,x:u8} unsafe impl Sync for W{} static S:W=W{c:Cell::new(1),x:2}; const X:&u8=&S.x; fn main(){let _=X;}"#,
        ),
        (
            "none_static_read",
            false,
            r#"use std::cell::Cell; struct W{c:Option<Cell<u8>>,x:u8} unsafe impl Sync for W{} static S:W=W{c:None,x:4}; const X:u8=S.x; fn main(){let _=X;}"#,
        ),
        (
            "zero_static_read",
            false,
            r#"use std::cell::Cell; struct W{c:[Cell<u8>;0],x:u8} unsafe impl Sync for W{} static S:W=W{c:[],x:4}; const X:u8=S.x; fn main(){let _=X;}"#,
        ),
        (
            "cast_erased",
            false,
            r#"use std::cell::Cell; struct W { c: Cell<u8>, x:u8 } unsafe impl Sync for W {} const X:*const u8=(&Cell::new(1u8) as *const Cell<u8>) as *const u8; fn main(){let _=X;}"#,
        ),
        (
            "reborrow_erased",
            false,
            r#"use std::cell::Cell; struct W { c: Cell<u8>, x:u8 } unsafe impl Sync for W {} const X:&u8=unsafe{&*((&Cell::new(1u8) as *const Cell<u8>) as *const u8)}; fn main(){let _=X;}"#,
        ),
        (
            "pure_field_temporary",
            true,
            r#"use std::cell::Cell; struct W { c: Cell<u8>, x:u8 } unsafe impl Sync for W {} const X:&u8=&W{c:Cell::new(1),x:7}.x; fn main(){let _=X;}"#,
        ),
        (
            "pure_field_local",
            false,
            r#"use std::cell::Cell; struct W { c: Cell<u8>, x:u8 } unsafe impl Sync for W {} const X:&u8={let w=W{c:Cell::new(1),x:7};&w.x}; fn main(){let _=X;}"#,
        ),
        (
            "raw_reinterpret",
            true,
            r#"use std::cell::Cell; struct W { c: Cell<u8>, x:u8 } unsafe impl Sync for W {} const X:*const Cell<u8>=&1u8 as *const u8 as *const Cell<u8>; fn main(){let _=X;}"#,
        ),
        (
            "refcell",
            false,
            r#"use std::cell::RefCell; const X:&RefCell<u8>=&RefCell::new(1); fn main(){let _=X;}"#,
        ),
        (
            "static_oob",
            false,
            r#"static X:[u8;2]=[1,2];const Y:&u8=&X[2];fn main(){let _=Y;}"#,
        ),
        (
            "nested_static",
            true,
            r#"struct A{a:(u8,[u32;2])} static X:A=A{a:(1,[2,3])}; const Y:&u32=&X.a.1[1];fn main(){println!("{}",Y);}"#,
        ),
        (
            "const_reference_table",
            true,
            r#"use std::cell::Cell;struct W(Cell<u8>);unsafe impl Sync for W{} static X:W=W(Cell::new(4));const Y:(&Cell<u8>,u8)=(&X.0,9);fn main(){Y.0.set(7);println!("{} {}", X.0.get(), Y.1);}"#,
        ),
        (
            "inline_const_raw",
            false,
            r#"use std::cell::Cell; const X:*const u8=(&const{Cell::new(1u8)} as *const Cell<u8>) as *const u8; fn main(){let _=X;}"#,
        ),
        (
            "array_const_raw",
            false,
            r#"use std::cell::Cell; const X:*const u8=(&[Cell::new(1u8)] as *const [Cell<u8>;1]) as *const u8; fn main(){let _=X;}"#,
        ),
        (
            "nested_ref_raw",
            false,
            r#"use std::cell::Cell; const X:*const u8=(&(&Cell::new(1u8),) as *const (&Cell<u8>,)) as *const u8; fn main(){let _=X;}"#,
        ),
        (
            "erased_nested_inline",
            false,
            r#"use std::cell::Cell; const X:&u8=unsafe { &**&(&Cell::new(1u8) as *const Cell<u8> as *const u8) }; fn main(){let _=X;}
"#,
        ),
        (
            "erased_aggregate_inline",
            false,
            r#"use std::cell::Cell; const X:&u8=unsafe { &*(&Cell::new(1u8) as *const Cell<u8> as *const u8,).0 }; fn main(){let _=X;}
"#,
        ),
        (
            "cast_integer",
            false,
            r#"use std::cell::Cell; const X:usize=&Cell::new(1u8) as *const Cell<u8> as usize; fn main(){let _=X;}
"#,
        ),
        (
            "static_enum_reduce",
            true,
            r#"static S: Option<u8> = Some(4); const X: u8 = match &S { Some(x) => *x, None => 0 }; fn main() { let _ = X; }
"#,
        ),
        (
            "static_enum",
            true,
            r#"static S:(Option<u8>,)=(Some(4),); const X:u8=match &S.0 {Some(x)=>*x,None=>0}; fn main(){println!("{}",X);}
"#,
        ),
        (
            "static_slice_reborrow",
            true,
            r#"static S:[u8;2]=[3,4]; const A:&[u8]=&S; const B:&[u8]=&*A; fn main(){println!("{} {}",B.len(), B[1]);}
"#,
        ),
        (
            "double_reborrow_control",
            true,
            r#"const X:&u8=unsafe { let p=&1u8 as *const u8; let q=&p; &**q }; fn main(){let _=X;}
"#,
        ),
        (
            "static_slice_rest",
            true,
            r#"static S:[u8;3]=[1,2,3];const R:&[u8]=match &S { [_,r @ ..]=>r };fn main(){println!("{} {}",R.len(),R[0]);}"#,
        ),
        (
            "static_slice_reverse",
            true,
            r#"static S:[u8;3]=[1,2,3];const A:&[u8]=&S;const R:&u8=match A { [..,r]=>r,_=>&0 };fn main(){println!("{}",R);}"#,
        ),
        (
            "static_slice_element",
            true,
            r#"static S:[u8;3]=[1,2,3];const A:&[u8]=&S;const R:&u8=&A[1];fn main(){println!("{}",R);}"#,
        ),
        (
            "phantom_read",
            true,
            r#"use std::cell::Cell;use std::marker::PhantomData;struct W{p:PhantomData<Cell<u8>>,x:u8}unsafe impl Sync for W{}static S:W=W{p:PhantomData,x:4};const R:u8=S.x;fn main(){println!("{}",R);}"#,
        ),
        (
            "mutable_slice_len",
            true,
            r#"use std::cell::Cell; struct W([Cell<u8>;2]); unsafe impl Sync for W {} static S:W=W([Cell::new(1),Cell::new(2)]); const A:&[Cell<u8>]=&S.0; const N:usize=A.len(); fn main(){println!("{}",N);}
"#,
        ),
        (
            "mutable_slice_index_ref",
            true,
            r#"use std::cell::Cell; struct W([Cell<u8>;2]); unsafe impl Sync for W {} static S:W=W([Cell::new(1),Cell::new(2)]); const A:&[Cell<u8>]=&S.0; const B:&Cell<u8>=&A[1]; fn main(){println!("{}",B.get());}
"#,
        ),
        (
            "mutable_array_len",
            true,
            r#"use std::cell::Cell; struct W([Cell<u8>;2]); unsafe impl Sync for W {} static S:W=W([Cell::new(1),Cell::new(2)]); const A:&[Cell<u8>;2]=&S.0; const N:usize=A.len(); fn main(){println!("{}",N);}
"#,
        ),
        (
            "immutable_slice_len",
            true,
            r#"static S:[u8;2]=[1,2];const A:&[u8]=&S;const N:usize=A.len();fn main(){println!("{}",N);}
"#,
        ),
    ] {
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

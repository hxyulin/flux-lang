//! Packing changes storage alignment, never the alignment promised by a borrow.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod cases;
#[test]
fn packed_borrows_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-packed-layout-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "by_value",
            "#[repr(C,packed)]struct P{a:u8,b:u64}fn main(){let mut p=P{a:1,b:9};let x=p.b;p.b=x+1;assert_eq!({p.b},10);}",
            true,
        ),
        (
            "raw_address",
            "#[repr(C,packed)]struct P{a:u8,b:u64}fn main(){let mut p=P{a:1,b:9};let q=&raw mut p.b;p.b=10;unsafe{q.write_unaligned(11);}assert_eq!({p.b},11);}",
            true,
        ),
        (
            "raw_escape",
            "fn address()->*const u32{let x=3;&raw const x}fn main(){let _=address();}",
            true,
        ),
        (
            "shared_field",
            "#[repr(C,packed)]struct P{a:u8,b:u64}fn main(){let p=P{a:1,b:9};let _=&p.b;}",
            false,
        ),
        (
            "mutable_field",
            "#[repr(C,packed)]struct P{a:u8,b:u64}fn main(){let mut p=P{a:1,b:9};let _=&mut p.b;}",
            false,
        ),
        (
            "format_borrow",
            "#[repr(C,packed)]struct P{a:u8,b:u64}fn main(){let p=P{a:1,b:9};println!(\"{}\",p.b);}",
            false,
        ),
        (
            "method_borrow",
            "#[repr(C,packed)]struct P{a:u8,b:u64}fn main(){let p=P{a:1,b:9};let _=p.b.clone();}",
            false,
        ),
        (
            "index_borrow",
            "#[repr(C,packed)]struct P{a:u8,b:[u64;2]}fn main(){let p=P{a:1,b:[2,3]};let _=&p.b[0];}",
            false,
        ),
        (
            "aligned_field",
            "#[repr(C,packed(2))]struct P{a:u8,b:u16,c:u64}fn main(){let p=P{a:1,b:2,c:3};assert_eq!(*&p.b,2);}",
            true,
        ),
        (
            "nested_byte",
            "struct Inner{a:u64,b:u8}#[repr(C,packed)]struct P{a:u8,b:Inner}fn main(){let p=P{a:1,b:Inner{a:2,b:3}};assert_eq!(*&p.b.b,3);}",
            true,
        ),
        (
            "nested_unaligned",
            "struct Inner{a:u64,b:u8}#[repr(C,packed)]struct P{a:u8,b:Inner}fn main(){let p=P{a:1,b:Inner{a:2,b:3}};let _=&p.b.a;}",
            false,
        ),
        (
            "deref_boundary",
            "#[repr(packed)]struct P<'a>{b:&'a u64}fn main(){let x=3;let p=P{b:&x};assert_eq!(*&*p.b,3);}",
            true,
        ),
        (
            "generic_reference",
            "#[repr(packed)]struct P<T>{x:T}fn f<T>(p:&P<T>){let _=&p.x;}fn main(){}",
            false,
        ),
        (
            "generic_pointer",
            "#[repr(packed(8))]struct P<T:?Sized>{x:*const T}fn f<T:?Sized>(p:&P<T>){let _=&p.x;}fn main(){}",
            false,
        ),
        (
            "bad_pack_zero",
            "#[repr(packed(0))]struct P(u32);fn main(){}",
            false,
        ),
        (
            "bad_pack_three",
            "#[repr(packed(3))]struct P(u32);fn main(){}",
            false,
        ),
        (
            "bad_pack_suffix",
            "#[repr(packed(2u32))]struct P(u32);fn main(){}",
            false,
        ),
        (
            "bad_pack_large",
            "#[repr(packed(1073741824))]struct P(u32);fn main(){}",
            false,
        ),
        (
            "conflicting_pack",
            "#[repr(packed(2),packed(4))]struct P(u32);fn main(){}",
            false,
        ),
        (
            "align_conflict",
            "#[repr(packed,align(1))]struct P(u32);fn main(){}",
            false,
        ),
        (
            "transparent_conflict",
            "#[repr(transparent,packed)]struct P(u32);fn main(){}",
            false,
        ),
        ("packed_enum", "#[repr(packed)]enum E{A}fn main(){}", false),
        (
            "aligned_payload",
            "#[repr(align(8))]struct A(u8);#[repr(packed)]struct P(A);fn main(){}",
            false,
        ),
        (
            "transitive_aligned_payload",
            "#[repr(align(8))]struct A(u8);struct B(A);#[repr(packed)]struct P(B);fn main(){}",
            false,
        ),
        (
            "aligned_pointer",
            "#[repr(align(8))]struct A(u8);#[repr(packed)]struct P(*const A);fn main(){}",
            true,
        ),
        (
            "generic_thin_pointer",
            "#[repr(packed(8))]struct P<T>{x:*const T}fn f<T>(p:&P<T>){let _=&p.x;}fn main(){}",
            true,
        ),
        (
            "derived_copy_fields",
            "#[repr(packed)]#[derive(Clone,Copy,Debug,PartialEq,Eq,PartialOrd,Ord,Hash)]struct P{a:u8,b:u64}fn main(){let p=P{a:1,b:2};let q=p.clone();assert_eq!(p,q);println!(\"{p:?}\");}",
            true,
        ),
        (
            "derived_owned_rejection",
            "#[repr(packed)]#[derive(Debug)]struct P{value:String}fn main(){}",
            false,
        ),
        (
            "derived_generic",
            "#[repr(packed)]#[derive(Clone,Debug,PartialEq)]struct P<T>{value:T}fn main(){let p=P{value:7u32};let _=p.clone();}",
            true,
        ),
        (
            "derived_noncopy_generic",
            "#[repr(packed)]#[derive(Clone)]struct P<T>{value:T}fn main(){let p=P{value:String::new()};let _=p.clone();}",
            false,
        ),
        (
            "union_align_conflict",
            "#[repr(packed,align(1))]union P{value:u32}fn main(){}",
            false,
        ),
        (
            "const_raw_packed",
            "#[repr(C,packed)]struct P{a:u8,b:u64}static P0:P=P{a:1,b:2};const RAW:*const u64=&raw const P0.b;fn main(){let _=RAW;}",
            true,
        ),
        (
            "const_unaligned_ref",
            "#[repr(C,packed)]struct P{a:u8,b:u64}static P0:P=P{a:1,b:2};const BAD:&u64=unsafe{&*(&raw const P0.b)};fn main(){let _=BAD;}",
            false,
        ),
        (
            "const_unaligned_read",
            "#[repr(C,packed)]struct P{a:u8,b:u64}static P0:P=P{a:1,b:2};const BAD:u64=unsafe{*(&raw const P0.b)};fn main(){let _=BAD;}",
            false,
        ),
        (
            "array_len_borrow",
            "#[repr(packed)]struct P{b:[u64;2]}fn main(){let p=P{b:[2,3]};let _=p.b.len();}",
            false,
        ),
        (
            "array_empty_borrow",
            "#[repr(packed)]struct P{b:[u64;2]}fn main(){let p=P{b:[2,3]};let _=p.b.is_empty();}",
            false,
        ),
        (
            "raw_nested_lifetime",
            "fn f<'a>(x:&'a u32)->*const &'static u32{&raw const x}fn main(){}",
            false,
        ),
        (
            "generic_unsized_drop",
            "#[repr(C,packed)]struct P<T:?Sized>{tag:u8,value:T}fn main(){}",
            false,
        ),
        (
            "unsized_dropping_slice",
            "struct Leaf;impl Drop for Leaf{fn drop(&mut self){}}#[repr(C,packed)]struct P{tag:u8,value:[Leaf]}fn main(){}",
            false,
        ),
        (
            "unsized_copy_slice",
            "#[repr(C,packed)]struct P{tag:u8,value:[u64]}fn main(){}",
            true,
        ),
        (
            "unsized_str",
            "#[repr(C,packed)]struct P{tag:u8,value:str}fn main(){}",
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

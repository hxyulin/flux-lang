//! Typed constant-pointer reads retain allocation layout and frame lifetimes.
use std::process::Command;

#[test]
fn const_pointer_reads_match_rustc() {
    let dir = std::env::temp_dir().join(format!("flux-pointer-views-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, rustc_accepted, flux_accepted, text) in [
        (
            "dead_dynamic_temp",
            false,
            false,
            r###"const fn ptr(n:u8)->*const u8 { &(n+1) } const X:u8=unsafe{*ptr(7)}; fn main(){let _=X;}"###,
        ),
        (
            "pure_arithmetic_promotion",
            true,
            true,
            r###"const fn ptr()->*const u8 { &(1u8+1) } const X:u8=unsafe{*ptr()}; fn main(){let _=X;}"###,
        ),
        (
            "integer_memory_to_pointer",
            true,
            true,
            r###"const X:*const u8=unsafe{*(&37usize as *const usize as *const *const u8)};fn main(){let _=X;}"###,
        ),
        (
            "absolute_pointer_memory_bits",
            true,
            true,
            r###"const X:usize=unsafe{*(&(37usize as *const u8) as *const *const u8 as *const usize)};fn main(){let _=X;}"###,
        ),
        (
            "absolute_pointer_memory_byte",
            true,
            true,
            r###"const X:u8=unsafe{*(&(37usize as *const u8) as *const *const u8 as *const u8)};fn main(){let _=X;}"###,
        ),
        (
            "dead_dynamic_nested",
            false,
            false,
            r###"const fn p(n:u8)->*const u8{let x=&(n+1);*(&x) as *const u8}const X:u8=unsafe{*p(7)};fn main(){let _=X;}"###,
        ),
        (
            "dead_dynamic_array_field",
            false,
            false,
            r###"const fn p(n:u8)->*const u8{&[n,8][0]}const X:u8=unsafe{*p(7)};fn main(){let _=X;}"###,
        ),
        (
            "bad_bool_reference",
            false,
            false,
            r###"const X:&bool=unsafe{&*(&2u8 as *const u8 as *const bool)};fn main(){let _=X;}"###,
        ),
        (
            "bad_char_reference",
            false,
            false,
            r###"const X:&char=unsafe{&*(&0xd800u32 as *const u32 as *const char)};fn main(){let _=X;}"###,
        ),
        (
            "mutable_parameter_storage",
            true,
            false,
            r###"const fn p(mut n:u8)->u8{let q=std::ptr::addr_of!(n);n=9;unsafe{*q}}const X:u8=p(7);fn main(){let _=X;}"###,
        ),
        (
            "cell_ref",
            true,
            true,
            r###"use std::cell::Cell; const X:&u8=unsafe{*(&Cell::new(&1u8) as *const Cell<&u8> as *const &u8)}; fn main(){let _=X;}"###,
        ),
        (
            "generic",
            true,
            true,
            r###"const fn read<T,U:Copy>(x:&T)->U{unsafe{*(x as *const T as *const U)}} const X:u8=read::<u32,u8>(&0x01020304);fn main(){let _=X;}"###,
        ),
        (
            "bytes",
            true,
            true,
            r###"const X:[u8;4]=unsafe{*(&0x01020304u32 as *const u32 as *const [u8;4])}; fn main(){let _=X;}"###,
        ),
        (
            "words",
            true,
            true,
            r###"const X:[u16;4]=unsafe{*(&0x0102030405060708u64 as *const u64 as *const [u16;4])};fn main(){let _=X;}"###,
        ),
        (
            "one_field",
            true,
            true,
            r###"#[repr(C)]struct S(&'static u8);const X:&u8=unsafe{*(&S(&9) as *const S as *const &u8)};fn main(){let _=X;}"###,
        ),
        (
            "struct_target",
            true,
            true,
            r###"#[repr(C)]#[derive(Copy,Clone)]struct S{a:u16,b:u16}const X:S=unsafe{*(&0x04030201u32 as *const u32 as *const S)};fn main(){let _=X;}"###,
        ),
        (
            "char",
            true,
            true,
            r###"const X:char=unsafe{*(&0x1f980u32 as *const u32 as *const char)};fn main(){let _=X;}"###,
        ),
        (
            "bool",
            true,
            true,
            r###"const X:bool=unsafe{*(&1u8 as *const u8 as *const bool)};fn main(){let _=X;}"###,
        ),
        (
            "nonzero",
            true,
            true,
            r###"use std::num::NonZeroU32;const X:NonZeroU32=unsafe{*(&17u32 as *const u32 as *const NonZeroU32)};fn main(){let _=X;}"###,
        ),
        (
            "nonnull",
            true,
            true,
            r###"use std::ptr::NonNull;const X:NonNull<u8>={let p=&1u8 as *const u8;unsafe{*(&p as *const *const u8 as *const NonNull<u8>)}};fn main(){let _=X;}"###,
        ),
        (
            "static_view",
            true,
            true,
            r###"static S:u32=0x01020304;const X:&u8=unsafe{&*(&S as *const u32 as *const u8)};fn main(){let _=X;}"###,
        ),
        (
            "wide_view",
            true,
            true,
            r###"const X:&[u8]={let s:&[u32]=&[0x04030201,8];unsafe{&*(s as *const [u32] as *const [u8])}};fn main(){let _=X;}"###,
        ),
        (
            "str_bytes",
            true,
            true,
            r###"const X:&[u8]="hello".as_bytes();fn main(){let _=X;}"###,
        ),
        (
            "byte_str",
            true,
            true,
            r###"const X:&str=unsafe{std::str::from_utf8_unchecked(&[104,105])};fn main(){let _=X;}"###,
        ),
        (
            "local",
            true,
            true,
            r###"const X:u8={let n=7u8;unsafe{*(&n as *const u8)}};fn main(){let _=X;}"###,
        ),
        (
            "local_call",
            true,
            true,
            r###"const fn read(p:*const u8)->u8{unsafe{*p}}const X:u8={let n=7u8;read(&n as *const u8)};fn main(){let _=X;}"###,
        ),
        (
            "local_reborrow",
            true,
            true,
            r###"const fn reborrow(x:&u8)->&u8{&*x}const X:u8={let n=7u8;*reborrow(&n)};fn main(){let _=X;}"###,
        ),
        (
            "nested_local_ptr",
            true,
            true,
            r###"const X:u8={let n=7u8;let p=&n as *const u8;let q=&p;unsafe{**q}};fn main(){let _=X;}"###,
        ),
        (
            "copy_local_ref",
            true,
            true,
            r###"use std::cell::Cell;const X:u8={let n=7u8;unsafe{**(&Cell::new(&n) as *const Cell<&u8> as *const &u8)}};fn main(){let _=X;}"###,
        ),
        (
            "zst",
            true,
            true,
            r###"static S:u32=9;const X:&()=unsafe{&*(&S as *const u32 as *const ())};fn main(){let _=X;}"###,
        ),
        (
            "bad_bool",
            false,
            false,
            r###"const X:bool=unsafe{*(&2u8 as *const u8 as *const bool)};fn main(){let _=X;}"###,
        ),
        (
            "bad_char",
            false,
            false,
            r###"const X:char=unsafe{*(&0xd800u32 as *const u32 as *const char)};fn main(){let _=X;}"###,
        ),
        (
            "bad_nonzero",
            false,
            false,
            r###"use std::num::NonZeroU32;const X:NonZeroU32=unsafe{*(&0u32 as *const u32 as *const NonZeroU32)};fn main(){let _=X;}"###,
        ),
        (
            "bad_nonnull",
            false,
            false,
            r###"use std::ptr::NonNull;const X:NonNull<u8>=unsafe{*(&(0usize as *mut u8) as *const *mut u8 as *const NonNull<u8>)};fn main(){let _=X;}"###,
        ),
        (
            "oob",
            false,
            false,
            r###"const X:u64=unsafe{*(&1u8 as *const u8 as *const u64)};fn main(){let _=X;}"###,
        ),
        (
            "dead_scope",
            false,
            false,
            r###"const X:u8={let p={let n=7u8;&n as *const u8};unsafe{*p}};fn main(){let _=X;}"###,
        ),
        (
            "dead_frame",
            false,
            false,
            r###"const fn ptr(n:u8)->*const u8{&n}const X:u8=unsafe{*ptr(7)};fn main(){let _=X;}"###,
        ),
        (
            "escape",
            false,
            false,
            r###"const X:&u8={let n=1u8;unsafe{&*(&n as *const u8)}};fn main(){let _=X;}"###,
        ),
        (
            "cell_escape",
            false,
            false,
            r###"use std::cell::Cell;const X:&u8={let n=1u8;unsafe{*(&Cell::new(&n) as *const Cell<&u8> as *const &u8)}};fn main(){let _=X;}"###,
        ),
        (
            "erased_interior",
            false,
            false,
            r###"use std::cell::Cell;const X:*const u8=(&Cell::new(1u8) as *const Cell<u8>) as *const u8;fn main(){let _=X;}"###,
        ),
        (
            "pointer_integer",
            false,
            false,
            r###"const X:usize=unsafe{*(&(&1u8) as *const &u8 as *const usize)};fn main(){let _=X;}"###,
        ),
        (
            "bad_utf8",
            false,
            false,
            r###"const X:&str=unsafe{std::str::from_utf8_unchecked(&[255])};fn main(){let _=X;}"###,
        ),
        (
            "mutable_local",
            true,
            false,
            r###"const X:u8={let mut n=7u8;let p=std::ptr::addr_of!(n);n=9;unsafe{*p}};fn main(){let _=X;}"###,
        ),
        (
            "padding",
            false,
            false,
            r###"#[repr(C)]struct S(u8,u32);const X:u16=unsafe{*(&S(1,2) as *const S as *const u16)};fn main(){let _=X;}"###,
        ),
        (
            "unsize_view",
            true,
            false,
            r###"const X:&[u8]=unsafe{&*(&0x04030201u32 as *const u32 as *const [u8;4])};fn main(){let _=X;}"###,
        ),
        (
            "large_zst_view",
            true,
            false,
            r#"const X:[[();16777216];2]=unsafe{*(&() as *const () as *const [[();16777216];2])};fn main(){let _=X;}"#,
        ),
        (
            "box_ref",
            false,
            false,
            r#"const X:&Box<u8>=unsafe{&*(&(&1u8 as *const u8) as *const *const u8 as *const Box<u8>)};fn main(){let _=X;}"#,
        ),
        (
            "box_zst",
            true,
            true,
            r#"const X:&Box<()>=unsafe{&*(&(1usize as *const ()) as *const *const () as *const Box<()>)};fn main(){let _=X;}"#,
        ),
        (
            "box_owned",
            false,
            false,
            r#"const X:Box<u8>=unsafe{*(&(&1u8 as *const u8) as *const *const u8 as *const Box<u8>)};fn main(){let _=X;}"#,
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
            rustc_accepted,
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
        assert_eq!(errors.is_empty(), flux_accepted, "flux {name}: {errors:?}");
        if rustc_accepted && !flux_accepted {
            assert!(
                errors.iter().any(|d| d.message.contains("not supported")
                    || d.message.contains("too many fields")),
                "{name}: {errors:?}"
            );
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn large_source_fields_are_read_by_physical_offset() {
    use std::fmt::Write;
    let count = 512;
    let mut text = String::from("#[repr(C)] struct S {");
    for i in 0..count {
        write!(text, "f{i}:u64,").unwrap();
    }
    text.push_str("} const VALUES:[u64;512]=unsafe{*(&S{");
    for i in 0..count {
        write!(text, "f{i}:{i},").unwrap();
    }
    text.push_str("} as *const S as *const [u64;512])};fn main(){let _=VALUES;}");
    let (_, _, hir) = flux_hir::lower_text("large_views.rs", &text);
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let mir = flux_mir::build_file(&hir, &typed);
    assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
    let id = hir
        .items
        .consts
        .iter()
        .find(|(_, c)| c.name.is_some_and(|name| name.as_str() == "VALUES"))
        .unwrap()
        .0;
    let flux_mir::eval::Value::Aggregate(fields) = mir.const_values.items.get(id).unwrap() else {
        panic!("expected an initialized array");
    };
    assert_eq!(fields.len(), count);
    for (i, field) in fields.iter().enumerate() {
        assert_eq!(
            field,
            &flux_mir::eval::Value::Int {
                bits: i as u128,
                ty: flux_ty::IntTy::U64
            }
        );
    }
}

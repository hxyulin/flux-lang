//! Genuine opaque assembly with tied operands and normal Rust place semantics.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
use std::{process::Command, time::Duration};

#[test]
fn inline_asm_matches_rustc() {
    support::oracle::compare_fixture("inline_asm");
}

#[test]
fn inline_asm_contracts_match_rustc() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../out/inline-asm-contracts/{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source) in [
        (
            "char_name",
            r#"fn main(){let mut x=0u64;unsafe{core::arch::asm!("/*{'a'}*/",'a'=inout(reg)x)}}"#,
        ),
        (
            "float_name",
            r#"fn main(){let mut x=0u64;unsafe{core::arch::asm!("/*{1.5}*/",1.5=inout(reg)x)}}"#,
        ),
        (
            "byte_name",
            r#"fn main(){let mut x=0u64;unsafe{core::arch::asm!("/*{b'a'}*/",b'a'=inout(reg)x)}}"#,
        ),
        (
            "underscore_name",
            r#"fn main(){let mut x=0u64;unsafe{core::arch::asm!("/*{_}*/",_=inout(reg)x)}}"#,
        ),
        (
            "raw_name",
            r#"fn main(){let mut x=0u64;unsafe{core::arch::asm!("/*{type}*/",r#type=inout(reg)x)}}"#,
        ),
        (
            "bare",
            r#"fn main(){let mut x=3u32;unsafe{asm!("/*{}*/",inout(reg)x)}}"#,
        ),
        (
            "core_root",
            r#"fn main(){let mut x=3u32;unsafe{core::asm!("/*{}*/",inout(reg)x)}}"#,
        ),
        (
            "std_root",
            r#"fn main(){let mut x=3u32;unsafe{std::asm!("/*{}*/",inout(reg)x)}}"#,
        ),
        (
            "alias",
            r#"use std::arch::asm as assembly;fn main(){let mut x=3u32;unsafe{assembly!("/*{}*/",inout(reg)x)}}"#,
        ),
        (
            "missing_named",
            r#"fn main(){unsafe{core::arch::asm!("", x=)}}"#,
        ),
        (
            "missing_operand",
            r#"fn main(){unsafe{core::arch::asm!("",inout(reg))}}"#,
        ),
        (
            "wrong_brace",
            r#"fn main(){let mut x=1u64;unsafe{core::arch::asm!("{",inout(reg)x)}}"#,
        ),
        (
            "bad_option",
            r#"fn main(){unsafe{core::arch::asm!("nop",options(wrong))}}"#,
        ),
        (
            "repeat_options",
            r#"fn main(){unsafe{core::arch::asm!("nop",options(nostack,nostack))}}"#,
        ),
        (
            "scalar",
            r#"fn main(){let mut x=3u32;unsafe{core::arch::asm!("add {0:w}, {0:w}, #1",inout(reg)x)}}"#,
        ),
        (
            "pointer",
            r#"fn main(){let mut x=3;let mut p=&raw mut x;unsafe{core::arch::asm!("/*{}*/",inout(reg)p)}}"#,
        ),
        (
            "reference",
            r#"fn main(){let x=3;let mut p=&x;unsafe{core::arch::asm!("/*{}*/",inout(reg)p)}assert_eq!(*p,3)}"#,
        ),
        (
            "vreg_integer",
            r#"fn main(){let mut x=3i64;unsafe{core::arch::asm!("/*{0:d}*/",inout(vreg)x)}}"#,
        ),
        (
            "vreg_small",
            r#"fn main(){let mut x=3u8;unsafe{core::arch::asm!("/*{0:b}*/",inout(vreg)x)}}"#,
        ),
        (
            "float",
            r#"fn main(){let mut x=3.0f32;unsafe{core::arch::asm!("/*{0:d}*/",inout(vreg)x)}}"#,
        ),
        (
            "pure",
            r#"fn main(){let mut x=3u32;unsafe{core::arch::asm!("/*{0:w}*/",inout(reg)x,options(pure,nomem))}}"#,
        ),
        (
            "unsafe",
            r#"fn main(){let mut x=3u32;core::arch::asm!("/*{}*/",inout(reg)x)}"#,
        ),
        (
            "immutable",
            r#"fn main(){let x=3u32;unsafe{core::arch::asm!("/*{}*/",inout(reg)x)}}"#,
        ),
        (
            "uninitialized",
            r#"fn main(){let mut x:u32;unsafe{core::arch::asm!("/*{}*/",inout(reg)x)}}"#,
        ),
        (
            "bad_type",
            r#"fn main(){let mut x=true;unsafe{core::arch::asm!("/*{}*/",inout(reg)x)}}"#,
        ),
        (
            "wide",
            r#"fn main(){let mut x=3u128;unsafe{core::arch::asm!("/*{}*/",inout(reg)x)}}"#,
        ),
        (
            "fat",
            r#"fn main(){let mut x:&[u8]=&[];unsafe{core::arch::asm!("/*{}*/",inout(reg)x)}}"#,
        ),
        (
            "unknown",
            r#"fn main(){let mut x=3u32;unsafe{core::arch::asm!("/*{missing}*/",inout(reg)x)}}"#,
        ),
        (
            "unused",
            r#"fn main(){let mut x=3u32;unsafe{core::arch::asm!("",inout(reg)x)}}"#,
        ),
        (
            "modifier",
            r#"fn main(){let mut x=3u32;unsafe{core::arch::asm!("/*{0:v}*/",inout(reg)x)}}"#,
        ),
        (
            "options",
            r#"fn main(){let mut x=3u32;unsafe{core::arch::asm!("/*{}*/",inout(reg)x,options(nomem,readonly))}}"#,
        ),
        (
            "pure_memory",
            r#"fn main(){let mut x=3u32;unsafe{core::arch::asm!("/*{}*/",inout(reg)x,options(pure))}}"#,
        ),
        (
            "const",
            r#"const X:u32={let mut x=3u32;unsafe{core::arch::asm!("/*{}*/",inout(reg)x)}x};fn main(){}"#,
        ),
        (
            "const_fn",
            r#"const fn f()->u32{let mut x=3u32;unsafe{core::arch::asm!("/*{}*/",inout(reg)x)}x}fn main(){}"#,
        ),
    ] {
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let reference = support::process::run(
            Command::new("rustc")
                .args(["--edition=2024", "--emit=metadata"])
                .arg(&path)
                .arg("-o")
                .arg(root.join(format!("{name}.rmeta"))),
            &root,
            &format!("{name}-rustc"),
            Duration::from_secs(30),
        )
        .completed();
        let (_, _, hir) = flux_hir::lower_text(format!("{name}.rs"), source);
        let mut errors = hir.diagnostics.clone();
        if errors.is_empty() {
            let typed = flux_mir::check_file(&hir);
            errors.extend(typed.diagnostics.clone());
            if errors.is_empty() {
                let mir = flux_mir::build_file(&hir, &typed);
                errors.extend(mir.diagnostics);
            }
        }
        assert_eq!(
            errors.is_empty(),
            reference.status.success(),
            "{name}: flux={errors:?}, rustc={}",
            String::from_utf8_lossy(&reference.stderr)
        );
    }
}

#[test]
fn vector_inline_asm_matches_rustc() {
    support::oracle::compare_fixture("vector_inline_asm");
}

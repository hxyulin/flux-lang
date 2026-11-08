//! Native Linux coverage stays explicit until the full runtime is ported.
#![cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod support;

#[test]
fn x86_64_programs_match_rustc() {
    for name in [
        "backend_full_selection",
        "x86_asm_registers",
        "btree_map",
        "pointer_slice_helpers",
        "atomic_family",
        "monotonic_time",
        "backend_system_time",
        "unix_file_io",
        "process_pipes",
        "scoped_threads",
        "thread_local",
    ] {
        support::oracle::compare_fixture(name);
    }
}

#[test]
fn x86_asm_register_types_match_rustc() {
    use std::{process::Command, time::Duration};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../out/x86-asm-types/{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source) in [
        (
            "byte",
            r#"fn main(){let mut x=1u8;unsafe{core::arch::asm!("/*{}*/",inout(reg_byte)x)}}"#,
        ),
        (
            "byte_wrong_width",
            r#"fn main(){let mut x=1u32;unsafe{core::arch::asm!("/*{}*/",inout(reg_byte)x)}}"#,
        ),
        (
            "byte_modifier",
            r#"fn main(){let mut x=1u8;unsafe{core::arch::asm!("/*{0:e}*/",inout(reg_byte)x)}}"#,
        ),
        (
            "xmm_float",
            r#"fn main(){let mut x=1f32;unsafe{core::arch::asm!("/*{}*/",inout(xmm_reg)x)}}"#,
        ),
        (
            "xmm_bool",
            r#"fn main(){let mut x=true;unsafe{core::arch::asm!("/*{}*/",inout(xmm_reg)x)}}"#,
        ),
        (
            "xmm_modifier",
            r#"fn main(){let mut x=1f32;unsafe{core::arch::asm!("/*{0:e}*/",inout(xmm_reg)x)}}"#,
        ),
        (
            "split",
            r#"fn main(){let x=1u8;let y:u8;unsafe{core::arch::asm!("/*{}*/",inout(reg_byte)x=>y)}assert_eq!(y,1)}"#,
        ),
        (
            "split_untyped_output",
            r#"fn main(){let y;unsafe{core::arch::asm!("/*{}*/",inout(reg_byte)1u8=>y)}}"#,
        ),
        (
            "split_uninitialized_input",
            r#"fn main(){let x:u8;let y:u8;unsafe{core::arch::asm!("/*{}*/",inout(reg_byte)x=>y)}}"#,
        ),
        (
            "split_immutable_output",
            r#"fn main(){let y=0u8;unsafe{core::arch::asm!("/*{}*/",inout(reg_byte)1u8=>y)}assert_eq!(y,1)}"#,
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
                errors.extend(flux_mir::build_file(&hir, &typed).diagnostics);
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

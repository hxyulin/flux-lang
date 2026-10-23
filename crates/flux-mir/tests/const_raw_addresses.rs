//! Thin raw constant addresses retain pointer restrictions and validity checks.
use std::process::Command;

#[test]
fn const_addresses_match_rustc() {
    let dir = std::env::temp_dir().join(format!("flux-const-addresses-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, accepted, text) in [
        (
            "empty_vec",
            true,
            r#"const V:Vec<u32>=Vec::new(); fn main(){let _=V;}"#,
        ),
        (
            "address",
            true,
            r#"const P:*const u8=37usize as *const u8; fn main(){let _=P;}"#,
        ),
        (
            "negative",
            true,
            r#"const P:*const u8=-1i8 as *const u8; fn main(){let _=P;}"#,
        ),
        (
            "dangling",
            true,
            r#"const N:std::ptr::NonNull<u64>=std::ptr::NonNull::dangling(); fn main(){let _=N;}"#,
        ),
        (
            "roundtrip",
            false,
            r#"const X:usize=(37usize as *const u8) as usize; fn main(){let _=X;}"#,
        ),
        (
            "null_roundtrip",
            false,
            r#"const X:usize=(0usize as *const u8) as usize; fn main(){let _=X;}"#,
        ),
        (
            "comparison",
            false,
            r#"const P:*const u8=37usize as *const u8; const B:bool=P==P; fn main(){let _=B;}"#,
        ),
        (
            "dereference",
            false,
            r#"const X:u8=unsafe{*(37usize as *const u8)}; fn main(){let _=X;}"#,
        ),
        (
            "null_nonnull",
            false,
            r#"const X:std::ptr::NonNull<u8>=unsafe{std::ptr::NonNull::new_unchecked(0usize as *mut u8)}; fn main(){let _=X;}"#,
        ),
        (
            "some_null",
            false,
            r#"const X:Option<std::ptr::NonNull<u8>>=Some(unsafe{std::ptr::NonNull::new_unchecked(0usize as *mut u8)}); fn main(){let _=X;}"#,
        ),
        (
            "reference",
            false,
            r#"const X:&u8=unsafe{&*(37usize as *const u8)}; fn main(){let _=X;}"#,
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

#[test]
fn wide_address_casts_remain_rejected() {
    let text =
        "const P:*const [u8]=(37usize as *const [u8; 3]) as *const [u8]; fn main(){let _=P;}";
    let (_, _, hir) = flux_hir::lower_text("unsize.rs", text);
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let typed = flux_mir::check_file(&hir);
    assert!(
        typed
            .diagnostics
            .iter()
            .any(|d| d.message.contains("cannot cast thin pointer")),
        "{:?}",
        typed.diagnostics
    );
}

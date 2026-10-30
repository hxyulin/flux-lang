//! Integer exposure consumes a reified address without inventing call rights.
#[test]
fn function_address_casts_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-function-address-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, body, valid) in [
        ("item", "fn f(){}fn main(){let _=f as usize;}", true),
        (
            "static_input",
            "fn f(_: &'static u32){}fn main(){let _=f as usize;}",
            true,
        ),
        (
            "linked_return",
            "fn f(x:&u32)->&u32{x}fn main(){let _=f as usize;}",
            true,
        ),
        (
            "early_owner",
            "struct S<'a>(&'a u32);impl<'a>S<'a>{fn f(&self)->&'a u32{self.0}}fn main(){let _=S::f as usize;}",
            true,
        ),
        (
            "generic",
            "fn f<T>(x:T)->T{x}fn g<T>()->usize{f::<T> as usize}fn main(){let _=g::<u32>();}",
            true,
        ),
        (
            "c",
            "extern \"C\" fn f(_:u8)->u8{7}fn main(){let _=f as usize;}",
            true,
        ),
        (
            "unsafe",
            "unsafe fn f(){}fn main(){let _=f as usize;}",
            true,
        ),
        #[cfg(target_arch = "aarch64")]
        (
            "feature",
            "#[target_feature(enable=\"neon\")]fn f(){}fn main(){let _=f as usize;}",
            true,
        ),
        (
            "pointer",
            "fn f(){}fn main(){let p:fn()=f;let _=p as usize;}",
            true,
        ),
        (
            "widths",
            "fn f(){}fn main(){let _=f as u8;let _=f as i8;let _=f as u128;let _=f as i128;}",
            true,
        ),
        (
            "const_item",
            "fn f(){}const ADDRESS:usize=f as usize;fn main(){let _=ADDRESS;}",
            false,
        ),
        (
            "const_pointer",
            "fn f(){}const P:fn()=f;const ADDRESS:usize=P as usize;fn main(){let _=ADDRESS;}",
            false,
        ),
        (
            "static_address",
            "fn f(){}static ADDRESS:usize=f as usize;fn main(){let _=ADDRESS;}",
            false,
        ),
        ("float", "fn f(){}fn main(){let _=f as f64;}", false),
        ("bool", "fn f(){}fn main(){let _=f as bool;}", false),
        ("integer_to_fn", "fn main(){let _=0usize as fn();}", false),
        (
            "intrinsic",
            "fn main(){let _=core::mem::transmute::<u32,u32> as usize;}",
            false,
        ),
        #[cfg(target_arch = "aarch64")]
        (
            "no_safe_call",
            "#[target_feature(enable=\"sha2\")]fn f(){}fn main(){let _=f as usize;f();}",
            false,
        ),
        (
            "unsafe_pointer_safe_call",
            "unsafe fn f(){}fn main(){let p:unsafe fn()=f;let _=p as usize;p();}",
            false,
        ),
    ] {
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, body).unwrap();
        let oracle = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("oracle.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            oracle.status.success(),
            valid,
            "rustc {name}: {}",
            String::from_utf8_lossy(&oracle.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text(&path, body);
        assert!(hir.diagnostics.is_empty(), "{name}: {:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        let mut errors = typed.diagnostics.clone();
        if errors.is_empty() {
            errors.extend(flux_mir::build_file(&hir, &typed).diagnostics);
        }
        assert_eq!(errors.is_empty(), valid, "flux {name}: {errors:?}");
    }
}

//! Raw slice pointers use their address/length aggregate at C call boundaries.
const SOURCE: &str = r#"
unsafe extern "C" {
    fn host_slice(a: usize, p: *mut [u8], b: usize) -> *mut [u8];
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn flux_slice(a: usize, p: *mut [u8], b: usize) -> *mut [u8] {
    unsafe { host_slice(a, p, b) }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn flux_indirect(
    f: unsafe extern "C" fn(usize, *mut [u8], usize) -> *mut [u8],
    a: usize, p: *mut [u8], b: usize,
) -> *mut [u8] { unsafe { f(a, p, b) } }
#[unsafe(no_mangle)]
pub extern "C" fn flux_text(p: *const str) -> *const str { p }
#[unsafe(no_mangle)]
pub extern "C" fn flux_spilled(a:usize,b:usize,c:usize,d:usize,e:usize,f:usize,g:usize,p:*const [u8],z:usize)->*const [u8] {
    if a+b+c+d+e+f+g == 28 && z == 789 { p } else { core::ptr::slice_from_raw_parts(core::ptr::null(),0) }
}
"#;

fn module() -> keel_ir::Module {
    let (_, _, hir) = flux_hir::lower_text("slice-c-abi.rs", SOURCE);
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let mir = flux_mir::build_file(&hir, &typed);
    assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
    let module = flux_codegen::lower_module(&hir, &typed, &mir).unwrap();
    keel_ir::verify_module(&module).unwrap();
    module
}

#[test]
fn slice_pointers_lower_as_c_aggregates() {
    let module = module();
    let function = module
        .functions
        .iter()
        .find(|f| f.name == "flux_slice")
        .unwrap();
    assert!(
        function
            .sig
            .params
            .iter()
            .any(|p| matches!(p.purpose, keel_ir::Purpose::ByVal(_)))
    );
    assert!(
        function
            .sig
            .params
            .iter()
            .any(|p| matches!(p.purpose, keel_ir::Purpose::Sret(_)))
    );
    for target in [
        flux_codegen::Target::Aarch64AppleDarwin,
        flux_codegen::Target::X86_64LinuxGnu,
    ] {
        flux_codegen::compile_object(
            &mut module.clone(),
            target,
            flux_codegen::OptimizationTier::None,
        )
        .unwrap();
    }
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[test]
fn native_slice_calls_match_rustc() {
    use flux_codegen::{OptimizationTier, Target};
    let root = std::env::temp_dir().join(format!("flux-slice-c-abi-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let source = root.join("reference.rs");
    std::fs::write(&source, SOURCE).unwrap();
    let driver = root.join("driver.rs");
    std::fs::write(&driver, r#"
#![allow(improper_ctypes, improper_ctypes_definitions)]
unsafe extern "C" {
    fn flux_slice(a: usize, p: *mut [u8], b: usize) -> *mut [u8];
    fn flux_indirect(f: unsafe extern "C" fn(usize,*mut [u8],usize)->*mut [u8],a:usize,p:*mut [u8],b:usize)->*mut [u8];
    fn flux_text(p: *const str) -> *const str;
    fn flux_spilled(a:usize,b:usize,c:usize,d:usize,e:usize,f:usize,g:usize,p:*const [u8],z:usize)->*const [u8];
}
#[unsafe(no_mangle)]
unsafe extern "C" fn host_slice(a:usize,p:*mut [u8],b:usize)->*mut [u8] {
    assert_eq!(a, 123);
    assert_eq!(b, 789);
    let slice = unsafe { &mut *p };
    assert_eq!(slice, [10,20,30,40]);
    slice[1] += 7;
    std::ptr::slice_from_raw_parts_mut(slice.as_mut_ptr().wrapping_add(1),2)
}
fn main() {
    for indirect in [false,true] {
        let mut values = [10,20,30,40];
        let p = std::ptr::slice_from_raw_parts_mut(values.as_mut_ptr(),4);
        let result = unsafe { if indirect { flux_indirect(host_slice,123,p,789) } else { flux_slice(123,p,789) } };
        assert_eq!(result as *mut u8, values.as_mut_ptr().wrapping_add(1));
        assert_eq!(unsafe { &*result }, [27,30]);
        assert_eq!(values, [10,27,30,40]);
    }
    let values = [1,2,3,4,5];
    let result = unsafe { flux_spilled(1,2,3,4,5,6,7,&values as *const [u8],789) };
    assert_eq!(unsafe { &*result }, values);
    let text = "raw string";
    let result = unsafe { flux_text(text as *const str) };
    assert_eq!(unsafe { &*result }, text);
    let empty = std::ptr::slice_from_raw_parts_mut(std::ptr::null_mut::<u8>(), 0);
    let result = unsafe { flux_indirect(identity, 123, empty, 789) };
    assert!((result as *mut u8).is_null());
    assert_eq!(result.len(),0);
}
unsafe extern "C" fn identity(_:usize,p:*mut [u8],_:usize)->*mut [u8]{p}
"#).unwrap();
    for (name, tier) in [
        ("rustc", None),
        ("none", Some(OptimizationTier::None)),
        ("full", Some(OptimizationTier::Full)),
    ] {
        let object = root.join(format!("{name}.o"));
        if let Some(tier) = tier {
            let bytes =
                flux_codegen::compile_object(&mut module(), Target::Aarch64AppleDarwin, tier)
                    .unwrap();
            std::fs::write(&object, bytes).unwrap();
        } else {
            let output = std::process::Command::new("rustc")
                .args([
                    "--edition=2024",
                    "--crate-type=lib",
                    "--emit=obj",
                    "-C",
                    "panic=abort",
                ])
                .arg(&source)
                .arg("-o")
                .arg(&object)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let binary = root.join(name);
        let output = std::process::Command::new("rustc")
            .args(["--edition=2024", "-C", "panic=abort", "-C"])
            .arg(format!("link-arg={}", object.display()))
            .arg(&driver)
            .arg("-o")
            .arg(&binary)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let output = std::process::Command::new(binary).output().unwrap();
        assert!(
            output.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn other_wide_pointers_and_variadic_slices_still_reject() {
    for source in [
        "trait Trait {} unsafe extern \"C\" { fn take(p:*const dyn Trait); } fn main(){}",
        "extern \"C\" fn take(p:&[u8]){} fn main(){}",
        "unsafe extern \"C\" { fn take(n:usize,...); } fn main(){unsafe{take(0,core::ptr::slice_from_raw_parts(core::ptr::null::<u8>(),0));}}",
    ] {
        let (_, _, hir) = flux_hir::lower_text("slice-c-abi-negative.rs", source);
        assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        if !typed.diagnostics.is_empty() {
            assert!(
                typed
                    .diagnostics
                    .iter()
                    .any(|d| d.message.contains("passing slices")
                        || d.message.contains("passing references")),
                "{:?}",
                typed.diagnostics
            );
            continue;
        }
        let mir = flux_mir::build_file(&hir, &typed);
        assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
        let errors = flux_codegen::lower_module(&hir, &typed, &mir).unwrap_err();
        assert!(
            errors
                .iter()
                .all(|d| d.message.contains("C ABI is not supported")),
            "{errors:?}"
        );
    }
}

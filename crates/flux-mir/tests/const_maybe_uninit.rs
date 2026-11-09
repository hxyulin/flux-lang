//! Uninitialized storage is valid only behind MaybeUninit's validity boundary.
fn compare(name: &str, source: &str, accepted: bool) {
    let root = std::env::temp_dir().join(format!(
        "flux-const-uninit-{name}-{accepted}-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("case.rs");
    std::fs::write(&path, source).unwrap();
    let output = std::process::Command::new("rustc")
        .args(["--edition=2024", "--emit=metadata", "--out-dir"])
        .arg(&root)
        .arg(&path)
        .output()
        .unwrap();
    assert_eq!(
        output.status.success(),
        accepted,
        "rustc {name}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let (_, _, hir) = flux_hir::lower_text(&path, source);
    let mut errors = hir.diagnostics.clone();
    if errors.is_empty() {
        let typed = flux_mir::check_file(&hir);
        errors.extend(typed.diagnostics.clone());
        if errors.is_empty() {
            errors.extend(flux_mir::build_file(&hir, &typed).diagnostics);
        }
    }
    assert_eq!(errors.is_empty(), accepted, "Flux {name}: {errors:?}");
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn storage_and_initialized_payloads() {
    for (name, source) in [
        (
            "scalar",
            "const X:core::mem::MaybeUninit<u32>=core::mem::MaybeUninit::uninit();fn main(){let _=X;}",
        ),
        (
            "reference",
            "const X:core::mem::MaybeUninit<&'static u32>=core::mem::MaybeUninit::uninit();fn main(){let _=X;}",
        ),
        (
            "array",
            "const X:[core::mem::MaybeUninit<u32>;8]=[core::mem::MaybeUninit::uninit();8];fn main(){let _=X;}",
        ),
        (
            "static_cell",
            "struct Slot<T>(core::cell::UnsafeCell<core::mem::MaybeUninit<T>>);unsafe impl<T:Send+Sync> Sync for Slot<T>{}impl<T> Slot<T>{const fn new()->Self{Self(core::cell::UnsafeCell::new(core::mem::MaybeUninit::uninit()))}}static X:Slot<Vec<usize>>=Slot::new();fn main(){let _=&X;}",
        ),
        (
            "initialized",
            "const X:u32=unsafe{core::mem::MaybeUninit::new(42).assume_init()};fn main(){assert_eq!(X,42);}",
        ),
    ] {
        compare(name, source, true);
    }
}
#[test]
fn uninitialized_reads_are_rejected() {
    for (name, source) in [
        (
            "integer",
            "const X:u32=unsafe{core::mem::MaybeUninit::<u32>::uninit().assume_init()};fn main(){}",
        ),
        (
            "reference",
            "const X:&u32=unsafe{core::mem::MaybeUninit::<&u32>::uninit().assume_init()};fn main(){}",
        ),
        (
            "nested",
            "const X:Option<u32>=Some(unsafe{core::mem::MaybeUninit::<u32>::uninit().assume_init()});fn main(){}",
        ),
        (
            "reinterpret",
            "const X:bool=unsafe{core::mem::transmute(core::mem::MaybeUninit::<bool>::uninit())};fn main(){}",
        ),
    ] {
        compare(name, source, false);
    }
}

//! Sized UnsafeCell: structural traits, borrow safety and const boundaries.
//! Negative programs are compiled only; none are executed.
fn errors(text: &str) -> Vec<String> {
    let (db, root, hir) = flux_hir::lower_text("unsafe_cell.rs", text);
    assert!(db.file(root).parse.errors().is_empty());
    let mut errors = hir.diagnostics.clone();
    let typed = flux_mir::check_file(&hir);
    errors.extend(typed.diagnostics.iter().cloned());
    if errors.is_empty() {
        errors.extend(flux_mir::build_file(&hir, &typed).diagnostics);
    }
    errors.into_iter().map(|d| d.message).collect()
}

fn rustc_accepts(text: &str, case: usize) -> bool {
    let dir = std::env::temp_dir().join(format!("flux-unsafe-cell-{}-{case}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("probe.rs");
    std::fs::write(&path, text).unwrap();
    let output = std::process::Command::new("rustc")
        .args(["--edition=2024", "--emit=metadata"])
        .arg(&path)
        .arg("--out-dir")
        .arg(&dir)
        .output()
        .unwrap();
    std::fs::remove_dir_all(dir).unwrap();
    output.status.success()
}

#[test]
fn sized_traits_and_const_construction() {
    for text in [
        "use core::cell::UnsafeCell; fn send<T: Send>() {} fn main() { send::<UnsafeCell<u32>>(); }",
        "use core::cell::UnsafeCell; struct W<T>(UnsafeCell<T>); unsafe impl<T: Sync> Sync for W<T> {} fn sync<T: Sync>() {} fn main() { sync::<W<u32>>(); }",
        "use core::cell::UnsafeCell; const C: UnsafeCell<u32> = UnsafeCell::new(3); fn main() { let mut c = C; *c.get_mut() = 4; let _ = c.into_inner(); }",
    ] {
        assert!(errors(text).is_empty(), "{text}: {:?}", errors(text));
    }
}

#[test]
fn invalid_rust_is_rejected_without_execution() {
    for (case, (text, expected)) in [
        ("use core::cell::UnsafeCell; fn sync<T: Sync>() {} fn main() { sync::<UnsafeCell<u32>>(); }", "Sync"),
        ("use core::cell::UnsafeCell; struct W(UnsafeCell<u32>); fn sync<T: Sync>() {} fn main() { sync::<W>(); }", "Sync"),
        ("use core::cell::UnsafeCell; fn send<T: Send>() {} fn main() { send::<UnsafeCell<*mut u8>>(); }", "Send"),
        ("use core::cell::UnsafeCell; fn copy<T: Copy>() {} fn main() { copy::<UnsafeCell<u32>>(); }", "Copy"),
        ("use core::cell::UnsafeCell; fn clone<T: Clone>() {} fn main() { clone::<UnsafeCell<u32>>(); }", "Clone"),
        ("use core::cell::UnsafeCell; fn shorten<'a>(x: &'a UnsafeCell<&'static u32>) -> &'a UnsafeCell<&'a u32> { x } fn main() {}", "lifetime"),
        ("use core::cell::UnsafeCell; fn main() { let c = UnsafeCell::new(1); *c.get() = 2; }", "unsafe"),
        ("use core::cell::UnsafeCell; fn main() { let mut c = UnsafeCell::new(1); let r = c.get_mut(); let _s = c.get(); *r = 3; }", "borrow"),
        ("use core::cell::UnsafeCell; fn leak<'a>() -> &'a mut u32 { let mut c = UnsafeCell::new(1); c.get_mut() } fn main() {}", "return"),
        ("use core::cell::UnsafeCell; const C: &UnsafeCell<u32> = &UnsafeCell::new(1); fn main() {}", "constants"),
        ("use core::cell::UnsafeCell; const C: *const UnsafeCell<u32> = &UnsafeCell::new(1); fn main() {}", "constants"),
    ].into_iter().enumerate() {
        assert!(!rustc_accepts(text, case), "rustc accepted negative case {case}");
        let found = errors(text);
        assert!(found.iter().any(|e| e.contains(expected)), "{text}: expected {expected}, got {found:?}");
    }
}

#[test]
fn borrowed_const_storage_is_explicitly_unsupported() {
    for (case, text) in [
        "use core::cell::UnsafeCell; const C: u32 = UnsafeCell::new(1).into_inner(); fn main() {}",
        "use core::cell::UnsafeCell; const C: u32 = { let mut c = UnsafeCell::new(1); *c.get_mut() }; fn main() {}",
        "use core::cell::UnsafeCell; const C: u32 = unsafe { *UnsafeCell::new(1).get() }; fn main() {}",
        "use core::cell::UnsafeCell; const C: &u8 = unsafe { *(&UnsafeCell::new(&1u8) as *const UnsafeCell<&u8> as *const &u8) }; fn main() {}",
        "use core::cell::UnsafeCell; struct W(UnsafeCell<&'static u8>); const C: &u8 = unsafe { *(&W(UnsafeCell::new(&1u8)) as *const W as *const &u8) }; fn main() {}",
    ].into_iter().enumerate() {
        assert!(rustc_accepts(text, 100 + case), "rustc rejected valid const case {case}");
        let found = errors(text);
        assert!(found.iter().any(|e| e.contains("not supported") || e.contains("non-const")), "{text}: {found:?}");
    }
}

#[test]
fn layout_preserves_payload_and_suppresses_niches() {
    let text = "use core::cell::UnsafeCell; enum Void {} #[repr(align(64))] struct A(u8); fn probe(a: UnsafeCell<A>, b: UnsafeCell<Void>, c: UnsafeCell<bool>, d: UnsafeCell<()>) {} fn main() {}";
    let (_, _, hir) = flux_hir::lower_text("layout.rs", text);
    let typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let mir = flux_mir::build_file(&hir, &typed);
    assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
    let layouts = flux_mir::layout::Layouts::new(&hir.items, &typed.sigs, &mir.discriminants);
    let (id, _) = hir
        .items
        .functions
        .iter()
        .find(|(_, f)| f.name.as_str() == "probe")
        .unwrap();
    let types = &typed.sigs.fns[id].params;
    for (ty, size, align, uninhabited) in [
        (types[0], 64, 64, false),
        (types[1], 0, 1, true),
        (types[2], 1, 1, false),
        (types[3], 0, 1, false),
    ] {
        let layout = layouts.of(ty);
        assert_eq!(
            (layout.size, layout.align, layout.uninhabited),
            (size, align, uninhabited)
        );
        assert!(layout.niche.is_none());
        assert_eq!(layout.field_offset(0, 0), 0);
    }
}

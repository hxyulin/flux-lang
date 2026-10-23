//! Executable union boundary: unsupported cases fail explicitly, before codegen.
fn errors(text: &str) -> Vec<String> {
    let (db, root, hir) = flux_hir::lower_text("union.rs", text);
    assert!(db.file(root).parse.errors().is_empty());
    let mut errors = hir.diagnostics.clone();
    let typed = flux_mir::check_file(&hir);
    errors.extend(typed.diagnostics.iter().cloned());
    if errors.is_empty() {
        let mir = flux_mir::build_file(&hir, &typed);
        errors.extend(mir.diagnostics);
    }
    errors.into_iter().map(|d| d.message).collect()
}

#[test]
fn supported_integer_unions() {
    for text in [
        "union U { a: u32, b: i32 } fn main() { let mut u = U { a: 1 }; u.b = -2; let _ = unsafe { u.a }; }",
        "#[derive(Copy, Clone)] union U { a: u8, b: u64 } fn main() { let u = U { b: 3 }; let v = u.clone(); let _ = unsafe { (u.a, v.b) }; }",
        "union U { a: u32, #[cfg(any())] a: Missing, #[cfg_attr(all(), cfg(any()))] b: Missing } fn main() { let _ = U { a: 1 }; }",
        "union U { a: u32, b: i32 } fn main() { let u: U; u = U { a: 1 }; let _ = unsafe { u.b }; }",
    ] {
        assert!(errors(text).is_empty(), "{text}: {:?}", errors(text));
    }
}

#[test]
fn union_rejections() {
    // Rust also rejects invalid accesses and borrows. Valid Rust outside the
    // bounded subset receives explicit unsupported diagnostics.
    for (text, expected) in [
        (
            "#[repr(C, Rust)] union U { a: u32 } fn main() {}",
            "conflicting representation hints",
        ),
        (
            "union U { a: u32, b: i32 } fn main() { let u = U { a: 1 }; let _ = u.b; }",
            "access to union field is unsafe",
        ),
        (
            "union U { a: u32 } fn main() { let u = U { a: 1 }; let _ = &u.a; }",
            "access to union field is unsafe",
        ),
        (
            "union U { a: u32 } fn main() { let mut u = U { a: 1 }; let _ = &mut u.a; }",
            "access to union field is unsafe",
        ),
        (
            "union U { a: u32 } fn main() { let mut u = U { a: 1 }; u.a += 1; }",
            "access to union field is unsafe",
        ),
        (
            "union U { a: u32, b: i32 } fn main() { let _ = U { a: 1, b: 2 }; }",
            "exactly one field",
        ),
        (
            "union U { a: u32 } fn main() { let _ = U {}; }",
            "exactly one field",
        ),
        (
            "union U { a: u32 } fn main() { let u = U { a: 1 }; let _ = U { ..u }; }",
            "exactly one field",
        ),
        (
            "union U { a: u32 } fn main() { let u = U { a: 1 }; u.a = 2; }",
            "not declared as mutable",
        ),
        (
            "union U { a: u32 } fn main() { let u: U; u.a = 1; }",
            "isn't initialized",
        ),
        (
            "union U { a: u32 } fn main() { let mut u = U { a: 1 }; let _v = u; u.a = 1; }",
            "reinitializing moved unions through fields",
        ),
        (
            "union U { a: u32, b: i32 } fn main() { let mut u = U { a: 1 }; unsafe { let a = &mut u.a; let b = &mut u.b; *a = 2; *b = 3; } }",
            "more than once",
        ),
        (
            "union U { a: u32, b: i32 } fn main() { let mut u = U { a: 1 }; unsafe { let a = &u.a; u.b = 2; let _ = *a; } }",
            "borrowed",
        ),
        (
            "struct S(u32); union U { a: S } fn main() {}",
            "union fields must implement Copy",
        ),
        (
            "union U<T: Copy> { a: T } fn main() {}",
            "generic unions are not supported yet",
        ),
        (
            "#[repr(packed)] union U { a: u32 } fn main() {}",
            "repr(packed)",
        ),
        (
            "#[repr(align(16))] union U { a: u32 } fn main() {}",
            "union alignment hints are not supported yet",
        ),
        (
            "#[derive(Debug)] union U { a: u32 } fn main() {}",
            "this union derive",
        ),
        (
            "#[derive(Clone)] union U { a: u32 } fn main() {}",
            "union Clone without Copy",
        ),
        (
            "union U { a: u32 } fn main() { let u = U { a: 1 }; unsafe { let U { a } = u; } }",
            "union patterns are not supported yet",
        ),
        (
            "union U { a: u32 } const U0: U = U { a: 1 }; fn main() {}",
            "union constant evaluation is not supported yet",
        ),
        (
            "union U { a: u32, b: u64 } const X: u64 = unsafe { U { a: 1 }.b }; fn main() {}",
            "union constant evaluation is not supported yet",
        ),
        (
            "union U { a: u32 } const fn f() -> u32 { unsafe { U { a: 1 }.a } } const X: u32 = f(); fn main() {}",
            "union constant evaluation is not supported yet",
        ),
    ] {
        let found = errors(text);
        assert!(
            found.iter().any(|e| e.contains(expected)),
            "{text}\nexpected {expected:?}, found {found:?}"
        );
    }
}

#[test]
fn supported_aggregate_unions() {
    for text in [
        "#[derive(Copy, Clone)] struct S<const N: usize>([u8; N]); union U { a: S<4> } fn main() { let _ = U { a: S([1; 4]) }; }",
        "trait Tr { type Out; } struct Marker; impl Tr for Marker { type Out = [u8; 4]; } union U { a: <Marker as Tr>::Out } fn main() { let _ = U { a: [1; 4] }; }",
        "union U { a: [u8; 4], b: u32 } fn main() { let u = U { a: [1; 4] }; let _ = unsafe { u.b }; }",
        "#[derive(Copy, Clone)] struct S(u32); #[derive(Copy, Clone)] union U { a: S, b: u32 } fn main() { let u = U { a: S(1) }; let _ = u.clone(); let _ = unsafe { u.a.0 }; }",
        "#[derive(Copy, Clone)] struct S<T> { a: T } type A = S<[u8; 16]>; union U { a: A } fn main() { let u = U { a: S { a: [2; 16] } }; let _ = unsafe { u.a.a[3] }; }",
        "#[derive(Copy, Clone)] struct S(u32, u32); union U { a: S, b: S } fn main() { let mut u = U { a: S(1, 2) }; unsafe { let a = &mut u.a.0; let b = &mut u.a.1; *a = 3; *b = 4; } }",
        "#[derive(Copy, Clone)] struct S(u32); union U { a: S, #[cfg(any())] absent: Missing } fn main() { let _ = U { a: S(1) }; }",
    ] {
        let found = errors(text);
        assert!(found.is_empty(), "{text}: {found:?}");
    }
}

#[test]
fn aggregate_union_rejections() {
    for (text, expected) in [
        (
            "struct S(u32); impl Drop for S { fn drop(&mut self) {} } union U { a: S } fn main() {}",
            "union fields must implement Copy",
        ),
        (
            "struct S(u32); union U { a: [S; 2] } fn main() {}",
            "union fields must implement Copy",
        ),
        (
            "#[derive(Clone)] struct S(u32); union U { a: S } fn main() {}",
            "union fields must implement Copy",
        ),
        (
            "struct S(u32); #[derive(Copy, Clone)] struct W<T>(T); union U { a: W<S> } fn main() {}",
            "union fields must implement Copy",
        ),
        (
            "#[derive(Copy, Clone)] struct S(u32); impl Drop for S { fn drop(&mut self) {} } union U { a: S } fn main() {}",
            "destructor",
        ),
        ("union U { a: [u8] } fn main() {}", "unsized fields"),
        (
            "struct S([u8]); union U { a: S } fn main() {}",
            "unsized fields",
        ),
        (
            "#[derive(Copy, Clone)] struct S(&'static u32); union U { a: S } fn main() {}",
            "this union field type is not supported yet",
        ),
        (
            "union U { a: (u32, u32) } fn main() {}",
            "this union field type is not supported yet",
        ),
        (
            "union U { a: core::mem::ManuallyDrop<u32> } fn main() {}",
            "this union field type is not supported yet",
        ),
        (
            "#[derive(Copy, Clone)] struct S(u32); union U { a: S } fn main() { let u = U { a: S(1) }; let _ = u.a.0; }",
            "access to union field is unsafe",
        ),
        (
            "union U { a: [u8; 4] } fn main() { let u = U { a: [1; 4] }; let _ = &u.a[0]; }",
            "access to union field is unsafe",
        ),
        (
            "#[derive(Copy, Clone)] struct S(u32, u32); union U { a: S, b: S } fn main() { let mut u = U { a: S(1, 2) }; unsafe { let a = &mut u.a.0; let b = &mut u.b.1; *a = 3; *b = 4; } }",
            "more than once",
        ),
        (
            "#[derive(Copy, Clone)] struct S(u32, u32); union U { a: S, b: S } fn main() { let mut u = U { a: S(1, 2) }; unsafe { let r = &mut u; let a = &mut r.a.0; let b = &mut r.b.1; *a = 3; *b = 4; } }",
            "more than once",
        ),
        (
            "#[derive(Copy, Clone)] struct S(u32, u32); union U { a: S, b: S } fn main() { let mut u = U { a: S(1, 2) }; unsafe { let a = &u.a.0; let mut f = || { u.b.1 = 4; }; f(); let _ = *a; } }",
            "borrow",
        ),
        (
            "#[derive(Copy, Clone)] struct S([u8; 4]); union U { a: S, b: [u8; 4] } fn main() { let mut u = U { b: [1; 4] }; unsafe { let a = &u.a.0[0]; let b = &mut u.b[1]; *b = 2; let _ = *a; } }",
            "borrow",
        ),
    ] {
        let found = errors(text);
        assert!(
            found.iter().any(|e| e.contains(expected)),
            "{text}: expected {expected:?}, found {found:?}"
        );
    }
}

#[test]
fn concrete_copy_union_fields_match_rustc_boundary() {
    // Check both compilers: valid Rust outside the supported representation
    // subset must remain an explicit rejection, not silently lose regions.
    let cases = [
        (
            "union U { empty: (), flag: bool } fn main() { let u = U { flag: true }; let _ = unsafe { u.flag }; }",
            true,
            None,
        ),
        (
            "#[derive(Copy, Clone)] enum E { A, B { flag: bool, n: usize } } union U { e: E } fn main() { let _ = U { e: E::B { flag: true, n: 3 } }; }",
            true,
            None,
        ),
        (
            "#[derive(Copy, Clone)] union Inner { flag: bool, byte: u8 } #[derive(Copy, Clone)] struct S { kind: Inner } union U { s: S } fn main() {}",
            true,
            None,
        ),
        (
            "union U { call: unsafe fn(&[u8], &mut usize) -> Option<usize> } fn main() {}",
            true,
            None,
        ),
        (
            "#[derive(Copy, Clone)] enum E<T> { A, B(T) } union U { e: E<[bool; 4]> } fn main() {}",
            true,
            None,
        ),
        (
            "enum E { A, B(bool) } union U { e: E } fn main() {}",
            false,
            Some("union fields must implement Copy"),
        ),
        (
            "enum E { A, B(bool) } impl Drop for E { fn drop(&mut self) {} } union U { e: E } fn main() {}",
            false,
            Some("union fields must implement Copy"),
        ),
        (
            "union Inner { byte: u8 } union U { inner: Inner } fn main() {}",
            false,
            Some("union fields must implement Copy"),
        ),
        (
            "#[derive(Copy, Clone)] enum E<T> { A, B(T) } struct S(u8); union U { e: E<S> } fn main() {}",
            false,
            Some("union fields must implement Copy"),
        ),
        (
            "union U<T: Copy> { field: T } fn main() {}",
            true,
            Some("generic unions are not supported yet"),
        ),
        (
            "union U { r: &'static u8 } fn main() {}",
            true,
            Some("this union field type is not supported yet"),
        ),
        (
            "#[derive(Copy, Clone)] enum E { A, B(&'static u8) } union U { e: E } fn main() {}",
            true,
            Some("this union field type is not supported yet"),
        ),
        (
            "#[derive(Copy, Clone)] struct S(&'static u8); #[derive(Copy, Clone)] union Inner { s: S } union U { inner: Inner } fn main() {}",
            true,
            Some("this union field type is not supported yet"),
        ),
        (
            "union U { marker: core::marker::PhantomData<&'static u8> } fn main() {}",
            true,
            Some("this union field type is not supported yet"),
        ),
        (
            "union U { pair: (u32, bool) } fn main() {}",
            true,
            Some("this union field type is not supported yet"),
        ),
        (
            "union U { pointer: *const u8 } fn main() {}",
            true,
            Some("this union field type is not supported yet"),
        ),
        (
            "union U { call: fn() -> &'static u8 } fn main() {}",
            true,
            Some("lifetimes in function pointer types are not supported yet"),
        ),
        (
            "union U { a: bool, b: u8 } fn main() { let mut u = U { a: true }; unsafe { let a = &u.a; u.b = 1; let _ = *a; } }",
            false,
            Some("borrowed"),
        ),
    ];
    let dir = std::env::temp_dir().join(format!("flux-union-boundary-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (i, (text, rustc_accepts, expected)) in cases.iter().enumerate() {
        let source = dir.join(format!("case_{i}.rs"));
        std::fs::write(&source, text).unwrap();
        let rustc = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata", "-Awarnings"])
            .arg(&source)
            .arg("--out-dir")
            .arg(&dir)
            .output()
            .unwrap();
        assert_eq!(
            rustc.status.success(),
            *rustc_accepts,
            "{text}: {}",
            String::from_utf8_lossy(&rustc.stderr)
        );
        let found = errors(text);
        match expected {
            None => assert!(found.is_empty(), "{text}: {found:?}"),
            Some(expected) => assert!(
                found.iter().any(|e| e.contains(expected)),
                "{text}: expected {expected:?}, found {found:?}"
            ),
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}

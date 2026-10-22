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
            "union U { a: bool } fn main() {}",
            "non-integer union fields are not supported yet",
        ),
        (
            "union U { a: [u8; 4] } fn main() {}",
            "non-integer union fields are not supported yet",
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

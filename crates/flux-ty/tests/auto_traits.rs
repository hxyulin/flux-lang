//! Compare the bounded built-in auto-trait rules directly with stable rustc.
use std::process::Command;

#[test]
fn send_sync_match_rustc() {
    let cases = [
        (
            "scalars",
            "fn main() { send::<(u8, bool, char, [usize; 2])>(); sync::<str>(); let _ = take(1); } fn take<T: Send>(x: T) -> T { x }",
            true,
        ),
        (
            "closure_send",
            "fn take<T: Send>(_: T) {} fn main() { let x = 1u8; take(move || x); }",
            true,
        ),
        (
            "closure_raw",
            "fn take<T: Send>(_: T) {} fn main() { let p = core::ptr::null::<u8>(); take(move || p); }",
            false,
        ),
        (
            "closure_cell",
            "fn take<T: Send>(_: T) {} fn main() { let c = core::cell::Cell::new(1u8); take(|| c.get()); }",
            false,
        ),
        (
            "generic_fields",
            "struct W<T>(T); fn f<T: Send + Sync>() { send::<W<T>>(); sync::<W<T>>(); send::<&W<T>>(); } fn main() { f::<u8>(); }",
            true,
        ),
        (
            "generic_missing",
            "struct W<T>(T); fn f<T>() { send::<W<T>>(); } fn main() {}",
            false,
        ),
        (
            "shared_reference",
            "fn f<T: Sync>() { send::<&T>(); sync::<&T>(); sync::<&mut T>(); } fn main() { f::<u8>(); }",
            true,
        ),
        (
            "shared_send_bound",
            "fn f<T: Send>() { send::<&T>(); } fn main() {}",
            false,
        ),
        (
            "mutable_reference",
            "fn f<T: Send>() { send::<&mut T>(); } fn main() { f::<core::cell::Cell<u8>>(); }",
            true,
        ),
        ("raw_const", "fn main() { send::<*const u8>(); }", false),
        ("raw_mut", "fn main() { sync::<*mut u8>(); }", false),
        (
            "nested_raw",
            "struct W<T>(T); enum E<T> { A, B(W<T>) } fn main() { send::<E<*const u8>>(); }",
            false,
        ),
        (
            "enum",
            "enum E<T> { A, B(T) } fn f<T: Send>() { send::<E<T>>(); } fn main() { f::<u8>(); }",
            true,
        ),
        (
            "phantom",
            "fn f<T: Send>() { send::<core::marker::PhantomData<T>>(); } fn main() { f::<u8>(); }",
            true,
        ),
        (
            "phantom_raw",
            "fn main() { sync::<core::marker::PhantomData<*const u8>>(); }",
            false,
        ),
        (
            "cell_send",
            "fn main() { send::<core::cell::Cell<u8>>(); send::<core::cell::RefCell<u8>>(); }",
            true,
        ),
        (
            "cell_sync",
            "fn main() { sync::<core::cell::Cell<u8>>(); }",
            false,
        ),
        (
            "refcell_sync",
            "fn main() { sync::<core::cell::RefCell<u8>>(); }",
            false,
        ),
        (
            "cell_shared",
            "fn main() { send::<&core::cell::Cell<u8>>(); }",
            false,
        ),
        (
            "nonnull",
            "fn main() { send::<core::ptr::NonNull<u8>>(); }",
            false,
        ),
        (
            "unsafe_impl",
            "struct W<T>(*const T); unsafe impl<T: Sync> Send for W<T> {} unsafe impl<T: Sync> Sync for W<T> {} fn f<T: Sync>() { send::<W<T>>(); sync::<W<T>>(); } fn main() { f::<u8>(); }",
            true,
        ),
        (
            "impl_replaces_structure",
            "struct W<T>(T); unsafe impl<T: Sync> Send for W<T> {} fn main() { send::<W<core::cell::Cell<u8>>>(); }",
            false,
        ),
        (
            "safe_impl",
            "struct W(*const u8); impl Send for W {} fn main() {}",
            false,
        ),
        (
            "overlap",
            "struct W; unsafe impl Send for W {} unsafe impl Send for W {} fn main() {}",
            false,
        ),
        (
            "orphan",
            "unsafe impl Send for *const u8 {} fn main() {}",
            false,
        ),
        (
            "specialized",
            "struct W<T>(T); unsafe impl Send for W<u8> {} fn main() { send::<W<u8>>(); }",
            true,
        ),
        (
            "lifetime_specialized",
            "struct W<'a>(&'a u8); unsafe impl Send for W<'static> {} fn main() {}",
            true,
        ),
        (
            "nested_lifetime_specialized",
            "struct W<T>(T); unsafe impl Send for W<&'static u8> {} fn main() {}",
            true,
        ),
        (
            "specialized_disables_structure",
            "struct W<T>(T); unsafe impl Send for W<u8> {} fn main() { send::<W<u16>>(); }",
            false,
        ),
        (
            "static_cell",
            "static VALUE: core::cell::Cell<u8> = core::cell::Cell::new(1); fn main() {}",
            false,
        ),
        (
            "static_unsafe_impl",
            "struct W(*const u8); unsafe impl Sync for W {} static VALUE: W = W(core::ptr::null()); fn main() {}",
            true,
        ),
        (
            "foreign_reference",
            "struct W; unsafe impl Send for &W {} fn main() {}",
            false,
        ),
        (
            "ordinary_named_send",
            "trait Send {} struct W; fn f<T: Send>() {} fn main() { f::<W>(); }",
            false,
        ),
        (
            "memchr_iter",
            "struct Iter<'h> { start: *const u8, end: *const u8, haystack: core::marker::PhantomData<&'h [u8]> } unsafe impl<'h> Send for Iter<'h> {} unsafe impl<'h> Sync for Iter<'h> {} fn main() { send::<Iter>(); sync::<Iter>(); }",
            true,
        ),
    ];
    let dir = std::env::temp_dir().join(format!("flux-auto-traits-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, body, expected) in cases {
        let text = format!(
            "#![allow(dead_code)]\nfn send<T: Send + ?Sized>() {{}} fn sync<T: Sync + ?Sized>() {{}}\n{body}\n"
        );
        let path = dir.join(format!("{name}.rs"));
        std::fs::write(&path, &text).unwrap();
        let rustc = Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata", "-o"])
            .arg(dir.join(format!("{name}.rmeta")))
            .arg(&path)
            .output()
            .unwrap();
        assert_eq!(
            rustc.status.success(),
            expected,
            "{name}: rustc: {}",
            String::from_utf8_lossy(&rustc.stderr)
        );
        let (db, root, hir) = flux_hir::lower_text(&path, &text);
        let typed = flux_ty::check_file(&hir);
        let accepted = db.file(root).parse.errors().is_empty()
            && hir.diagnostics.is_empty()
            && typed.diagnostics.is_empty();
        // Lifetime specialization is deliberately rejected until trait
        // selection carries region obligations through both solvers.
        if matches!(name, "lifetime_specialized" | "nested_lifetime_specialized") {
            assert!(
                typed
                    .diagnostics
                    .iter()
                    .any(|d| d.message.contains("not supported yet"))
            );
        }
        assert_eq!(
            accepted,
            expected && !matches!(name, "lifetime_specialized" | "nested_lifetime_specialized"),
            "{name}: flux: {:?}\n{:?}\n{:?}",
            db.file(root).parse.errors(),
            hir.diagnostics,
            typed.diagnostics
        );
    }
    std::fs::remove_dir_all(dir).unwrap();
}

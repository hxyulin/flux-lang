//! The std facade must name the original library definitions, including
//! across independent compilations that reuse cached signatures and bodies.
use flux_hir::def_map::Ns;
use flux_intern::Symbol;

const IMPORTS: &str = r#"
extern crate alloc;
extern crate std;
use std::fmt::Debug as sf;
use core::fmt::Debug as cf;
use std::iter::FusedIterator as SI;
use core::iter::FusedIterator as CI;
use std::ops::Index as SX;
use core::ops::Index as CX;
use std::vec::Vec as SV;
use alloc::vec::Vec as AV;
use std::string::String as SS;
use alloc::string::String as AS;
use std::boxed::Box as SB;
use alloc::boxed::Box as AB;
use std::option::Option as SO;
use core::option::Option as CO;
use std::num::NonZeroU32 as SN;
use core::num::NonZeroU32 as CN;
use std::sync::atomic::AtomicUsize as SA;
use core::sync::atomic::AtomicUsize as CA;
use std::sync::atomic::AtomicU32 as SA32;
use core::sync::atomic::AtomicU32 as CA32;
use std::sync::atomic::Ordering as SOrder;
use core::sync::atomic::Ordering as COrder;
use std::sync::Arc as SArc;
use alloc::sync::Arc as AArc;
use std::sync::Weak as SWeak;
use alloc::sync::Weak as AWeak;
use ::std::mem as sm;
use core::mem as cm;
fn fused<I: SI>(i: I) -> I { i }
fn indexed<T: SX<usize, Output = i32>>(v: &T) -> i32 { *v.index(0) }
fn exchange(v: SV<i32>) -> AV<i32> { v }
fn text(s: SS) -> AS { s }
fn boxed(b: SB<i32>) -> AB<i32> { b }
fn main() {
    let v: Vec<i32> = exchange(vec![3, 5]);
    let s: String = text(String::from("std"));
    let b: Box<i32> = boxed(Box::new(indexed(&v)));
    let mut iter = fused(v.into_iter());
    let first: SO<i32> = iter.next();
    let _: CO<i32> = first;
    let _: CN = SN::new(3).unwrap();
    let _ = std::fmt::format(format_args!("{} {}", s, b));
    let _ = std::thread::panicking();
    let a: CA = SA::new(1);
    let order: COrder = SOrder::Relaxed;
    let _ = a.load(order);
    let a32: CA32 = SA32::new(1);
    let _ = a32.fetch_add(1, order);
    let _ = a32.load(order);
    std::sync::atomic::fence(SOrder::Acquire);
}
"#;

#[test]
fn reexports_and_cached_library_ids() {
    let mut previous = None;
    // Different program file counts and declarations must not enter the
    // process-wide library cache or shift any embedded file/item IDs.
    for count in [0, 3, 1] {
        let mut db = flux_hir::SourceDb::new();
        for n in 0..count {
            db.add(format!("unused{n}.rs"), "struct Unrelated;");
        }
        let declarations: String = (0..count)
            .map(|n| format!("struct Program{n};\n"))
            .collect();
        let root = db.add("main.rs", format!("{declarations}{IMPORTS}"));
        let hir = flux_hir::lower(&mut db, root, Symbol::intern("main"));
        assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
        let typed = flux_ty::check_file(&hir);
        assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
        let items = &hir.items;
        assert_eq!(items.main_crate().0, 3);
        assert_eq!(items.crates[2].name.as_str(), "std");
        assert!(!items.crates[2].is_core);
        for krate in &items.crates[..2] {
            assert!(
                items
                    .lookup(krate.root, Symbol::intern("std"), Ns::Types)
                    .is_none()
            );
        }
        let module = items.module_of(items.main_crate());
        let mut identities = Vec::new();
        for (a, b) in [
            ("sf", "cf"),
            ("SI", "CI"),
            ("SX", "CX"),
            ("SV", "AV"),
            ("SS", "AS"),
            ("SB", "AB"),
            ("SO", "CO"),
            ("SN", "CN"),
            ("sm", "cm"),
            ("SA", "CA"),
            ("SA32", "CA32"),
            ("SOrder", "COrder"),
            ("SArc", "AArc"),
            ("SWeak", "AWeak"),
        ] {
            let a = items.lookup(module, Symbol::intern(a), Ns::Types).unwrap();
            let b = items.lookup(module, Symbol::intern(b), Ns::Types).unwrap();
            assert_eq!(a, b);
            identities.push(a);
        }
        let file = db.shared(flux_hir::FileId::std_root());
        assert_eq!(file.path.to_str(), Some("<std>/lib.rs"));
        assert!(db.exists(std::path::Path::new("<std>/thread.rs")));
        assert!(db.load(std::path::Path::new("<std>/absent.rs")).is_err());
        if let Some((ids, previous_file)) = &previous {
            assert_eq!(&identities, ids);
            assert!(std::sync::Arc::ptr_eq(&file, previous_file));
        }
        previous = Some((identities, file));
    }
}

#[test]
fn facade_keeps_library_privacy_and_alloc_opt_in() {
    for source in [
        "use std::intrinsics; fn main() {}",
        "use std::fmt::flags; fn main() {}",
        "use alloc::vec::Vec; fn main() {}",
        "use std::fs; fn main() {}",
        "use std::sync::atomic::AtomicU64; fn main() {}",
        "use std::thread::spawn; fn main() {}",
        "use std::num::NonZeroU64; fn main() {}",
        "use std::vec::RawVec; fn main() {}",
        "use core::unicode; fn main() {}",
    ] {
        let (_, _, hir) = flux_hir::lower_text("main.rs", source);
        assert!(!hir.diagnostics.is_empty(), "accepted {source}");
    }
    let (_, _, bare) = flux_hir::lower_text_bare("bare.rs", "use std::fmt; fn main() {}");
    assert_eq!(bare.items.crates.len(), 1);
    assert!(!bare.diagnostics.is_empty());
    let mut db = flux_hir::SourceDb::new();
    let root = db.add("core_only.rs", "use std::fmt; fn main() {}");
    let items = flux_hir::ItemTree::new(&mut db, root, Symbol::intern("bare"), false);
    assert_eq!(items.crates.len(), 1);
    assert!(!items.diagnostics.is_empty());
}

#[test]
fn arbitrary_iterators_are_not_marked_fused() {
    let (_, _, hir) = flux_hir::lower_text(
        "main.rs",
        r#"
        struct Resuming;
        impl Iterator for Resuming {
            type Item = i32;
            fn next(&mut self) -> Option<i32> { None }
        }
        fn require<I: std::iter::FusedIterator>(_: I) {}
        fn main() { require(Resuming); }
    "#,
    );
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let typed = flux_ty::check_file(&hir);
    assert!(
        typed
            .diagnostics
            .iter()
            .any(|d| d.message.contains("FusedIterator")),
        "{:?}",
        typed.diagnostics
    );
}

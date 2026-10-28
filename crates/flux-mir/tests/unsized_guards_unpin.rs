//! Unsized guards preserve borrowing, and Unpin follows ownership boundaries.
use std::process::Command;

#[test]
fn unsized_guards_and_unpin_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-unsized-markers-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let cases = [
        (
            "plain_unpin",
            r#"fn need<T:Unpin>(_:T){}fn main(){need(3u32);}"#,
            true,
        ),
        (
            "pinned_marker",
            r#"use std::marker::PhantomPinned;fn need<T:Unpin>(_:T){}fn main(){need(PhantomPinned);}"#,
            false,
        ),
        (
            "nested_marker",
            r#"use std::marker::PhantomPinned;struct S(PhantomPinned);fn need<T:Unpin>(_:T){}fn main(){need(S(PhantomPinned));}"#,
            false,
        ),
        (
            "marker_refs",
            r#"use std::marker::PhantomPinned;fn need<T:Unpin>(_:T){}fn main(){let mut x=PhantomPinned;need(&x);need(&mut x);need(&x as *const _);}"#,
            true,
        ),
        (
            "heap_owners",
            r#"use std::marker::PhantomPinned;fn need<T:Unpin>(_:T){}fn main(){need(Box::new(PhantomPinned));need(std::rc::Rc::new(PhantomPinned));let a=std::sync::Arc::new(PhantomPinned);need(std::sync::Arc::downgrade(&a));need(a);}"#,
            true,
        ),
        (
            "manual_unpin",
            r#"use std::marker::PhantomPinned;struct S(PhantomPinned);impl Unpin for S{}fn need<T:Unpin>(_:T){}fn main(){need(S(PhantomPinned));}"#,
            true,
        ),
        (
            "marker_send_sync",
            r#"use std::marker::PhantomPinned;fn need<T:Send+Sync>(_:T){}fn main(){need(PhantomPinned);}"#,
            true,
        ),
        (
            "phantom_ownership",
            r#"use std::marker::{PhantomData,PhantomPinned};fn need<T:Unpin>(_:T){}fn main(){need(PhantomData::<PhantomPinned>);}"#,
            false,
        ),
        (
            "unsized_refcell_escape",
            r#"fn bad()->std::cell::Ref<'static,[u32]>{let x=std::cell::RefCell::new([1,2]);let v:&std::cell::RefCell<[u32]>=&x;v.borrow()}fn main(){}"#,
            false,
        ),
        (
            "unsized_guard_not_send",
            r#"fn need<T:Send>(_:T){}fn main(){let x=std::sync::Mutex::new([1u32,2]);let v:&std::sync::Mutex<[u32]>=&x;need(v.lock().unwrap());}"#,
            false,
        ),
        (
            "vec_ownership",
            "use std::marker::PhantomPinned;fn need<T:Unpin>(_:T){}fn main(){need(vec![PhantomPinned]);}",
            false,
        ),
        (
            "constructor_prefix",
            "struct W<P,T:?Sized>{p:P,t:T}fn make<P,T>(p:P,t:T)->W<P,T>{W{p,t}}fn main(){let x:&W<_,[u8]>=&make(7u32,[1u8,2]);assert_eq!(x.p,7);}",
            true,
        ),
        (
            "constructor_bad_prefix",
            "struct W<P,T:?Sized>{p:P,t:T}fn make<P,T>(p:P,t:T)->W<P,T>{W{p,t}}fn main(){let _x:&W<u32,[u8]>=&make(true,[1u8,2]);}",
            false,
        ),
    ];
    for (name, source, accepts) in cases {
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let rustc = Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("oracle.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            rustc.status.success(),
            accepts,
            "rustc {name}: {}",
            String::from_utf8_lossy(&rustc.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text(&path, source);
        assert!(hir.diagnostics.is_empty(), "{name}: {:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        if !typed.diagnostics.is_empty() {
            assert!(!accepts, "{name}: {:?}", typed.diagnostics);
            continue;
        }
        let mir = flux_mir::build_file(&hir, &typed);
        assert_eq!(
            typed.diagnostics.is_empty() && mir.diagnostics.is_empty(),
            accepts,
            "{name}: types={:?}, MIR={:?}",
            typed.diagnostics,
            mir.diagnostics
        );
    }
    std::fs::remove_dir_all(root).unwrap();
}

//! Owned and shared unwind contracts are different structural auto traits.
use std::process::Command;

#[test]
fn unwind_safety_matches_rustc() {
    let root = std::env::temp_dir().join(format!("flux-unwind-contracts-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let prefix = "use std::panic::{UnwindSafe,RefUnwindSafe,AssertUnwindSafe};use std::cell::{Cell,RefCell,UnsafeCell};fn owned<T:UnwindSafe+?Sized>(){}fn shared<T:RefUnwindSafe+?Sized>(){}";
    for (name, body) in [
        ("scalars", "owned::<(u32,[u8;3],f64)>();shared::<str>();"),
        (
            "cell_owned",
            "owned::<Cell<u8>>();owned::<RefCell<u8>>();owned::<UnsafeCell<u8>>();",
        ),
        ("cell_shared", "shared::<Cell<u8>>();"),
        ("refcell_shared", "shared::<RefCell<u8>>();"),
        ("unsafe_shared", "shared::<UnsafeCell<u8>>();"),
        (
            "shared_reference",
            "owned::<&u8>();shared::<&u8>();shared::<&mut u8>();",
        ),
        ("mutable_reference", "owned::<&mut u8>();"),
        ("borrowed_cell", "owned::<&Cell<u8>>();"),
        (
            "raw_scalar",
            "owned::<*const u8>();owned::<*mut u8>();shared::<*const u8>();",
        ),
        ("raw_cell_owned", "owned::<*const Cell<u8>>();"),
        ("raw_cell_shared", "shared::<*const Cell<u8>>();"),
        (
            "atomic",
            "shared::<std::sync::atomic::AtomicUsize>();shared::<std::sync::atomic::AtomicPtr<Cell<u8>>>();",
        ),
        ("owned_box", "owned::<Box<RefCell<u8>>>();"),
        ("shared_box", "shared::<Box<RefCell<u8>>>();"),
        (
            "owned_vec",
            "owned::<Vec<RefCell<u8>>>();owned::<std::vec::IntoIter<Cell<u8>>>();",
        ),
        ("shared_vec", "shared::<Vec<Cell<u8>>>();"),
        ("deque", "owned::<std::collections::VecDeque<Cell<u8>>>();"),
        (
            "rc_scalar",
            "owned::<std::rc::Rc<u8>>();shared::<std::rc::Rc<u8>>();",
        ),
        ("rc_cell", "owned::<std::rc::Rc<Cell<u8>>>();"),
        (
            "mutex",
            "owned::<std::sync::Mutex<Cell<u8>>>();shared::<std::sync::Mutex<Cell<u8>>>();",
        ),
        (
            "rwlock",
            "owned::<std::sync::RwLock<Cell<u8>>>();shared::<std::sync::RwLock<Cell<u8>>>();",
        ),
        ("once_owned", "owned::<std::sync::OnceLock<Cell<u8>>>();"),
        ("once_shared", "shared::<std::sync::OnceLock<Cell<u8>>>();"),
        ("once_scalar", "shared::<std::sync::OnceLock<u8>>();"),
        ("once_mutref", "shared::<std::sync::OnceLock<&mut u8>>();"),
        ("lazy_owned", "owned::<std::sync::LazyLock<Cell<u8>>>();"),
        ("lazy_shared", "shared::<std::sync::LazyLock<u8>>();"),
        ("lazy_cell", "shared::<std::sync::LazyLock<Cell<u8>>>();"),
        (
            "explicit_assertion",
            "owned::<AssertUnwindSafe<&mut u8>>();shared::<AssertUnwindSafe<Cell<u8>>>();let mut v=AssertUnwindSafe(7u8);*v+=1;assert_eq!(*v,8);",
        ),
        (
            "closure_owned",
            "fn take<F:UnwindSafe>(_:F){}let x=Cell::new(7u8);take(move||x.get());",
        ),
        (
            "closure_shared",
            "fn take<F:UnwindSafe>(_:F){}let x=Cell::new(7u8);take(||x.get());",
        ),
        (
            "closure_mut",
            "fn take<F:UnwindSafe>(_:F){}let mut x=7;take(||{x+=1;});",
        ),
        (
            "asserted_closure",
            "fn take<F:FnOnce()->u8+UnwindSafe>(f:F)->u8{f()}let mut x=7;assert_eq!(take(AssertUnwindSafe(||{x+=1;x})),8);",
        ),
        (
            "recursive",
            "struct Node{next:Option<Box<Node>>,value:u8}owned::<Node>();shared::<Node>();",
        ),
        (
            "recursive_negative",
            "struct Node{next:Option<Box<Node>>,value:Cell<u8>}shared::<Node>();",
        ),
    ] {
        let source = format!("{prefix}fn main(){{{body}}}");
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, &source).unwrap();
        let oracle = Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join(format!("{name}.rmeta")))
            .output()
            .unwrap();
        let (_, _, hir) = flux_hir::lower_text(&path, &source);
        let typed = flux_ty::check_file(&hir);
        let diagnostics: Vec<_> = hir.diagnostics.iter().chain(&typed.diagnostics).collect();
        assert_eq!(
            diagnostics.is_empty(),
            oracle.status.success(),
            "{name}: rustc={} flux={diagnostics:?}",
            String::from_utf8_lossy(&oracle.stderr)
        );
    }
    std::fs::remove_dir_all(root).unwrap();
}

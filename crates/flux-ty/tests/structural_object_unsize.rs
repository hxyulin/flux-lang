//! Object unsizing changes the structural tail, never an unrelated prefix.
#[test]
fn structural_object_coercions_and_auto_traits_match_rustc() {
    let prelude = "use std::{sync::{Arc,Mutex},rc::Rc,ptr::NonNull};trait Value{fn value(&self)->u8;}impl Value for u8{fn value(&self)->u8{*self}}struct Wrap<H,T:?Sized>{head:H,tail:T}trait Signal:Send+Sync{}impl Signal for u8{}fn send<T:Send+?Sized>(){}fn sync<T:Sync+?Sized>(){}";
    for (name, body, accepted) in [
        (
            "owners",
            "fn f(){let _:Box<Wrap<u8,dyn Value>>=Box::new(Wrap{head:1,tail:2u8});let _:Arc<Wrap<u8,dyn Value>>=Arc::new(Wrap{head:1,tail:2u8});let _:Rc<Wrap<u8,dyn Value>>=Rc::new(Wrap{head:1,tail:2u8});}",
            true,
        ),
        (
            "nonnull",
            "fn f(x:NonNull<Wrap<u8,u8>>)->NonNull<Wrap<u8,dyn Value>>{x}",
            true,
        ),
        (
            "nested",
            "fn f(x:&Wrap<u8,Wrap<u16,u8>>)->&Wrap<u8,Wrap<u16,dyn Value>>{x}",
            true,
        ),
        (
            "mutable",
            "fn f(x:&mut Wrap<u8,u8>)->&mut Wrap<u8,dyn Value>{x}",
            true,
        ),
        (
            "raw_cast",
            "fn f(x:*mut Wrap<u8,u8>)->*const Wrap<u8,dyn Value>{x as *const Wrap<u8,dyn Value>}",
            true,
        ),
        (
            "prefix_type",
            "fn f(x:&Wrap<u8,u8>)->&Wrap<u16,dyn Value>{x}",
            false,
        ),
        (
            "nonnull_prefix_type",
            "fn f(x:NonNull<Wrap<u8,u8>>)->NonNull<Wrap<u16,dyn Value>>{x}",
            false,
        ),
        (
            "auto_trait_strengthening",
            "fn f(x:&Wrap<u8,dyn Value>)->&Wrap<u8,dyn Value+Send>{x}",
            false,
        ),
        (
            "tail_not_sized",
            "impl Value for [u8]{fn value(&self)->u8{0}}fn f(x:&Wrap<u8,[u8]>)->&Wrap<u8,dyn Value>{x}",
            false,
        ),
        (
            "mutex_send_sync",
            "fn f(){send::<Box<Mutex<dyn std::io::Write+Send>>>();sync::<Box<Mutex<dyn std::io::Write+Send>>>();}",
            true,
        ),
        (
            "signal_supertraits",
            "fn f(){send::<Arc<Wrap<u8,dyn Signal>>>();sync::<Arc<Wrap<u8,dyn Signal>>>();}",
            true,
        ),
        (
            "missing_send",
            "fn f(){send::<Box<Mutex<dyn std::io::Write>>>();}",
            false,
        ),
        (
            "arc_missing_sync",
            "fn f(){send::<Arc<Wrap<u8,dyn Value+Send>>>();}",
            false,
        ),
    ] {
        let source = format!("{prelude}{body}fn main(){{}}");
        let root = std::env::temp_dir().join(format!(
            "flux-structural-object-{name}-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("case.rs");
        std::fs::write(&path, &source).unwrap();
        let reference = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata", "--out-dir"])
            .arg(&root)
            .arg(&path)
            .output()
            .unwrap();
        assert_eq!(
            reference.status.success(),
            accepted,
            "rustc {name}: {}",
            String::from_utf8_lossy(&reference.stderr)
        );
        let (db, _, hir) = flux_hir::lower_text(&path, &source);
        let typed = flux_ty::check_file(&hir);
        let success = db.syntax_errors().is_empty()
            && hir.diagnostics.is_empty()
            && typed.diagnostics.is_empty();
        assert_eq!(
            success,
            accepted,
            "flux {name}: syntax {:?}, hir {:?}, types {:?}",
            db.syntax_errors(),
            hir.diagnostics,
            typed.diagnostics
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}

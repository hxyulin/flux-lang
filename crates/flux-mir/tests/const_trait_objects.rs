//! Const trait-object construction keeps ordinary object, safety and borrow rules.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod cases;
#[test]
fn const_trait_object_contracts_match_rustc() {
    let root =
        std::env::temp_dir().join(format!("flux-const-trait-objects-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "promoted",
            "trait P{fn v(&self)->u32;}impl P for u32{fn v(&self)->u32{*self}}const P:&dyn P=&7u32;fn main(){assert_eq!(P.v(),7);}",
            true,
        ),
        (
            "static",
            "trait P:Sync{}impl P for u32{}static X:u32=7;static P:&dyn P=&X;fn main(){}",
            true,
        ),
        (
            "const_fn",
            "trait P{}impl P for u32{}const fn wrap(x:&u32)->&dyn P{x}const P:&dyn P=wrap(&7u32);fn main(){}",
            true,
        ),
        (
            "generic",
            "trait P{}impl P for u32{}const fn wrap<T:P>(x:&T)->&dyn P{x}const P:&dyn P=wrap(&7u32);fn main(){}",
            true,
        ),
        (
            "reborrow",
            "trait P{}impl P for u32{}const A:&dyn P=&7u32;const B:&dyn P=&*A;fn main(){}",
            true,
        ),
        (
            "indexed_const_slice",
            "trait P{fn run(&self)->u32;}struct B{run:fn()->u32}fn f()->u32{7}impl P for B{fn run(&self)->u32{(self.run)()}}const B:&[B]=&[B{run:f},B{run:f}];const P:&[&dyn P]=&[&B[0],&B[1]];fn main(){}",
            true,
        ),
        (
            "offset_reference",
            "trait P{}impl P for u32{}static A:[u32;2]=[7,9];const P:&dyn P=unsafe{&*A.as_ptr().add(1)};fn main(){}",
            true,
        ),
        (
            "raw_null_object",
            "trait P{}impl P for u32{}const P:*const dyn P=core::ptr::null::<u32>();fn main(){}",
            true,
        ),
        (
            "raw_one_past_object",
            "trait P{}impl P for u32{}static A:[u32;2]=[7,9];const P:*const dyn P=unsafe{A.as_ptr().add(2)};fn main(){}",
            true,
        ),
        (
            "null_object_reference",
            "trait P{}impl P for u32{}const P:&dyn P=unsafe{&*core::ptr::null::<u32>()};fn main(){}",
            false,
        ),
        (
            "one_past_object_reference",
            "trait P{}impl P for u32{}static A:[u32;2]=[7,9];const P:&dyn P=unsafe{&*A.as_ptr().add(2)};fn main(){}",
            false,
        ),
        (
            "trait_bound",
            "trait P{}const P:&dyn P=&7u32;fn main(){}",
            false,
        ),
        (
            "not_object_safe",
            "trait P{fn f<T>(&self);}impl P for u32{fn f<T>(&self){}}const P:&dyn P=&7u32;fn main(){}",
            false,
        ),
        (
            "associated_type",
            "trait P{type Out;}impl P for u32{type Out=u32;}const P:&dyn P<Out=u64>=&7u32;fn main(){}",
            false,
        ),
        (
            "local_escape",
            "trait P{}impl P for u32{}const P:&dyn P={let x=7u32;&x};fn main(){}",
            false,
        ),
        (
            "interior_temporary",
            "use std::cell::Cell;trait P{}impl P for Cell<u32>{}const P:&dyn P=&Cell::new(7);fn main(){}",
            false,
        ),
        (
            "static_not_sync",
            "use std::cell::Cell;trait P{}impl P for Cell<u32>{}static P:&dyn P=&Cell::new(7);fn main(){}",
            false,
        ),
        (
            "auto_trait",
            "use std::rc::Rc;trait P{}impl P for Rc<u32>{}const fn f(x:&Rc<u32>)->&(dyn P+Send){x}fn main(){}",
            false,
        ),
        (
            "virtual_const_call",
            "trait P{fn v(&self)->u32;}impl P for u32{fn v(&self)->u32{*self}}const V:u32={let p:&dyn P=&7u32;p.v()};fn main(){}",
            false,
        ),
    ] {
        if !cases::selected(name) {
            continue;
        }
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let oracle = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join(format!("{name}.rmeta")))
            .output()
            .unwrap();
        assert_eq!(
            oracle.status.success(),
            accepted,
            "{name}: {}",
            String::from_utf8_lossy(&oracle.stderr)
        );
        let (db, _, hir) = flux_hir::lower_text(&path, source);
        let mut errors = db.syntax_errors();
        errors.extend(hir.diagnostics.clone());
        if errors.is_empty() {
            let typed = flux_mir::check_file(&hir);
            errors.extend(typed.diagnostics.clone());
            if errors.is_empty() {
                errors.extend(flux_mir::build_file(&hir, &typed).diagnostics);
            }
        }
        assert_eq!(errors.is_empty(), accepted, "{name}: {errors:?}");
    }
}

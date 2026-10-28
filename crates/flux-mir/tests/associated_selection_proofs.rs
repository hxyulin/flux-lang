//! Nested associated selections prove inputs before normalizing their outer receiver.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;
use std::process::Command;

#[test]
fn associated_selection_proofs_match_rustc() {
    let dir = std::env::temp_dir().join(format!(
        "flux-associated-selection-proofs-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, accepted, source) in [
        (
            "nested_receiver",
            true,
            "trait First{type Out;}trait Second<T>{type Out;}trait Third{type Out;}trait State{type Out:Copy;}impl First for u32{type Out=u64;}impl Second<u16> for u64{type Out=u8;}impl Third for u16{type Out=<<u32 as First>::Out as Second<u16>>::Out;}impl State for u16{type Out=<u16 as Third>::Out;}fn main(){}",
        ),
        (
            "nested_argument",
            true,
            "trait First{type Out;}trait Second<T>{type Out;}trait Third{type Out;}trait State{type Out:Copy;}impl First for u32{type Out=u64;}impl Second<u16> for u64{type Out=u8;}impl Second<u64> for u16{type Out=u8;}impl Third for u16{type Out=<u16 as Second<<u32 as First>::Out>>::Out;}impl State for u16{type Out=<u16 as Third>::Out;}fn main(){}",
        ),
        (
            "nested_wrong_argument",
            false,
            "trait First{type Out;}trait Second<T>{type Out;}trait Third{type Out;}trait State{type Out:Copy;}impl First for u32{type Out=u64;}impl Second<u16> for u64{type Out=u8;}impl Third for u16{type Out=<<u32 as First>::Out as Second<u32>>::Out;}impl State for u16{type Out=<u16 as Third>::Out;}fn main(){}",
        ),
        (
            "nested_wrong_output_bound",
            false,
            "trait First{type Out;}trait Second<T>{type Out;}trait Third{type Out;}trait State{type Out:Copy;}impl First for u32{type Out=u64;}impl Second<u16> for u64{type Out=u8;}struct NotCopy;impl Second<u8> for u64{type Out=NotCopy;}impl Third for u16{type Out=<<u32 as First>::Out as Second<u8>>::Out;}impl State for u16{type Out=<u16 as Third>::Out;}fn main(){}",
        ),
        (
            "nested_static_header",
            true,
            "trait First{type Out;}trait Second{type Out;}trait Third{type Out;}trait State{type Out:Copy;}struct W<T>(T);impl<T:'static>First for W<T>{type Out=u64;}impl Second for u64{type Out=u8;}impl<T:'static>Third for W<T>{type Out=<<Self as First>::Out as Second>::Out;}impl<T:'static>State for W<T>{type Out=<Self as Third>::Out;}fn main(){}",
        ),
        (
            "nested_missing_static",
            false,
            "trait First{type Out;}trait Second{type Out;}trait Third{type Out;}trait State{type Out:Copy;}struct W<T>(T);impl<T:'static>First for W<T>{type Out=u64;}impl Second for u64{type Out=u8;}impl<T>Third for W<T>{type Out=<<Self as First>::Out as Second>::Out;}impl<T>State for W<T>{type Out=<Self as Third>::Out;}fn main(){}",
        ),
    ] {
        if !case_filter::selected(name) {
            continue;
        }
        let path = dir.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let rustc = Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata", "-Awarnings"])
            .arg(&path)
            .arg("-o")
            .arg(dir.join(format!("{name}.rmeta")))
            .output()
            .unwrap();
        assert_eq!(
            rustc.status.success(),
            accepted,
            "rustc {name}: {}",
            String::from_utf8_lossy(&rustc.stderr)
        );
        let (db, _, hir) = flux_hir::lower_text(&path, source);
        assert!(
            db.syntax_errors().is_empty(),
            "{name}: {:?}",
            db.syntax_errors()
        );
        assert!(hir.diagnostics.is_empty(), "{name}: {:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        let diagnostics = if typed.diagnostics.is_empty() {
            flux_mir::build_file(&hir, &typed).diagnostics
        } else {
            typed.diagnostics
        };
        assert_eq!(
            diagnostics.is_empty(),
            accepted,
            "Flux {name}: {diagnostics:?}"
        );
    }
    std::fs::remove_dir_all(dir).unwrap();
}

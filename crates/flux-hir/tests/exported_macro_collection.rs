//! Exported macros have crate-root item scope, separate from module names.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;
#[test]
fn exported_macro_collection_matches_rustc() {
    let root = std::env::temp_dir().join(format!("flux-exported-macros-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "later_reexport",
            "mod consumer { use crate::prelude::*; build!(Generated); } mod prelude { pub use crate::build; } #[macro_export] macro_rules! build { ($name:ident) => { pub struct $name; }; } fn main() { let _ = consumer::Generated; }",
            true,
        ),
        (
            "later_impl_reexport",
            "struct S; mod consumer { use crate::prelude::*; impl super::S { methods!(); } } mod prelude { pub use crate::methods; } #[macro_export] macro_rules! methods { () => { pub fn value() -> u32 { 42 } }; } fn main() { assert_eq!(S::value(),42); }",
            true,
        ),
        (
            "later_module",
            "mod debug{crate::entity!(DebugType);}mod entity{#[macro_export]macro_rules! entity{($name:ident)=>{pub struct $name;}}}fn main(){let _=debug::DebugType;}",
            true,
        ),
        (
            "root_unqualified_export",
            "entity!(Root);mod entity{#[macro_export]macro_rules! entity{($name:ident)=>{pub struct $name;}}}fn main(){let _=Root;}",
            true,
        ),
        (
            "self_root",
            "self::make!(Root);mod source{#[macro_export]macro_rules! make{($name:ident)=>{pub struct $name;}}}fn main(){let _=Root;}",
            true,
        ),
        (
            "generated_export",
            "entity!(DebugType);crate::factory!();mod source{#[macro_export]macro_rules! factory{()=>{#[macro_export]macro_rules! entity{($name:ident)=>{pub struct $name;}}}}}fn main(){let _=DebugType;}",
            true,
        ),
        (
            "textual_order",
            "make!();macro_rules! make{()=>{}}fn main(){}",
            false,
        ),
        (
            "private_macro",
            "mod debug{crate::entity!(DebugType);}mod entity{macro_rules! entity{($name:ident)=>{pub struct $name;}}}fn main(){}",
            false,
        ),
        (
            "wrong_self_module",
            "mod debug{self::entity!(DebugType);}mod entity{#[macro_export]macro_rules! entity{($name:ident)=>{pub struct $name;}}}fn main(){}",
            false,
        ),
        (
            "missing_export",
            "mod entity{}mod debug{crate::entity!();}fn main(){}",
            false,
        ),
    ] {
        if !case_filter::selected(name) {
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
        let mut diagnostics = db.syntax_errors();
        diagnostics.extend(hir.diagnostics);
        assert_eq!(diagnostics.is_empty(), accepted, "{name}: {diagnostics:?}");
    }
}

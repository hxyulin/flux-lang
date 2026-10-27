//! Diagnostic hints and exported helper macros retain Rust's lookup rules.
use std::process::Command;

#[test]
fn backend_macro_admission_matches_rustc() {
    let root = std::env::temp_dir().join(format!(
        "flux-backend-macro-admission-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    for (name, text, accepted) in [
        (
            "qualified_derive",
            "#[::core::prelude::v1::derive(::core::clone::Clone,::core::marker::Copy)]struct S(u32); fn main(){let _=S(1).clone();}",
            true,
        ),
        (
            "derive_alias",
            "use core::prelude::v1::derive as Builtin; #[Builtin(Clone)]struct S; fn main(){let _=S.clone();}",
            true,
        ),
        (
            "fake_derive",
            "mod fake{pub trait Clone{}} #[derive(fake::Clone)]struct S; fn main(){}",
            false,
        ),
        (
            "unsafe_derive",
            "#[unsafe(::core::prelude::v1::derive(Clone))]struct S; fn main(){}",
            false,
        ),
        (
            "macro_item_fragment",
            "macro_rules! pass{($i:item)=>{$i}} macro_rules! prelude{()=>{struct Inner;}} pass!(prelude!();); fn main(){let _=Inner;}",
            true,
        ),
        (
            "adjacent_metavariable_repeat",
            "macro_rules! m{($($i:ident $e:expr)*)=>{}}fn main(){m!(A 1 B 2);}",
            true,
        ),
        (
            "expression_repeat",
            "macro_rules! m{($($e:expr)*)=>{}}fn main(){m!(1 2);}",
            true,
        ),
        (
            "direct_bad_follower",
            "macro_rules! m{($e:expr $i:ident)=>{}}fn main(){}",
            false,
        ),
        (
            "token_head_repeat",
            "macro_rules! m{($(x $e:expr)*)=>{}}fn main(){m!(x 1 x 2);}",
            true,
        ),
        (
            "bad_follow_after_repeat",
            "macro_rules! m{($($i:ident $e:expr)* $j:ident)=>{}}fn main(){}",
            false,
        ),
        (
            "hint",
            "trait T{} struct S; #[diagnostic::do_not_recommend] impl T for S{} fn main(){}",
            true,
        ),
        (
            "configured_hint",
            "trait T{} struct S; #[cfg_attr(all(),diagnostic::do_not_recommend)] impl T for S{} fn main(){}",
            true,
        ),
        (
            "unsafe_hint",
            "trait T{} struct S; #[unsafe(diagnostic::do_not_recommend)] impl T for S{} fn main(){}",
            false,
        ),
        (
            "unknown_namespace",
            "trait T{} struct S; #[other::do_not_recommend] impl T for S{} fn main(){}",
            false,
        ),
        (
            "local_inner",
            "#[macro_export]macro_rules! helper{()=>{17}} #[macro_export(local_inner_macros)]macro_rules! outer{()=>{helper!()}} fn main(){macro_rules! helper{()=>{99}} let _=outer!();}",
            true,
        ),
        (
            "private_helper",
            "mod definitions{macro_rules! helper{()=>{17}} #[macro_export(local_inner_macros)]macro_rules! outer{()=>{helper!()}}} fn main(){let _=outer!();}",
            false,
        ),
        (
            "captured_macro",
            "#[macro_export(local_inner_macros)]macro_rules! outer{($m:ident)=>{$m!()}} fn main(){macro_rules! helper{()=>{99}} let _=outer!(helper);}",
            true,
        ),
        (
            "qualified_call",
            "#[macro_export(local_inner_macros)]macro_rules! outer{()=>{core::line!()}} fn main(){let _=outer!();}",
            true,
        ),
    ] {
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, text).unwrap();
        let oracle = Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("probe.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            oracle.status.success(),
            accepted,
            "{name}: {}",
            String::from_utf8_lossy(&oracle.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text(&path, text);
        assert_eq!(
            hir.diagnostics.is_empty(),
            accepted,
            "{name}: {:?}",
            hir.diagnostics
        );
    }
}

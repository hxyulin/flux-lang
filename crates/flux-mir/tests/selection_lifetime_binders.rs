//! Anonymous owner rows and existential impl lifetime selection.
#[test]
fn selection_binder_controls_match_rustc() {
    for (name, source, accepted) in [
        (
            "native",
            include_str!("../../../tests/run/selection_lifetime_binders.rs"),
            true,
        ),
        (
            "unused",
            "trait Family<'de>{type Error;fn value(&self)->Self::Error;}struct S;impl<'de,'unused>Family<'de> for S{type Error=();fn value(&self)->(){}}fn main(){}",
            true,
        ),
        (
            "anonymous_escape",
            "trait Into<'a>{type Out;fn into(self)->Self::Out;}struct S<'s>(&'s u8);impl<'s>Into<'_> for S<'s>{type Out=&'static u8;fn into(self)->Self::Out{self.0}}fn main(){}",
            false,
        ),
        (
            "constrained_selection",
            "trait Family{type Out;fn value()->Self::Out;}struct S<T>(T);impl<'b:'static,T:'b>Family for S<T>{type Out=();fn value()->(){}}fn f<'a>(_:&'a u8){let _=<S<&'a u8>as Family>::value();}fn main(){}",
            false,
        ),
        (
            "stronger_method",
            "trait Seed<'a>{type Out;}trait Access<'a>{type Error;fn next<T:Seed<'a>>(&mut self,seed:T)->Result<Option<T::Out>,Self::Error>;}struct S;impl<'a,'unused>Access<'a> for S{type Error=();fn next<T:Seed<'static>>(&mut self,_seed:T)->Result<Option<T::Out>,()>{Ok(None)}}fn main(){}",
            false,
        ),
    ] {
        let directory = std::env::temp_dir().join(format!(
            "flux-selection-binders-{name}-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("case.rs");
        std::fs::write(&path, source).unwrap();
        let reference = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata", "--out-dir"])
            .arg(&directory)
            .arg(&path)
            .output()
            .unwrap();
        assert_eq!(
            reference.status.success(),
            accepted,
            "rustc {name}: {}",
            String::from_utf8_lossy(&reference.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text(&path, source);
        assert!(hir.diagnostics.is_empty(), "{name}: {:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        let mut errors = typed.diagnostics.clone();
        if errors.is_empty() {
            errors.extend(flux_mir::build_file(&hir, &typed).diagnostics);
        }
        assert_eq!(errors.is_empty(), accepted, "flux {name}: {errors:?}");
        std::fs::remove_dir_all(directory).unwrap();
    }
}

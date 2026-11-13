//! Abstract output equality provenance and closure parent output lifetimes.
#[test]
fn caller_projection_and_callback_controls_match_rustc() {
    for (name, source, accepted) in [
        (
            "native",
            include_str!("../../../tests/run/caller_projection_regions.rs"),
            true,
        ),
        (
            "mixed",
            "fn wrap<'a,F>(f:F)->impl for<'x>Fn(&'x u8)->(&'x u8,&'a u8)where F:for<'x>Fn(&'x u8)->(&'x u8,&'a u8){move|i:&u8|f(i)}fn main(){}",
            true,
        ),
        (
            "pointer_context",
            "fn main(){let f:for<'a>fn(&'a u8)->&'a u8=|x:&u8|x;let value=7;assert_eq!(*f(&value),7);}",
            true,
        ),
        (
            "object_context",
            "fn main(){let f:&dyn for<'a>Fn(&'a u8)->&'a u8=&|x:&u8|x;let value=7;assert_eq!(*f(&value),7);}",
            true,
        ),
        ("unhinted", "fn main(){let _f=|x:&u8|x;}", false),
        (
            "mixed_swapped",
            "fn wrap<'a,F>(f:F)->impl for<'x>Fn(&'x u8)->(&'a u8,&'x u8)where F:for<'x>Fn(&'x u8)->(&'x u8,&'a u8){move|i:&u8|f(i)}fn main(){}",
            false,
        ),
        (
            "unrelated",
            "fn wrap<'a,'b,F>(f:F)->impl Fn(&'a u8)->&'a u8 where F:Fn(&'a u8)->&'b u8{move|i:&'a u8|f(i)}fn main(){}",
            false,
        ),
        (
            "static_escape",
            "fn wrap<'a,F>(f:F)->impl Fn(&'a u8)->&'static u8 where F:Fn(&'a u8)->&'a u8{move|i:&'a u8|f(i)}fn main(){}",
            false,
        ),
        (
            "projection_escape",
            "trait Parser<I>{type Error;}trait Error<I>{fn from_input(input:I)->Self;}struct P;struct E<'a>(&'a u8);impl<'a>Parser<&'a u8> for P{type Error=E<'a>;}impl<'a>Error<&'a u8> for E<'a>{fn from_input(input:&'a u8)->Self{Self(input)}}fn convert<I,E,F>(input:I)->E where F:Parser<I,Error=E>,E:Error<I>{<F as Parser<I>>::Error::from_input(input)}fn bad<'a>(input:&'a u8)->E<'static>{convert::<_,E<'static>,P>(input)}fn main(){}",
            false,
        ),
        (
            "missing_equality",
            "trait Parser<I>{type Error;}trait Error<I>{fn from_input(input:I)->Self;}fn convert<I,E,F>(input:I)->E where F:Parser<I>,E:Error<I>{<F as Parser<I>>::Error::from_input(input)}fn main(){}",
            false,
        ),
    ] {
        let directory =
            std::env::temp_dir().join(format!("flux-caller-output-{name}-{}", std::process::id()));
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

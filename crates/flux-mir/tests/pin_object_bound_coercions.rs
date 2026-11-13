//! Identity object coercions retain a Pin pointer's bound and reference rows.
#[test]
fn pin_object_bounds_match_rustc() {
    let prelude = "use std::pin::Pin;trait Value{}";
    for (name, source, accepted) in [
        (
            "get",
            "struct Holder<'a>{value:Pin<Box<dyn Value+'a>>}impl Holder<'_>{fn get(&mut self)->Pin<&mut dyn Value>{self.value.as_mut()}}fn main(){}",
            true,
        ),
        (
            "shorten",
            "fn get<'a>(p:Pin<&'a mut(dyn Value+'static)>)->Pin<&'a mut(dyn Value+'a)>{p}fn main(){}",
            true,
        ),
        (
            "widen",
            "fn get<'a>(p:Pin<&'a mut(dyn Value+'a)>)->Pin<&'a mut(dyn Value+'static)>{p}fn main(){}",
            false,
        ),
        (
            "escape",
            "fn get<'a>(p:Pin<&'a mut(dyn Value+'static)>)->Pin<&'static mut(dyn Value+'static)>{p}fn main(){}",
            false,
        ),
        (
            "store",
            "fn put<'a>(slot:&mut Pin<&'static mut(dyn Value+'static)>,value:Pin<&'a mut(dyn Value+'a)>){*slot=value;}fn main(){}",
            false,
        ),
        (
            "callback",
            "fn takes(_:Pin<&mut(dyn Value+'static)>){ }fn main(){let _:for<'a>fn(Pin<&'a mut(dyn Value+'a)>)=takes;}",
            false,
        ),
        (
            "associated",
            "trait Out{type Item;}fn get<'a>(p:Pin<&'a mut(dyn Out<Item=&'static u8>+'static)>)->Pin<&'a mut(dyn Out<Item=&'a u8>+'a)>{p}fn main(){}",
            false,
        ),
    ] {
        let source = format!("{prelude}{source}");
        let directory =
            std::env::temp_dir().join(format!("flux-pin-object-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("case.rs");
        std::fs::write(&path, &source).unwrap();
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
        let (_, _, hir) = flux_hir::lower_text(&path, &source);
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

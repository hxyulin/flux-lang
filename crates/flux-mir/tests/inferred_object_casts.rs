//! Contextual `as _` uses the ordinary object coercion and its region proof.
use flux_hir::{SourceCrate, SourceDb};
use flux_intern::Symbol;
use flux_syntax::Edition;

#[test]
fn contextual_object_casts_match_rustc() {
    let root =
        std::env::temp_dir().join(format!("flux-inferred-object-casts-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "same",
            "trait Tr{}fn f(x:&dyn Tr)->&dyn Tr{x as _}fn main(){}",
            true,
        ),
        (
            "erase_markers",
            "trait Tr{}fn f(x:&(dyn Tr+Send+Sync))->&dyn Tr{x as _}fn main(){}",
            true,
        ),
        (
            "erase_one",
            "trait Tr{}fn f(x:&(dyn Tr+Send+Sync))->&(dyn Tr+Send){x as _}fn main(){}",
            true,
        ),
        (
            "explicit_target",
            "trait Tr{}fn f(x:&(dyn Tr+Send+Sync))->&dyn Tr{x as &dyn Tr}fn main(){}",
            true,
        ),
        (
            "clap_map",
            "fn f(x:&Option<Box<dyn std::error::Error+Send+Sync>>)->Option<&(dyn std::error::Error+'static)>{x.as_ref().map(|e|e.as_ref() as _)}fn main(){}",
            true,
        ),
        (
            "upcast",
            "trait Base{}trait Sub:Base{}fn f(x:&dyn Sub)->&dyn Base{x as _}fn main(){}",
            true,
        ),
        (
            "mutable",
            "trait Tr{}fn f(x:&mut(dyn Tr+Send))->&mut dyn Tr{x as _}fn main(){}",
            true,
        ),
        (
            "raw",
            "trait Tr{}fn f(x:*const(dyn Tr+Send))->*const dyn Tr{x as _}fn main(){}",
            true,
        ),
        (
            "add_marker",
            "trait Tr{}fn f(x:&dyn Tr)->&(dyn Tr+Send){x as _}fn main(){}",
            false,
        ),
        (
            "wrong_principal",
            "trait Tr{}trait Other{}fn f(x:&dyn Tr)->&dyn Other{x as _}fn main(){}",
            false,
        ),
        (
            "downcast",
            "trait Base{}trait Sub:Base{}fn f(x:&dyn Base)->&dyn Sub{x as _}fn main(){}",
            false,
        ),
        (
            "mutability",
            "trait Tr{}fn f(x:&dyn Tr)->&mut dyn Tr{x as _}fn main(){}",
            false,
        ),
        (
            "lifetime_escape",
            "trait Tr{}fn f<'a>(x:&'a(dyn Tr+'a))->&'static(dyn Tr+'static){x as _}fn main(){}",
            false,
        ),
        (
            "object_lifetime_escape",
            "trait Tr{}fn f<'a>(x:&'a(dyn Tr+'a))->&'a(dyn Tr+'static){x as _}fn main(){}",
            false,
        ),
        (
            "unrelated_reference",
            "fn f(x:&u32)->&u64{x as _}fn main(){}",
            false,
        ),
    ] {
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let output = std::process::Command::new("rustc")
            .args(["--edition=2021", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("r.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            output.status.success(),
            accepted,
            "Rust {name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let mut db = SourceDb::new();
        let file = db.add_with_edition(format!("/object-casts/{name}.rs"), source, Edition::E2021);
        let hir = flux_hir::lower_sources(
            &mut db,
            &[SourceCrate {
                root: file,
                name: Symbol::intern(name),
                dependencies: vec![],
            }],
        );
        let typed = flux_ty::check_file(&hir);
        let mut errors: Vec<_> = hir
            .diagnostics
            .iter()
            .chain(&typed.diagnostics)
            .map(|d| d.message.clone())
            .collect();
        if errors.is_empty() {
            errors.extend(
                flux_mir::build_file(&hir, &typed)
                    .diagnostics
                    .into_iter()
                    .map(|d| d.message),
            );
        }
        assert_eq!(errors.is_empty(), accepted, "Flux {name}: {errors:?}");
    }
    std::fs::remove_dir_all(root).unwrap();
}

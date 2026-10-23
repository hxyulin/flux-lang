//! Lifetime contracts are compile-only: invalid Rust is never executed.
use std::process::Command;

fn oracle(name: &str, body: &str, rust_accepts: bool, flux_accepts: bool) {
    let dir = std::env::temp_dir().join(format!("flux-cell-variance-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join(format!("{name}.rs"));
    std::fs::write(
        &file,
        format!(
            "use std::cell::{{Cell, RefCell, UnsafeCell, Ref, RefMut}};\n\
             use std::sync::{{Arc, Weak}};\n\
             use std::marker::PhantomData;\n\
             struct Wrap<T>(T);\n\
             type Alias<T> = Wrap<Cell<T>>;\n\
             {body}\nfn main() {{}}\n"
        ),
    )
    .unwrap();
    let rustc = Command::new("rustc")
        .args(["--edition=2024", "--emit=metadata"])
        .arg(&file)
        .arg("-o")
        .arg(dir.join(format!("{name}.rmeta")))
        .output()
        .unwrap();
    assert_eq!(
        rustc.status.success(),
        rust_accepts,
        "rustc {name}: {}",
        String::from_utf8_lossy(&rustc.stderr)
    );
    let flux = Command::new(env!("CARGO_BIN_EXE_flux"))
        .arg("mir")
        .arg(&file)
        .output()
        .unwrap();
    assert_eq!(
        flux.status.success(),
        flux_accepts,
        "Flux {name}: {}",
        String::from_utf8_lossy(&flux.stderr)
    );
    if rust_accepts && !flux_accepts {
        assert!(
            String::from_utf8_lossy(&flux.stderr)
                .contains("lifetimes in function pointer types are not supported yet")
        );
    }
    std::fs::remove_file(file).unwrap();
}

#[test]
fn invariant_payloads_and_equal_regions() {
    for (name, ty) in [
        ("cell", "Cell<&'b u32>"),
        ("ref_cell", "RefCell<&'b u32>"),
        ("unsafe_cell", "UnsafeCell<&'b u32>"),
        ("nested_cell", "Cell<Cell<&'b u32>>"),
        ("nested_ref_cell", "RefCell<Cell<&'b u32>>"),
        ("arc_cell", "Arc<Cell<&'b u32>>"),
        ("weak_cell", "Weak<Cell<&'b u32>>"),
        ("arc_ref_cell", "Arc<RefCell<&'b u32>>"),
        ("weak_ref_cell", "Weak<RefCell<&'b u32>>"),
        ("wrapper", "Wrap<Cell<&'b u32>>"),
        ("ref_wrapper", "Wrap<RefCell<&'b u32>>"),
        ("phantom", "PhantomData<Cell<&'b u32>>"),
        ("alias", "Alias<&'b u32>"),
        ("ref_mut_payload", "RefMut<'g, &'b u32>"),
    ] {
        let target = ty.replace("'b", "'a");
        for (suffix, bounds, accepts) in [
            ("shorten", "'a, 'b: 'a", false),
            ("lengthen", "'a: 'b, 'b", false),
            ("equal", "'a: 'b, 'b: 'a", true),
            ("identical", "'a, 'b", true),
        ] {
            let target = if suffix == "identical" { ty } else { &target };
            oracle(
                &format!("{name}_{suffix}"),
                &format!("fn f<'g, {bounds}>(x: {ty}) -> {target} {{ x }}"),
                accepts,
                accepts,
            );
        }
    }
}

#[test]
fn covariance_and_safe_mutation_constraints() {
    for (name, ty) in [
        ("arc", "Arc<&'b u32>"),
        ("weak", "Weak<&'b u32>"),
        ("ref_payload", "Ref<'g, &'b u32>"),
        ("ref_borrow", "Ref<'b, u32>"),
        ("ref_mut_borrow", "RefMut<'b, u32>"),
        ("wrapper", "Wrap<&'b u32>"),
        ("phantom", "PhantomData<&'b u32>"),
    ] {
        oracle(
            name,
            &format!(
                "fn f<'g, 'a, 'b: 'a>(x: {ty}) -> {} {{ x }}",
                ty.replace("'b", "'a")
            ),
            true,
            true,
        );
    }
    for (name, body) in [
        (
            "set",
            "fn f<'a, 'b: 'a>(x: &Cell<&'b u32>, y: &'a u32) { x.set(y); }",
        ),
        (
            "replace",
            "fn f<'a, 'b: 'a>(x: &Cell<&'b u32>, y: &'a u32) { x.replace(y); }",
        ),
        (
            "ref_replace",
            "fn f<'a, 'b: 'a>(x: &RefCell<&'b u32>, y: &'a u32) { x.replace(y); }",
        ),
        (
            "ref_mutate",
            "fn f<'a, 'b: 'a>(x: &RefCell<&'b u32>, y: &'a u32) { *x.borrow_mut() = y; }",
        ),
    ] {
        oracle(name, body, false, false);
        oracle(
            &format!("{name}_equal"),
            &body.replace("'a, 'b: 'a", "'a: 'b, 'b: 'a"),
            true,
            true,
        );
    }
    oracle(
        "unsupported_fn_regions",
        "fn f<'a>(x: fn(&'a u32)) -> fn(&'a u32) { x }",
        true,
        false,
    );
}

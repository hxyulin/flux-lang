//! Shared types are DAGs: the expanded textual type is exponentially larger
//! than the source (or the interned graph). Keep dangerous probes in children.
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use flux_intern::Symbol;
use flux_ty::{Ty, TyKind};

#[path = "../../flux-syntax/tests/support/scaling.rs"]
mod scaling;

#[test]
fn elaboration_preserves_diamond_arguments_and_order() {
    use flux_hir::TraitId;
    use flux_ty::{Predicate, Signatures, TraitRef, TraitSig};
    let make = |id, args: &[Ty]| TraitRef {
        trait_: TraitId::from_raw(id),
        name: Symbol::intern(&format!("GraphTrait{id}")),
        args: Ty::list(args),
    };
    let root = make(0, &[]);
    let left = make(1, &[]);
    let right = make(2, &[]);
    let base_bool = make(3, &[Ty::BOOL]);
    let base_char = make(3, &[Ty::CHAR]);
    let mut sigs = Signatures::default();
    for (t, supers) in [
        (root, vec![left, right]),
        (left, vec![base_bool]),
        (right, vec![base_bool, base_char]),
    ] {
        sigs.traits.insert(
            t.trait_,
            TraitSig {
                supertraits: supers.into(),
                ..TraitSig::default()
            },
        );
    }
    let out = flux_ty::traits::elaborate(&sigs, &[root.with_self(Ty::BOOL)]);
    assert_eq!(
        out,
        [root, left, base_bool, right, base_char].map(|trait_ref| Predicate {
            self_ty: Ty::BOOL,
            trait_ref,
        })
    );
}

#[test]
fn expanding_supertrait_cycle_is_bounded() {
    child("cycle", 0, Duration::from_secs(5));
}

#[test]
fn source_substitution_preserves_types() {
    child("source", 0, Duration::from_secs(5));
}

#[test]
fn shared_substitution_scales_with_graph_size() {
    child("scaling", 0, Duration::from_secs(15));
}

fn child(probe: &str, n: usize, timeout: Duration) {
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "type_graph_probe", "--nocapture"])
        .env("FLUX_TYPE_GRAPH_PROBE", probe)
        .env("FLUX_TYPE_GRAPH_N", n.to_string())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success(), "{probe}({n}): {status}");
            return;
        }
        if start.elapsed() > timeout {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("{probe}({n}) exceeded {timeout:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn shared_substitution_is_bounded() {
    child("subst", 28, Duration::from_secs(5));
}

#[test]
#[ignore = "subprocess entry point; run through the bounded parent"]
fn type_graph_probe() {
    let Ok(probe) = std::env::var("FLUX_TYPE_GRAPH_PROBE") else {
        return;
    };
    let n: usize = std::env::var("FLUX_TYPE_GRAPH_N").unwrap().parse().unwrap();
    match probe.as_str() {
        "subst" => {
            let mut ty = Ty::param(0, Symbol::intern("GraphParam"));
            for _ in 0..n {
                ty = Ty::tuple(&[ty, ty]);
            }
            let start = Instant::now();
            let mut result = ty.subst(&[Ty::BOOL]);
            println!("subst: {n} DAG nodes in {:?}", start.elapsed());
            for _ in 0..n {
                let TyKind::Tuple(&[a, b]) = result.kind() else {
                    panic!("expected pair")
                };
                assert_eq!(a, b);
                result = a;
            }
            assert_eq!(result, Ty::BOOL);
        }
        "scaling" => {
            scaling::assert_scales(
                "shared substitution DAG nodes",
                1000,
                |n| {
                    let mut ty = Ty::param(0, Symbol::intern("ScalingGraphParam"));
                    for _ in 0..n {
                        ty = Ty::tuple(&[ty, ty]);
                    }
                    ty
                },
                |ty| {
                    std::hint::black_box(ty.subst(&[Ty::CHAR]));
                },
            );
        }
        "source" | "cycle" => {
            let name = if probe == "source" {
                "ok/type_graph_shared_substitution"
            } else {
                "err/type_graph_expanding_cycle"
            };
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/data")
                .join(name)
                .with_extension("rs");
            let text = std::fs::read_to_string(&path).unwrap();
            let (_, _, hir) = flux_hir::lower_text(&path, &text);
            assert!(hir.diagnostics.is_empty());
            let typed = flux_ty::check_file(&hir);
            assert_eq!(typed.diagnostics.is_empty(), probe == "source");
            let actual = typed.debug_dump(&hir);
            let snapshot = path.with_extension("ty");
            if std::env::var_os("UPDATE_EXPECT").is_some() {
                std::fs::write(&snapshot, &actual).unwrap();
            }
            assert_eq!(actual, std::fs::read_to_string(snapshot).unwrap());
        }
        _ => panic!("unknown probe"),
    }
}

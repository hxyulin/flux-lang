//! Ordered-map operations and allocation-slot reuse match Rust ownership rules.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;

#[test]
fn btree_map_surface_matches_rustc() {
    support::oracle::compare_fixture("btree_map_surface");
}

#[test]
fn btree_map_host_storage_invariants() {
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let implementation = repo.join("library/alloc/src/collections/btree_map.rs");
    let original = std::fs::read(&implementation).unwrap();
    let directory = repo.join(format!("out/btree-map-invariants/{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let wrapper = directory.join("host.rs");
    std::fs::write(&wrapper, format!("#![allow(dead_code)]\nmod vec {{ pub use std::vec::{{Vec, IntoIter}}; }}\n#[path = {implementation:?}] mod implementation;\n")).unwrap();
    let executable = directory.join("host");
    for optimized in [false, true] {
        let mut compile = std::process::Command::new("rustc");
        compile
            .args(["--edition=2024", "--test"])
            .arg(&wrapper)
            .arg("-o")
            .arg(&executable);
        if optimized {
            compile.arg("-O");
        }
        let output = compile.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let output = std::process::Command::new(&executable).output().unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert_eq!(std::fs::read(implementation).unwrap(), original);
}

#[test]
fn btree_map_entry_borrow_contracts_match_rustc() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(format!("out/btree-map-contracts/{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, body, accepted) in [
        (
            "unbounded_iteration_equality",
            "#[derive(PartialEq,Eq)]struct K;fn main(){let m:BTreeMap<K,K>=BTreeMap::new();let n:BTreeMap<K,K>=BTreeMap::new();assert!(m==n);assert!(m.keys().next().is_none());}",
            true,
        ),
        (
            "first_requires_order",
            "struct K;fn main(){let m:BTreeMap<K,()>=BTreeMap::new();m.first_key_value();}",
            false,
        ),
        (
            "vacant_key_requires_order",
            "fn key<K,V>(e:std::collections::btree_map::VacantEntry<'_,K,V>)->K{e.into_key()}fn main(){}",
            false,
        ),
        (
            "occupied_value_requires_order",
            "fn val<'a,K,V>(e:std::collections::btree_map::OccupiedEntry<'a,K,V>)->&'a mut V{e.into_mut()}fn main(){}",
            false,
        ),
        (
            "entry_requires_order",
            "struct K;fn main(){let mut m=BTreeMap::new();m.entry(K).or_insert(1);}",
            false,
        ),
        (
            "default_requires_value_default",
            "struct V;fn main(){let mut m:BTreeMap<u32,V>=BTreeMap::new();m.entry(1).or_default();}",
            false,
        ),
        (
            "entry_result_cannot_escape_map",
            "fn escape<'a>()->&'a mut u32{let mut m=BTreeMap::new();m.entry(1).or_insert(2)}fn main(){}",
            false,
        ),
        (
            "occupied_loan_excludes_remove",
            "fn main(){let mut m=BTreeMap::new();let v=m.entry(1).or_insert(2);m.remove(&1);println!(\"{}\",v);}",
            false,
        ),
        (
            "key_iterator_cannot_escape",
            "fn escape<'a>()->&'a u32{let mut m=BTreeMap::new();m.insert(1,2);m.keys().next().unwrap()}fn main(){}",
            false,
        ),
        (
            "entry_key_loan",
            "fn escape<'a>()->BTreeMap<&'a u32,u32>{let x=1;let mut m=BTreeMap::new();m.entry(&x).or_insert(2);m}fn main(){}",
            false,
        ),
        (
            "mutable_reference_lifetime",
            "fn via<'a>(m:&'a mut BTreeMap<u32,u32>)->&'a mut u32{m.entry(1).and_modify(|x|*x+=1).or_default()}fn main(){}",
            true,
        ),
    ] {
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, format!("use std::collections::BTreeMap;{body}")).unwrap();
        let reference = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("case.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            reference.status.success(),
            accepted,
            "rustc {name}: {}",
            String::from_utf8_lossy(&reference.stderr)
        );
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_flux"))
            .arg("mir")
            .arg(&path)
            .output()
            .unwrap();
        assert_eq!(
            output.status.success(),
            accepted,
            "Flux {name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

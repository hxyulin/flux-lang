//! Bounded ordered-map contracts. Invalid probes are compiled, never executed.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
use std::path::Path;
use std::process::Command;

fn compile(path: &Path, output: &Path, flux: bool, optimized: bool) -> std::process::Output {
    let mut command = Command::new(if flux {
        env!("CARGO_BIN_EXE_flux")
    } else {
        "rustc"
    });
    if flux {
        command.arg("build");
    } else {
        command.args(["--edition=2024", "-C", "panic=abort"]);
    }
    command.arg(path).arg("-o").arg(output);
    if optimized {
        command.arg("-O");
    }
    command.output().unwrap()
}

#[test]
fn ordered_map_source_and_std_oracle() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let implementation = root.join("library/alloc/src/collections/btree_map.rs");
    let original = std::fs::read(&implementation).unwrap();
    let driver = std::fs::read_to_string(root.join("tests/run/btree_map.rs")).unwrap();
    let arena_path = root.join("crates/flux-arena/src/typed.rs");
    let arena_original = std::fs::read_to_string(&arena_path).unwrap();
    let start = arena_original.find("pub struct Idx<T>").unwrap();
    let end = arena_original
        .find("/// Values of one type, named by")
        .unwrap();
    let index_driver = r#"
use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
fn index_consumer<T>() {
    let a = Idx::<T>::from_raw(4);
    let b = Idx::<T>::from_raw(2);
    let mut map: BTreeMap<Idx<T>, String> = Default::default();
    assert!(map.is_empty());
    map.insert(a, String::from("four"));
    map.insert(b, String::from("two"));
    assert!(map.contains_key(&a));
    assert_eq!(map.get(&b).unwrap(), "two");
    map.get_mut(&b).unwrap().push('!');
    let copy = map.clone();
    let mut borrowed = copy.iter().map(|(&idx, value)| (idx, value)).peekable();
    assert_eq!(borrowed.peek().unwrap().0.into_raw(), 2);
    assert_eq!(borrowed.next().unwrap().1, "two!");
    let owned = std::mem::take(&mut map);
    assert!(map.is_empty());
    let mut iter = owned.into_iter();
    assert_eq!(iter.next().unwrap().0.into_raw(), 2);
    assert_eq!(iter.next().unwrap().0.into_raw(), 4);
    assert!(iter.next().is_none());
}
struct NoTraits;
"#;
    let driver = format!(
        "{}\n{index_driver}\n{}",
        &arena_original[start..end],
        driver.replace("fn main() {", "fn main() { index_consumer::<NoTraits>();")
    );
    let dir = std::env::temp_dir().join(format!("flux-btree-map-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let wrapper = format!(
        "mod vec {{ pub use std::vec::Vec; }}\n#[path = {implementation:?}] mod implementation;\n"
    );
    // The implementation file is loaded verbatim. Only the driver's import changes.
    let wrapped = format!(
        "{wrapper}{}",
        driver.replace(
            "use std::collections::BTreeMap;",
            "use implementation::BTreeMap;"
        )
    );
    let mut expected = None;
    for (name, source, flux) in [
        ("std", driver.as_str(), false),
        ("source", wrapped.as_str(), false),
        ("flux", driver.as_str(), true),
    ] {
        let path = dir.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        for optimized in [false, true] {
            let executable = dir.join(format!("{name}-{optimized}"));
            let build = compile(&path, &executable, flux, optimized);
            assert!(
                build.status.success(),
                "{name}: {}",
                String::from_utf8_lossy(&build.stderr)
            );
            let run = Command::new(executable).output().unwrap();
            assert!(
                run.status.success(),
                "{name}: {}",
                String::from_utf8_lossy(&run.stderr)
            );
            assert!(run.stderr.is_empty());
            match &expected {
                None => expected = Some(run.stdout),
                Some(output) => assert_eq!(&run.stdout, output),
            }
        }
    }
    let valid = r#"
use std::collections::BTreeMap;
#[derive(Clone)] struct NoOrd;
fn main() {
    let map: BTreeMap<NoOrd, NoOrd> = Default::default();
    let copy = map.clone();
    assert!(map.iter().next().is_none());
    assert!(copy.into_iter().next().is_none());
}
"#;
    let invalid = [
        (
            "insert-ord",
            "struct Key; fn main() { let mut m = BTreeMap::new(); m.insert(Key, 1); }",
        ),
        (
            "query-ord",
            "struct Key; fn main() { let m: BTreeMap<Key,u8> = BTreeMap::new(); m.get(&Key); }",
        ),
        (
            "clone-value",
            "struct Value; fn main() { let m: BTreeMap<i32,Value> = BTreeMap::new(); m.clone(); }",
        ),
        (
            "clone-key",
            "struct Key; fn main() { let m: BTreeMap<Key,i32> = BTreeMap::new(); m.clone(); }",
        ),
        (
            "query-borrow",
            "fn main() { let m: BTreeMap<i32,i32> = BTreeMap::new(); m.get(\"wrong\"); }",
        ),
        (
            "mutability",
            "fn main() { let m: BTreeMap<i32,i32> = BTreeMap::new(); m.get_mut(&1); }",
        ),
        (
            "iter-lifetime",
            "fn escape<'a>() -> &'a i32 { let mut m = BTreeMap::new(); m.insert(1,2); m.iter().next().unwrap().1 } fn main() {}",
        ),
        (
            "key-lifetime",
            "fn escape<'a>() -> BTreeMap<&'a i32,i32> { let key=1; let mut m=BTreeMap::new(); m.insert(&key,2); m } fn main() {}",
        ),
        (
            "move",
            "fn main() { let m: BTreeMap<i32,i32> = BTreeMap::new(); let _it = m.into_iter(); m.is_empty(); }",
        ),
        (
            "loan",
            "fn main() { let mut m=BTreeMap::new(); m.insert(1,2); let v=m.get(&1).unwrap(); m.insert(2,3); println!(\"{}\",v); }",
        ),
    ];
    for (name, body, success) in std::iter::once(("unbounded", valid.to_owned(), true)).chain(
        invalid.into_iter().map(|(name, body)| {
            (
                name,
                format!("use std::collections::BTreeMap;\n{body}"),
                false,
            )
        }),
    ) {
        for (kind, source, flux) in [
            ("std", body.clone(), false),
            (
                "source",
                format!(
                    "{wrapper}{}",
                    body.replace(
                        "use std::collections::BTreeMap;",
                        "use implementation::BTreeMap;"
                    )
                ),
                false,
            ),
            ("flux", body.clone(), true),
        ] {
            let path = dir.join(format!("{name}-{kind}.rs"));
            std::fs::write(&path, source).unwrap();
            let build = compile(&path, &dir.join("compile-only"), flux, false);
            assert_eq!(
                build.status.success(),
                success,
                "{name}/{kind}: {}",
                String::from_utf8_lossy(&build.stderr)
            );
        }
    }
    assert_eq!(std::fs::read_to_string(arena_path).unwrap(), arena_original);
    assert_eq!(std::fs::read(implementation).unwrap(), original);
    std::fs::remove_dir_all(dir).unwrap();
}

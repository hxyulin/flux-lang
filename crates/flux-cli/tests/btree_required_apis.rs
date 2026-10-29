#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn required_btree_apis_match_rustc() {
    support::oracle::compare_fixture("btree_required_apis");
}

#[test]
fn required_btree_contracts_match_rustc() {
    use std::process::Command;
    let root = std::env::temp_dir().join(format!("flux-btree-required-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, expected, body) in [
        (
            "iter_clone_no_clone",
            true,
            "struct NoClone;fn main(){let m:BTreeMap<NoClone,NoClone>=BTreeMap::new();let _=m.iter().clone();}",
        ),
        (
            "iter_mut_send_cells",
            true,
            "fn need<T:Send>(_:T){}fn main(){let mut m:BTreeMap<std::cell::Cell<u32>,std::cell::Cell<u32>>=BTreeMap::new();need(m.iter_mut());}",
        ),
        (
            "values_mut_send_cells",
            true,
            "fn need<T:Send>(_:T){}fn main(){let mut m:BTreeMap<std::cell::Cell<u32>,std::cell::Cell<u32>>=BTreeMap::new();need(m.values_mut());}",
        ),
        (
            "iter_mut_not_sync_cell",
            false,
            "fn need<T:Sync>(_:T){}fn main(){let mut m:BTreeMap<u32,std::cell::Cell<u32>>=BTreeMap::new();need(m.iter_mut());}",
        ),
        (
            "shared_iter_not_send_cell",
            false,
            "fn need<T:Send>(_:T){}fn main(){let m:BTreeMap<u32,std::cell::Cell<u32>>=BTreeMap::new();need(m.iter());}",
        ),
        (
            "iter_mut_not_clone",
            false,
            "fn main(){let mut m=BTreeMap::from([(1,2)]);m.iter_mut().clone();}",
        ),
        (
            "mut_keys_invariant",
            false,
            "fn shorten<'a>(it:std::collections::btree_map::IterMut<'a,&'static u8,u32>)->std::collections::btree_map::IterMut<'a,&'a u8,u32>{it}fn main(){}",
        ),
        (
            "mut_values_invariant",
            false,
            "fn shorten<'a>(it:std::collections::btree_map::IterMut<'a,u32,&'static u8>)->std::collections::btree_map::IterMut<'a,u32,&'a u8>{it}fn main(){}",
        ),
        (
            "keys_debug_no_value_debug",
            true,
            "struct NoDebug;fn main(){let m:BTreeMap<u32,NoDebug>=BTreeMap::new();let _=format!(\"{:?}\",m.keys());}",
        ),
        (
            "values_debug_no_key_debug",
            true,
            "struct NoDebug;fn main(){let mut m:BTreeMap<NoDebug,u32>=BTreeMap::new();let _=format!(\"{:?}\",m.values());let _=format!(\"{:?}\",m.values_mut());let _=format!(\"{:?}\",m.into_values());}",
        ),
        (
            "iter_debug_requires_value_debug",
            false,
            "struct NoDebug;fn main(){let m:BTreeMap<u32,NoDebug>=BTreeMap::new();let _=format!(\"{:?}\",m.iter());}",
        ),
        (
            "iter_mut_debug_requires_key_debug",
            false,
            "struct NoDebug;fn main(){let mut m:BTreeMap<NoDebug,u32>=BTreeMap::new();let _=format!(\"{:?}\",m.iter_mut());}",
        ),
        (
            "hash_no_order",
            true,
            "#[derive(Hash)]struct NoOrd;fn need<T:std::hash::Hash>(_:T){}fn main(){need(BTreeMap::<NoOrd,NoOrd>::new());}",
        ),
        (
            "set_equality_no_order",
            true,
            "#[derive(PartialEq)]struct NoOrd;fn main(){let a=BTreeSet::<NoOrd>::new();let b=BTreeSet::new();assert!(a==b);}",
        ),
        (
            "array_from_move_only",
            true,
            "struct Owned;fn main(){let m=BTreeMap::from([(1,Owned)]);assert_eq!(m.len(),1);}",
        ),
        (
            "array_from_requires_order",
            false,
            "struct NoOrd;fn main(){let _=BTreeMap::from([(NoOrd,1)]);}",
        ),
        (
            "occupied_remove_requires_order",
            false,
            "fn remove<K,V>(e:std::collections::btree_map::OccupiedEntry<'_,K,V>)->V{e.remove()}fn main(){}",
        ),
        (
            "mut_iteration_borrows_map",
            false,
            "fn main(){let mut m=BTreeMap::from([(1,2)]);let mut it=m.iter_mut();m.clear();it.next();}",
        ),
        (
            "mut_key_not_writable",
            false,
            "fn main(){let mut m=BTreeMap::from([(1,2)]);let(k,_)=m.iter_mut().next().unwrap();*k=7;}",
        ),
        (
            "disjoint_mut_values",
            true,
            "fn main(){let mut m=BTreeMap::from([(1,2),(3,4)]);let mut it=m.iter_mut();let(_,a)=it.next().unwrap();let(_,b)=it.next_back().unwrap();*a+=*b;}",
        ),
    ] {
        let source = format!("use std::collections::{{BTreeMap,BTreeSet}};{body}");
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, &source).unwrap();
        let rust = Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("oracle.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            rust.status.success(),
            expected,
            "Rust {name}: {}",
            String::from_utf8_lossy(&rust.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text(&path, &source);
        assert!(hir.diagnostics.is_empty(), "{name}: {:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        let accepted = typed.diagnostics.is_empty()
            && flux_mir::build_file(&hir, &typed).diagnostics.is_empty();
        assert_eq!(accepted, expected, "Flux {name}: {:?}", typed.diagnostics);
    }
}

#[test]
fn btree_allocation_unwinds_preserve_ownership() {
    use std::{path::Path, process::Command, time::Duration};
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let root = repo.join(format!("out/btree-allocation/{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let source = root.join("host.rs");
    std::fs::write(
        &source,
        include_str!("fixtures/btree_required_apis/allocation.rs.txt").replace(
            "__SOURCE__",
            repo.join("library/alloc/src/collections/btree_map.rs")
                .to_str()
                .unwrap(),
        ),
    )
    .unwrap();
    for optimized in [false, true] {
        let exe = root.join(format!("host-{optimized}"));
        let mut rustc = Command::new("rustc");
        rustc
            .args(["--edition=2024", "-Awarnings"])
            .arg(&source)
            .arg("-o")
            .arg(&exe);
        if optimized {
            rustc.arg("-O");
        }
        let out =
            support::process::run(&mut rustc, &root, "build", Duration::from_secs(60)).completed();
        support::oracle::success(&out, "btree allocation host build");
        let out = support::process::run(
            &mut Command::new(exe),
            &root,
            "run",
            Duration::from_secs(60),
        )
        .completed();
        support::oracle::success(&out, "btree allocation host run");
        assert_eq!(out.stdout, b"btree allocation ownership complete\n");
    }
}

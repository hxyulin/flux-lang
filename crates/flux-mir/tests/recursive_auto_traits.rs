//! Only complete Send/Sync cycles are coinductive; every leaf still holds.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;
#[test]
fn recursive_auto_traits_match_rustc() {
    let root =
        std::env::temp_dir().join(format!("flux-recursive-auto-traits-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let mut borrowed_diamond = String::from("use std::sync::Arc;fn require<T:Send+Sync>(_:T){}");
    for level in 0..22 {
        borrowed_diamond.push_str(&format!(
            "struct Node{level}(Arc<Node{}>,Arc<Node{}>);",
            level + 1,
            level + 2
        ));
    }
    borrowed_diamond.push_str(
        "struct Node22(Arc<Node23>,Arc<Node0>);struct Node23(Arc<Node0>,u32);\
         fn check(root:&Node0){require(root);}fn main(){}",
    );
    let supertrait_diamond = borrowed_diamond.replace(
        "fn check(root:&Node0){require(root);}",
        "trait Shared:Send+Sync{} impl Shared for Node0{}",
    );
    let rejected_diamond =
        supertrait_diamond.replace("Arc<Node0>,u32", "Arc<Node0>,std::rc::Rc<u32>");
    let mut borrowed_supertrait = String::from("use std::sync::Arc;trait Shared:Send+Sync{}");
    for level in 0..20 {
        borrowed_supertrait.push_str(&format!(
            "struct Node{level}<'a>(Arc<Node{}<'a>>,Arc<Node{}<'a>>);",
            level + 1,
            level + 2
        ));
    }
    borrowed_supertrait.push_str(
        "struct Node20<'a>(Arc<Node21<'a>>,Arc<Node0<'a>>);\
         struct Node21<'a>(Arc<Node0<'a>>,&'a u32);\
         impl<'a> Shared for Node0<'a>{} fn main(){}",
    );
    let projected_environment = borrowed_supertrait.replace(
        "impl<'a> Shared for Node0<'a>{}",
        "trait TypeSource{type Out;}struct Witness;impl TypeSource for Witness{type Out=u32;}\
         impl<'a> Shared for Node0<'a> where Witness:TypeSource<Out=u32>{}",
    );
    let inherited_object_marker = projected_environment
        .replace(
            "trait Shared:Send+Sync{}",
            "trait Marker:Send+Sync{}trait Object:Marker{}trait Shared:Send+Sync{}",
        )
        .replace("&'a u32", "Box<dyn Object+'a>")
        .replace(
            "fn main(){}",
            "fn require<T:Send+Sync>(_:T){}fn check<'a>(node:Node0<'a>){require(move||{let _node=node;});}fn main(){}",
        );
    let missing_object_marker =
        inherited_object_marker.replace("trait Marker:Send+Sync", "trait Marker:Send");
    for (name, source, accepted) in [
        (
            "recursive",
            "struct N{next:Option<Box<N>>,n:u32}fn need<T:Send+Sync>(){}fn main(){need::<N>();}",
            true,
        ),
        (
            "generic_recursive",
            "struct N<T>{next:Option<Box<N<T>>>,value:T}fn need<T:Send+Sync>(){}fn generic<T:Send+Sync>(){need::<N<T>>();}fn main(){generic::<u32>();}",
            true,
        ),
        (
            "mutual",
            "struct A(Option<Box<B>>);struct B(Option<Box<A>>,u32);fn need<T:Send+Sync>(){}fn main(){need::<A>();}",
            true,
        ),
        ("borrowed_recursive_diamond", &borrowed_diamond, true),
        ("recursive_diamond_supertrait", &supertrait_diamond, true),
        ("recursive_diamond_non_send_leaf", &rejected_diamond, false),
        ("borrowed_recursive_supertrait", &borrowed_supertrait, true),
        (
            "inherited_object_marker_in_recursive_proof",
            &inherited_object_marker,
            true,
        ),
        (
            "missing_object_marker_in_recursive_proof",
            &missing_object_marker,
            false,
        ),
        (
            "recursive_supertrait_with_projection_assumption",
            &projected_environment,
            true,
        ),
        (
            "recursive_supertrait_requires_static_leaf",
            "use std::sync::Arc;trait Shared:Send+Sync{}struct StaticOnly<'a>(&'a u32);unsafe impl Send for StaticOnly<'static>{}struct N<'a>(Arc<N<'a>>,StaticOnly<'a>);impl<'a> Shared for N<'a>{}fn main(){}",
            false,
        ),
        (
            "static",
            "use std::sync::OnceLock;struct N(Option<Box<N>>,u32);static ROOT:OnceLock<N>=OnceLock::new();fn main(){}",
            true,
        ),
        (
            "unsafe_cell_leaf",
            "struct N(Option<Box<N>>,std::cell::UnsafeCell<u32>);fn need<T:Sync>(){}fn main(){need::<N>();}",
            false,
        ),
        (
            "raw_leaf",
            "struct N(Option<Box<N>>,*const u32);fn need<T:Send>(){}fn main(){need::<N>();}",
            false,
        ),
        (
            "rc_leaf",
            "struct N(Option<Box<N>>,std::rc::Rc<u32>);fn need<T:Send>(){}fn main(){need::<N>();}",
            false,
        ),
        (
            "inductive_cycle",
            "struct N(Option<Box<N>>);trait Gate{}impl Gate for N where N:Send{}unsafe impl Send for N where N:Gate{}fn need<T:Send>(){}fn main(){need::<N>();}",
            false,
        ),
    ] {
        if !case_filter::selected(name) {
            continue;
        }
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let oracle = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("oracle.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            oracle.status.success(),
            accepted,
            "{name}: {}",
            String::from_utf8_lossy(&oracle.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text(&path, source);
        let mut errors = hir.diagnostics.clone();
        if errors.is_empty() {
            let typed = flux_mir::check_file(&hir);
            errors.extend(typed.diagnostics.clone());
            if errors.is_empty() {
                errors.extend(flux_mir::build_file(&hir, &typed).diagnostics);
            }
        }
        assert_eq!(errors.is_empty(), accepted, "{name}: {errors:?}");
    }
}

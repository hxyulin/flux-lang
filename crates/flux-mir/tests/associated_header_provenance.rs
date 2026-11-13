//! Declaration equalities retain the selected RHS's original input row.
fn check(name: &str, source: &str, accepted: bool) {
    let root = std::env::temp_dir().join(format!(
        "flux-associated-header-{name}-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("case.rs");
    std::fs::write(&path, source).unwrap();
    let reference = std::process::Command::new("rustc")
        .args(["--edition=2024", "--emit=metadata", "--out-dir"])
        .arg(&root)
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
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn delegated_reference_associated_bounds_use_the_original_header() {
    check(
        "delegate",
        "trait Base{type Node:Copy;}impl<G:Base>Base for &G{type Node=G::Node;}trait GraphRef:Base+Copy{}impl<G:Base>GraphRef for &G{}trait Neighbors:GraphRef{type Iter:Iterator<Item=Self::Node>;}impl<'a,G:Neighbors>Neighbors for &'a G{type Iter=G::Iter;}fn main(){}",
        true,
    );
}

#[test]
fn selected_output_lifetimes_remain_correlated_with_self() {
    let prelude = "trait Source{type Item;}struct Marker<'a>(&'a u8);impl<'a>Source for Marker<'a>{type Item=&'a u8;}trait Bound:Source{type Iter:Iterator<Item=Self::Item>;}";
    check(
        "matching",
        &format!(
            "{prelude}impl<'a>Bound for Marker<'a>{{type Iter=std::array::IntoIter<&'a u8,1>;}}fn main(){{}}"
        ),
        true,
    );
    check(
        "static_mismatch",
        &format!(
            "{prelude}impl<'a>Bound for Marker<'a>{{type Iter=std::array::IntoIter<&'static u8,1>;}}fn main(){{}}"
        ),
        false,
    );
}

#[test]
fn selected_rhs_still_requires_its_lifetime_restricted_impl() {
    check(
        "restricted_selection",
        "trait Source{type Item;}impl Source for &'static u8{type Item=u8;}trait Bound{type Item;type Iter:Iterator<Item=Self::Item>;}struct Marker<'a>(&'a u8);impl<'a>Bound for Marker<'a>{type Item=<&'a u8 as Source>::Item;type Iter=std::array::IntoIter<u8,1>;}fn main(){}",
        false,
    );
}

#[test]
fn associated_bound_arguments_replay_the_original_header_rows() {
    check(
        "visit_map",
        "trait Base{type Node;}impl<G:Base>Base for &G{type Node=G::Node;}trait Map<N>{}trait Visit:Base{type Map:Map<Self::Node>;}impl<'a,G:Visit>Visit for &'a G{type Map=G::Map;}struct Graph<N>(N);impl<N>Base for Graph<N>{type Node=N;}struct Fixed;impl<N>Map<N> for Fixed{}impl<N>Visit for Graph<N>{type Map=Fixed;}fn main(){}",
        true,
    );
    let prelude = "trait Base{type Node;}trait Map<N>{}trait Visit:Base{type Map:Map<Self::Node>;}struct Graph<'a>(&'a u8);impl<'a>Base for Graph<'a>{type Node=&'a u8;}struct BorrowMap<'a>(&'a u8);impl<'a>Map<&'a u8> for BorrowMap<'a>{}";
    check(
        "map_matching_lifetime",
        &format!("{prelude}impl<'a>Visit for Graph<'a>{{type Map=BorrowMap<'a>;}}fn main(){{}}"),
        true,
    );
    check(
        "map_static_lifetime_mismatch",
        &format!(
            "{prelude}impl<'a>Visit for Graph<'a>{{type Map=BorrowMap<'static>;}}fn main(){{}}"
        ),
        false,
    );
    check(
        "map_type_mismatch",
        "trait Base{type Node;}trait Map<N>{}trait Visit:Base{type Map:Map<Self::Node>;}struct Graph;impl Base for Graph{type Node=u8;}struct Fixed;impl Map<u16> for Fixed{}impl Visit for Graph{type Map=Fixed;}fn main(){}",
        false,
    );
}

#[test]
fn selected_adapter_equalities_keep_nested_projection_provenance() {
    let adapter = "struct Adapt<I>(I);impl<I:Iterator>Iterator for Adapt<I>{type Item=I::Item;fn next(&mut self)->Option<Self::Item>{self.0.next()}}trait Output{type Item;type Iter:Iterator<Item=Self::Item>;}";
    check(
        "nested_adapter",
        &format!(
            "{adapter}struct Wrap<G>(G);impl<G:Output>Output for Wrap<G>{{type Item=G::Item;type Iter=std::iter::Chain<Adapt<G::Iter>,Adapt<G::Iter>>;}}fn main(){{}}"
        ),
        true,
    );
    check(
        "adapter_lifetime_matching",
        &format!(
            "{adapter}struct Graph<'a>(&'a u8);impl<'a>Output for Graph<'a>{{type Item=&'a u8;type Iter=std::iter::Chain<Adapt<std::array::IntoIter<&'a u8,1>>,Adapt<std::array::IntoIter<&'a u8,1>>>;}}fn main(){{}}"
        ),
        true,
    );
    check(
        "adapter_lifetime_mismatch",
        &format!(
            "{adapter}struct Graph<'a,'b>(&'a u8,&'b u8);impl<'a,'b>Output for Graph<'a,'b>{{type Item=&'a u8;type Iter=std::iter::Chain<Adapt<std::array::IntoIter<&'a u8,1>>,Adapt<std::array::IntoIter<&'b u8,1>>>;}}fn main(){{}}"
        ),
        false,
    );
    check(
        "adapter_static_mismatch",
        &format!(
            "{adapter}struct Graph<'a>(&'a u8);impl<'a>Output for Graph<'a>{{type Item=&'a u8;type Iter=std::iter::Chain<Adapt<std::array::IntoIter<&'a u8,1>>,Adapt<std::array::IntoIter<&'static u8,1>>>;}}fn main(){{}}"
        ),
        false,
    );
}

#[test]
fn selected_impl_replays_explicit_lifetime_selection_occurrences() {
    check(
        "inherited_selected_sibling",
        include_str!("../../../tests/run/associated_header_replay.rs"),
        true,
    );
    let prelude = "trait Source{type Item;}struct Graph;impl<'a>Source for &'a Graph{type Item=&'a u8;}trait Edges:Source{type Edges:Iterator<Item=Self::Item>;}struct Wrap(Graph);impl<'a>Source for &'a Wrap{type Item=<&'a Graph as Source>::Item;}";
    check(
        "inherited_selected_lifetime_mismatch",
        &format!(
            "{prelude}impl<'a>Edges for &'a Wrap{{type Edges=std::array::IntoIter<&'static u8,1>;}}fn main(){{}}"
        ),
        false,
    );
    check(
        "inherited_selected_type_mismatch",
        &format!(
            "{prelude}impl<'a>Edges for &'a Wrap{{type Edges=std::array::IntoIter<u16,1>;}}fn main(){{}}"
        ),
        false,
    );
}

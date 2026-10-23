//! Declaration classification only: no gate suppression or invalid execution.
use flux_hir::{Hir, Owner, TraitId};
use flux_ty::regions::{
    Region, TraitProfileReason as Reason, required_method_trait_profile as profile,
};
use flux_ty::{Signatures, Ty};

fn resolve(source: &str) -> (Hir, Signatures, TraitId) {
    let (db, file, hir) = flux_hir::lower_text("profile.rs", source);
    assert!(db.syntax_errors().is_empty(), "{:?}", db.syntax_errors());
    // Lowering diagnostics remain intact; excluded shapes may already be gated.
    let (sigs, _) = flux_ty::signatures(&hir.items);
    let id = hir
        .items
        .traits
        .iter()
        .find(|(_, t)| t.loc.file == file)
        .unwrap()
        .0;
    (hir, sigs, id)
}

const SHAPE: &str = "trait Shape<'a>: Copy { fn tag(x: bool) -> bool; fn cast(x: &'a i32) -> Option<Self>; fn view(self) -> &'a i32; }";

#[test]
fn alpha_renaming_and_elision_preserve_profile_and_gate() {
    for source in [
        SHAPE.to_owned(),
        SHAPE
            .replace("Shape", "Other")
            .replace("'a", "'tree")
            .replace("tag", "test")
            .replace("cast", "wrap")
            .replace("view", "read"),
        "trait T<'a>: Copy { fn get(&self, x: &'a i32) -> &Self; }".to_owned(),
        "trait T<'a> { fn f(x: &'static i32) -> &'static i32; }".to_owned(),
        "trait T<'a> {}".to_owned(),
        "struct Branch<'x>(&'x i32); trait K<'x>:Copy {fn make(x:Branch<'x>)->Self;}".to_owned(),
        "struct Leaf<'renamed>(&'renamed i32); trait L<'renamed>:Copy {fn wrap(y:Leaf<'renamed>)->Self;}".to_owned(),
    ] {
        let (hir, sigs, id) = resolve(&source);
        assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
        assert_eq!(profile(&hir, &sigs, id), Ok(()), "{source}");
        let (_, diagnostics) = flux_ty::signatures(&hir.items);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(
            diagnostics[0].message,
            "lifetime parameters on traits are not supported yet"
        );
    }
}

#[test]
fn production_declaration_has_profile_without_trait_name_selection() {
    // Read the current production declaration verbatim, selected by syntax kind.
    // The small type declarations supply its dependencies; the full source-graph
    // readiness/semantic regression is a separate integration milestone.
    let parsed = flux_syntax::parse_source_file(include_str!("../../flux-syntax/src/ast.rs"));
    assert!(parsed.errors().is_empty());
    let declarations: Vec<_> = parsed
        .root()
        .children()
        .filter(|n| n.kind() == flux_syntax::SyntaxKind::TRAIT)
        .collect();
    assert_eq!(declarations.len(), 1);
    let source = format!(
        "enum SyntaxKind {{ A }} struct SyntaxNode<'a>(&'a ()); {}",
        declarations[0].text()
    );
    let (hir, sigs, id) = resolve(&source);
    assert_eq!(profile(&hir, &sigs, id), Ok(()));
}

#[test]
fn excluded_declaration_shapes_have_reasons() {
    for (source, reason) in [
        ("trait T {}", Reason::OwnerLifetimes),
        ("trait T<'a,'b> {}", Reason::OwnerLifetimes),
        ("trait T<'a,U> {}", Reason::TraitParameters),
        ("trait T<'a,U=Self> {}", Reason::TraitParameters),
        ("trait T<'a,const N:usize> {}", Reason::TraitParameters),
        ("trait T<'a,const N:usize=1> {}", Reason::TraitParameters),
        ("trait T<'a> where Self: Copy {}", Reason::TraitPredicates),
        ("trait T<'a:'static> {}", Reason::TraitPredicates),
        ("trait T<'a> where Self:'a {}", Reason::TraitPredicates),
        ("trait T<'a> {type Out;}", Reason::AssociatedItems),
        ("trait T<'a> {type Out=Self;}", Reason::AssociatedItems),
        ("trait T<'a> {const C:usize;}", Reason::AssociatedItems),
        ("unsafe trait T<'a> {}", Reason::UnsafeTrait),
        ("trait T<'a> {fn f() {}}", Reason::MethodShape),
        ("trait T<'a> {unsafe fn f();}", Reason::MethodShape),
        ("trait T<'a> {fn f<'b>();}", Reason::MethodGenerics),
        ("trait T<'a> {fn f<U>();}", Reason::MethodGenerics),
        (
            "trait T<'a> {fn f<const N:usize>();}",
            Reason::MethodGenerics,
        ),
        (
            "trait T<'a> {fn f() where Self:Sized;}",
            Reason::MethodGenerics,
        ),
        (
            "trait T<'a> {fn f() where 'a:'static;}",
            Reason::MethodGenerics,
        ),
        (
            "trait T<'a> {fn f() where Self:'a;}",
            Reason::MethodGenerics,
        ),
        ("trait T<'a> {fn f(x:impl Copy);}", Reason::MethodGenerics),
        ("trait T<'a> {fn f()->impl Copy;}", Reason::SignatureType),
        (
            "trait T<'a> {async fn f();}",
            Reason::DeclarationDiagnostics,
        ),
        (
            "trait T<'a> {extern \"C\" fn f();}",
            Reason::DeclarationDiagnostics,
        ),
        ("trait T<'a> {fn f(x:fn(&i32));}", Reason::SignatureType),
        ("trait T<'a> {fn f(x:&dyn Copy);}", Reason::SignatureType),
        ("trait T<'a> {fn f(x:_);}", Reason::SignatureType),
        (
            "trait T<'a> {fn f()-><Self as Iterator>::Item;}",
            Reason::SignatureType,
        ),
    ] {
        let (hir, sigs, id) = resolve(source);
        assert_eq!(
            profile(&hir, &sigs, id).unwrap_err().reason,
            reason,
            "{source}"
        );
    }
}

#[test]
fn region_free_supertraits_are_not_required_method_traits() {
    for source in [
        "trait T<'a>:Copy {}",
        "trait T<'a>:A {} trait A:B {fn default(&self) {}} trait B:Sized {}",
        "trait T<'a>:A<u32> {} trait A<U> {fn generic<V>(&self,x:V) {}}",
        "trait T<'a>:A {} trait A<U=u32> {}",
        "trait T<'a>:A<u32> {} trait A<U:Copy> {}",
        "trait T<'a>:A {} trait A where Self:Copy {}",
        "trait T<'a>:A {} trait A {type Out:Copy;}",
        "trait T<'a>:A+B {} trait A:C {} trait B:C {} trait C {}",
    ] {
        let (hir, sigs, id) = resolve(source);
        assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
        assert_eq!(profile(&hir, &sigs, id), Ok(()), "{source}");
    }
}

#[test]
fn excluded_supertrait_edges_fail_transitively() {
    for source in [
        "trait T<'a>:A<'a> {} trait A<'b> {}",
        "trait T<'a>:A<'static> {} trait A<'b> {}",
        "trait T<'a>:A {} trait A<'b> {}",
        "trait T<'a>:A<&'a i32> {} trait A<U> {}",
        "trait T<'a>:A {} trait A:B<'static> {} trait B<'b> {}",
        "trait T<'a>:A {} trait A:B<&'static i32> {} trait B<U> {}",
        "trait T<'a>:A {} trait A:B {} trait B<U=&'static i32> {}",
        "trait T<'a>:A<Item=u32> {} trait A {type Item;}",
        "trait T<'a>:A {} trait A:B<Item=u32> {} trait B {type Item;}",
        "trait T<'a>:A {} trait A {type Out:B<'static>;} trait B<'b> {}",
        "trait T<'a>:A<u32> {} trait A<U:B<'static>> {} trait B<'b> {}",
        "trait T<'a>:A<u32> {} trait A<U> where U:'static {}",
        "struct Node<'a>(&'a i32); trait T<'a>:A<Node<'a>> {} trait A<U> {}",
        "struct Node<'a>(&'a i32); trait T<'a>:A<Node> {} trait A<U> {}",
    ] {
        let (hir, sigs, id) = resolve(source);
        assert_eq!(
            profile(&hir, &sigs, id).unwrap_err().reason,
            Reason::SupertraitRegions,
            "{source}"
        );
    }
}

#[test]
fn corrupted_owner_and_method_tables_fail_closed() {
    let (hir, sigs, id) = resolve(SHAPE);
    let method = hir.items.traits[id].functions[1];
    for corruption in 0..29 {
        let mut sigs = sigs.clone();
        match corruption {
            0 => sigs.regions.traits = Default::default(),
            1 => sigs.regions.fns = Default::default(),
            2 => sigs.fns = Default::default(),
            3 => sigs.regions.traits.get_mut(id).unwrap().names = Box::default(),
            4 => sigs.regions.traits.get_mut(id).unwrap().names[0].name = None,
            5 => sigs.regions.fns.get_mut(method).unwrap().owner = 0,
            6 => sigs.regions.fns.get_mut(method).unwrap().owner = 2,
            7 => sigs.regions.fns.get_mut(method).unwrap().names[0].name = None,
            8 => sigs.regions.fns.get_mut(method).unwrap().names = Box::default(),
            9 => sigs.regions.fns.get_mut(method).unwrap().params = Box::default(),
            10 => sigs.regions.fns.get_mut(method).unwrap().params[0] = Box::default(),
            11 => sigs.regions.fns.get_mut(method).unwrap().params[0][0] = Region::Erased,
            12 => sigs.regions.fns.get_mut(method).unwrap().params[0][0] = Region::Late(0),
            13 => sigs.regions.fns.get_mut(method).unwrap().params[0][0] = Region::Param(1),
            14 => sigs.regions.fns.get_mut(method).unwrap().params[0][0] = Region::Static,
            15 => sigs.regions.fns.get_mut(method).unwrap().ret = Box::new([Region::Param(0)]),
            16 => sigs.regions.fns.get_mut(method).unwrap().has_bounds = true,
            17 => {
                sigs.regions.fns.get_mut(method).unwrap().bound_selfs = Box::new([Box::default()])
            }
            18 => {
                sigs.regions.fns.get_mut(method).unwrap().bound_traits =
                    Box::new([Default::default()])
            }
            19 => {
                sigs.regions.fns.get_mut(method).unwrap().projections = Box::new([Box::default()])
            }
            20 => {
                sigs.regions.fns.get_mut(method).unwrap().opaques = Box::new([Default::default()])
            }
            21 => {
                sigs.regions.fns.get_mut(method).unwrap().names =
                    vec![sigs.regions.traits[id].names[0]; 2].into()
            }
            22 => sigs.fns.get_mut(method).unwrap().params = Box::default(),
            23 => sigs.fns.get_mut(method).unwrap().params[0] = Ty::ERROR,
            24 => sigs.fns.get_mut(method).unwrap().params[0] = Ty::BOOL,
            25 => sigs.fns.get_mut(method).unwrap().ret = Ty::BOOL,
            26 => {
                sigs.regions.fns.get_mut(method).unwrap().outlives =
                    Box::new([flux_ty::regions::Outlives::Region(
                        Region::Param(0),
                        Region::Param(99),
                    )])
            }
            27 => {
                sigs.regions.traits.get_mut(id).unwrap().outlives =
                    Box::new([flux_ty::regions::Outlives::Region(
                        Region::Param(0),
                        Region::Static,
                    )])
            }
            _ => sigs.regions.fns.get_mut(method).unwrap().names[0].range = Default::default(),
        }
        assert!(profile(&hir, &sigs, id).is_err(), "corruption {corruption}");
    }
}

#[test]
fn corrupted_method_membership_and_supertrait_metadata_fail_closed() {
    let (_, sigs, id) = resolve(SHAPE);
    for corruption in 0..12 {
        let (mut hir, _, _) = resolve(SHAPE);
        let mut sigs = sigs.clone();
        let method = hir.items.traits[id].functions[0];
        let edge = sigs.traits[id].supertraits[0].trait_;
        match corruption {
            0 => hir.items.traits[id].functions.clear(),
            1 => hir.items.traits[id].functions.push(method),
            2 => hir.items.functions[method].owner = None,
            3 => sigs.traits = Default::default(),
            4 => sigs.trait_predicates = Default::default(),
            5 => sigs.trait_defaults = Default::default(),
            6 => sigs.traits.get_mut(id).unwrap().supertraits = Box::default(),
            7 => sigs.traits.get_mut(id).unwrap().supertraits[0].trait_ = id,
            8 => sigs.traits.get_mut(id).unwrap().supertraits[0].args = &[Ty::BOOL],
            9 => {
                sigs.regions.traits.get_mut(edge).unwrap().names =
                    sigs.regions.traits[id].names.clone()
            }
            10 => sigs.traits.get_mut(edge).unwrap().supertraits = Box::default(),
            _ => sigs.traits.get_mut(id).unwrap().defaults = Box::new([Some(Ty::BOOL)]),
        }
        assert!(profile(&hir, &sigs, id).is_err(), "corruption {corruption}");
    }
}

#[test]
fn supertrait_cycles_reject_without_panicking() {
    // Cycle-bearing declarations are compile-only invalid examples.
    for source in [
        "trait T<'a>:A {} trait A:A {}",
        "trait T<'a>:A {} trait A:B {} trait B:A {}",
    ] {
        let (hir, sigs, id) = resolve(source);
        assert_eq!(
            profile(&hir, &sigs, id).unwrap_err().reason,
            Reason::SupertraitCycle
        );
        assert!(sigs.cyclic);
        assert!(
            hir.items
                .functions
                .values()
                .all(|f| f.owner != Some(Owner::Trait(id)))
        );
    }
}

#[test]
fn method_qualifiers_and_missing_adt_metadata_fail_closed() {
    for flag in 0..5 {
        let (mut hir, sigs, id) = resolve(SHAPE);
        let method = hir.items.traits[id].functions[0];
        let f = &mut hir.items.functions[method];
        match flag {
            0 => f.is_const = true,
            1 => f.is_extern = true,
            2 => f.is_variadic = true,
            3 => f.intrinsic = true,
            _ => f.has_body = true,
        }
        assert_eq!(
            profile(&hir, &sigs, id).unwrap_err().reason,
            Reason::MethodShape
        );
    }
    let (hir, mut sigs, id) =
        resolve("struct Node<'a>(&'a i32); trait T<'a> { fn f(x:Node<'a>); }");
    sigs.regions.structs = Default::default();
    assert!(profile(&hir, &sigs, id).is_err());
}

#[test]
fn in_range_but_wrong_binder_and_elided_name_arities_reject() {
    let (hir, sigs, id) = resolve("trait T<'a> { fn f(&self,x:&'a i32)->&'a i32; }");
    let method = hir.items.traits[id].functions[0];
    assert_eq!(sigs.regions.fns[method].names.len(), 2);
    assert_eq!(profile(&hir, &sigs, id), Ok(()));
    for corruption in 0..5 {
        let mut sigs = sigs.clone();
        let r = sigs.regions.fns.get_mut(method).unwrap();
        match corruption {
            0 => r.params[1][0] = Region::Param(1),
            1 => r.ret[0] = Region::Param(1),
            2 => r.ret[0] = Region::Erased,
            3 => r.names = r.names[..1].into(),
            _ => r.names[1].name = r.names[0].name,
        }
        assert!(profile(&hir, &sigs, id).is_err(), "corruption {corruption}");
    }
}

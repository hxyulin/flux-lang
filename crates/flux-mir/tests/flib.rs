use flux_hir::flib::{Decode, Decoder, Encode, Encoder};

#[test]
fn checked_generic_mir_round_trip() {
    let (_db, _, hir) = flux_hir::lower_text(
        "flib.rs",
        "fn id<T>(x: T) -> T { x } fn main() { let x = id(42); }",
    );
    let typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let mir = flux_mir::build_file(&hir, &typed);
    assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
    for (id, body) in mir
        .bodies
        .iter()
        .filter(|(id, _)| !hir.items.is_core(hir.items.functions[*id].loc.module))
    {
        let mut e = Encoder::new();
        body.encode(&mut e).unwrap();
        let bytes = e.into_bytes();
        let mut d = Decoder::new(&bytes);
        let copy = flux_mir::Body::decode(&mut d).unwrap();
        d.finish().unwrap();
        assert_eq!(body.locals.len(), copy.locals.len(), "function {id:?}");
        assert_eq!(body.blocks.len(), copy.blocks.len(), "function {id:?}");
        let mut e = Encoder::new();
        copy.encode(&mut e).unwrap();
        assert_eq!(bytes, e.into_bytes(), "function {id:?}");
        for n in 0..bytes.len() {
            assert!(
                flux_mir::Body::decode(&mut Decoder::new(&bytes[..n])).is_err(),
                "function {id:?}, prefix {n}"
            );
        }
    }
}

fn empty_mir() -> flux_mir::FileMir {
    flux_mir::FileMir {
        panic_strategy: flux_mir::PanicStrategy::Abort,
        bodies: Default::default(),
        closures: Default::default(),
        consts: Default::default(),
        const_values: Default::default(),
        discriminants: Default::default(),
        diagnostics: Vec::new(),
        bugs: Vec::new(),
    }
}

#[test]
fn checked_dependency_provider_seeds_mir_without_hir() {
    let (_db, _, hir) = flux_hir::lower_text(
        "dependency.rs",
        "pub fn id<T>(x: T) -> T { x } pub const N: usize = 3;",
    );
    let typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let mir = flux_mir::build_file(&hir, &typed);
    assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
    let provider = flux_mir::imported::ImportedMir::new(mir, hir.items.counts()).unwrap();
    let mut seeded = empty_mir();
    provider.seed_into(&mut seeded).unwrap();
    assert_eq!(
        provider.mir().bodies.iter().count(),
        seeded.bodies.iter().count()
    );
    assert_eq!(
        provider.mir().consts.iter().count(),
        seeded.consts.iter().count()
    );
    let before = seeded.bodies.iter().count();
    assert!(provider.seed_into(&mut seeded).is_err());
    assert_eq!(before, seeded.bodies.iter().count());
    assert!(flux_mir::imported::ImportedMir::new(seeded, Default::default()).is_err());
}

#[test]
fn dependency_values_and_discriminants_do_not_need_initializer_mir() {
    let (_db, _, hir) = flux_hir::lower_text(
        "values.rs",
        "pub const N: u32 = 3; pub enum Tag { A = 1 << 2, B = 9 }",
    );
    let typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let mir = flux_mir::build_file(&hir, &typed);
    assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
    let own = hir.items.main_crate();
    let constant = hir
        .items
        .consts
        .iter()
        .find(|(_, c)| {
            hir.items.modules[c.loc.module].krate == own
                && c.name.is_some_and(|n| n.as_str() == "N")
        })
        .unwrap()
        .0;
    let tag = hir
        .items
        .structs
        .iter()
        .find(|(_, s)| hir.items.modules[s.loc.module].krate == own)
        .unwrap()
        .0;
    let functions = Default::default();
    let consts = Default::default();
    let names = |id| hir.items.const_name(id);
    let mut evaluator =
        flux_mir::eval::Evaluator::new(&hir.items, &typed.sigs, &functions, &consts, &names)
            .with_imported(&mir.const_values, &mir.discriminants);
    assert_eq!(
        evaluator.eval_const(constant).unwrap(),
        flux_mir::eval::Value::Int {
            bits: 3,
            ty: flux_ty::IntTy::U32
        }
    );
    assert_eq!(evaluator.discriminants(tag).unwrap().as_ref(), &[4, 9]);
    assert!(evaluator.unbuilt.is_empty());
}

#[test]
fn inline_constants_keep_checked_bodies_in_metadata() {
    let (_, _, hir) = flux_hir::lower_text(
        "inline.rs",
        "const fn twice(x:u32)->u32{x*2}pub fn values<T>()->&'static[u32]{const{&[twice(7),twice(11)]}}pub fn unused<T>()->u8{const{panic!(\"deferred\")}}fn main(){}",
    );
    let typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let mir = flux_mir::build_file(&hir, &typed);
    assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
    for name in ["values", "unused"] {
        let (id, _) = hir
            .items
            .functions
            .iter()
            .find(|(_, f)| {
                f.name.as_str() == name
                    && f.loc.module == hir.items.module_of(hir.items.main_crate())
            })
            .unwrap();
        let body = &mir.bodies[id];
        assert_eq!(body.inline_constants.len(), 1, "{name}");
        assert!(body.inline_constants[0].owner.is_none());
        assert_eq!(body.inline_constants[0].param_count, 0);
        let mut encoder = Encoder::new();
        body.encode(&mut encoder).unwrap();
        let bytes = encoder.into_bytes();
        let mut decoder = Decoder::new(&bytes);
        let copied = flux_mir::Body::decode(&mut decoder).unwrap();
        decoder.finish().unwrap();
        let mut encoder = Encoder::new();
        copied.encode(&mut encoder).unwrap();
        assert_eq!(bytes, encoder.into_bytes(), "{name}");
    }
}

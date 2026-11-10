//! Strategy selection survives syntax and MIR metadata without mixing modes.
use flux_hir::flib::{Decode, Decoder, Encode, Encoder, IdKind};
use flux_mir::{FileMir, PanicStrategy};

#[test]
fn syntax_flags_round_trip_both_modes_and_keep_abort_bytes() {
    for strategy in [PanicStrategy::Abort, PanicStrategy::Unwind] {
        for debug_assertions in [false, true] {
            let cfg = flux_syntax::CfgOptions {
                panic_strategy: strategy,
                debug_assertions,
                ..Default::default()
            };
            let mut e = Encoder::new();
            cfg.encode(&mut e).unwrap();
            let bytes = e.into_bytes();
            assert_eq!(bytes.len(), 9);
            assert_eq!(&bytes[..8], &[0; 8]);
            assert_eq!(
                bytes[8],
                u8::from(debug_assertions)
                    | if strategy == PanicStrategy::Unwind {
                        2
                    } else {
                        0
                    }
            );
            assert_eq!(
                flux_syntax::CfgOptions::decode(&mut Decoder::new(&bytes)).unwrap(),
                cfg
            );
        }
    }
}

#[test]
fn configured_cfg_panic_matches_the_selected_strategy() {
    for strategy in [PanicStrategy::Abort, PanicStrategy::Unwind] {
        let mut db = flux_hir::SourceDb::new();
        db.set_panic_strategy(strategy);
        let root=db.add("cfg_strategy.rs", "#[cfg(panic=\"abort\")]fn abort_only(){} #[cfg(panic=\"unwind\")]fn unwind_only(){} fn query()->bool{cfg!(panic=\"unwind\")}");
        db.set_crate_cfg(root, Default::default()).unwrap();
        let hir = flux_hir::lower_sources(
            &mut db,
            &[flux_hir::SourceCrate {
                root,
                name: flux_intern::Symbol::intern("strategy"),
                dependencies: vec![],
            }],
        );
        assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
        let names: Vec<_> = hir
            .items
            .functions
            .iter()
            .filter(|(id, _)| hir.is_program(*id))
            .map(|(_, f)| f.name.as_str())
            .collect();
        assert_eq!(
            names.contains(&"unwind_only"),
            strategy == PanicStrategy::Unwind
        );
        assert_eq!(
            names.contains(&"abort_only"),
            strategy == PanicStrategy::Abort
        );
        let typed = flux_mir::check_file(&hir);
        assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
        let mir = flux_mir::build_file_with_panic_strategy(&hir, &typed, strategy);
        assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
        let (_, body) = mir
            .bodies
            .iter()
            .find(|(id, _)| {
                hir.is_program(*id) && hir.items.functions[*id].name.as_str() == "query"
            })
            .unwrap();
        assert!(body.blocks.values().flat_map(|b|&b.statements).any(|s|matches!(s.kind,flux_mir::StatementKind::Assign(_,flux_mir::Rvalue::Use(flux_mir::Operand::Const(flux_mir::Constant::Bool(value)))) if value==(strategy==PanicStrategy::Unwind))));
    }
}

#[test]
fn mir_round_trips_with_a_strategy_and_rejects_mixed_providers() {
    let (_, _, hir) = flux_hir::lower_text(
        "strategy.rs",
        "struct Owned; impl Drop for Owned{fn drop(&mut self){}} fn called(){} pub fn outer(){let _value=Owned;called();}",
    );
    let typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    for strategy in [PanicStrategy::Abort, PanicStrategy::Unwind] {
        let mir = flux_mir::build_file_with_panic_strategy(&hir, &typed, strategy);
        assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
        let mut e = Encoder::new();
        for (id, _) in mir.bodies.iter().filter(|(id, _)| hir.is_program(*id)) {
            e.ids.mark_owned(IdKind::Function, id.into_raw());
        }
        mir.encode_flib(&mut e).unwrap();
        let bytes = e.into_bytes();
        assert_eq!(
            bytes.starts_with(&u32::MAX.to_le_bytes()),
            strategy == PanicStrategy::Unwind
        );
        let mut decoded = FileMir {
            panic_strategy: strategy,
            ..Default::default()
        };
        let mut d = Decoder::new(&bytes);
        decoded
            .decode_flib_into(&mut d, &hir.items, &typed.sigs)
            .unwrap();
        d.finish().unwrap();
        assert_eq!(decoded.panic_strategy, strategy);
        assert_eq!(decoded.bodies.iter().count(), 3);
        assert_eq!(
            decoded.bodies.iter().any(|(_, body)| body.has_unwind()),
            strategy == PanicStrategy::Unwind
        );
        let other = if strategy == PanicStrategy::Abort {
            PanicStrategy::Unwind
        } else {
            PanicStrategy::Abort
        };
        let mut wrong = FileMir {
            panic_strategy: other,
            ..Default::default()
        };
        assert!(
            wrong
                .decode_flib_into(&mut Decoder::new(&bytes), &hir.items, &typed.sigs)
                .unwrap_err()
                .to_string()
                .contains("panic strategy mismatch")
        );
        assert_eq!(wrong.bodies.iter().count(), 0);
        let imported = flux_mir::imported::ImportedMir::new(decoded, hir.items.counts()).unwrap();
        assert!(imported.seed_into(&mut wrong).is_err());
        let rejected =
            flux_mir::build_file_from_with_panic_strategy(&hir, &typed, &imported, &[], other);
        assert_eq!(rejected.diagnostics.len(), 1);
        assert!(
            rejected.diagnostics[0]
                .message
                .contains("panic strategy mismatch")
        );
        let mut seeded = FileMir {
            panic_strategy: strategy,
            ..Default::default()
        };
        imported.seed_into(&mut seeded).unwrap();
        assert_eq!(seeded.bodies.iter().count(), 3);
        for n in 0..bytes.len() {
            let mut truncated = FileMir {
                panic_strategy: strategy,
                ..Default::default()
            };
            assert!(
                truncated
                    .decode_flib_into(&mut Decoder::new(&bytes[..n]), &hir.items, &typed.sigs)
                    .is_err(),
                "prefix {n}"
            );
        }
    }
}

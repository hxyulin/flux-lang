//! Both definitions are checked, but only the selected instance reaches ABI collection.
#[test]
fn selected_helpers_are_collected() {
    for (source, sized, fallback) in [
        (
            "fn main(){let b=Box::new(0..3u32);assert_eq!(b.last(),Some(2));}",
            true,
            false,
        ),
        (
            "fn main(){let b:Box<dyn Iterator<Item=u32>>=Box::new(0..3u32);assert_eq!(b.last(),Some(2));}",
            false,
            true,
        ),
        (
            "fn main(){let b:Box<dyn Iterator<Item=u32>>=Box::new(0..3u32);assert_eq!(Box::new(b).last(),Some(2));}",
            true,
            true,
        ),
    ] {
        let (_, _, hir) = flux_hir::lower_text("dispatch.rs", source);
        assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
        let mir = flux_mir::build_file(&hir, &typed);
        assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
        let collected = flux_mir::mono::collect(&hir, &typed.sigs, &mir).unwrap();
        assert_eq!(
            collected
                .iter()
                .any(|i| hir.items.functions[i.function].name.as_str() == "__boxed_last_sized"),
            sized
        );
        assert_eq!(
            collected
                .iter()
                .any(|i| hir.items.functions[i.function].name.as_str() == "__boxed_last_fallback"),
            fallback
        );
        for instance in collected {
            let body = flux_mir::mono::body_of(&hir, &typed.sigs, &mir, instance);
            assert!(!body.blocks.values().any(|block|matches!(block.terminator.kind,flux_mir::TerminatorKind::Call{func,..}if hir.items.functions[func].intrinsic==Some(flux_hir::Intrinsic::SizedDispatch))));
        }
    }
}

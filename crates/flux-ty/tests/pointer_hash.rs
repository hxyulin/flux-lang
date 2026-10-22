//! The supported pointer hash API is deliberately restricted to thin
//! pointers until metadata hashing is implemented.
#[test]
fn rejects_unsized_pointer_hashing() {
    let (_, _, hir) = flux_hir::lower_text(
        "main.rs",
        "fn main() {\n\
         let mut h = core::hash::BuildHasher::build_hasher(\n\
             &core::hash::BuildHasherDefault::<H>::default());\n\
         core::ptr::hash::<[u8], H>(&[1u8][..] as *const [u8], &mut h);\n\
         }\n\
         #[derive(Default)] struct H;\n\
         impl core::hash::Hasher for H {\n\
             fn finish(&self) -> u64 { 0 }\n\
             fn write(&mut self, _: &[u8]) {}\n\
         }",
    );
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let typed = flux_ty::check_file(&hir);
    assert!(
        typed
            .diagnostics
            .iter()
            .any(|d| d.message.contains("[u8]") && d.message.contains("size")),
        "{:?}",
        typed.diagnostics,
    );
}

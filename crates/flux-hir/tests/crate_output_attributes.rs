use flux_hir::{SourceDb, lower};
use flux_intern::Symbol;

#[test]
fn explicit_output_overrides_only_valid_crate_types() {
    for (attribute, supported, valid) in [
        (
            "#![crate_type=\"rlib\"]\n#![crate_type=\"dylib\"]",
            false,
            true,
        ),
        ("#![crate_type=\"staticlib\"]", false, true),
        ("#![crate_type=\"cdylib\"]", false, true),
        ("#![crate_type=\"proc-macro\"]", false, true),
        ("#![crate_type=\"rlib\"]", true, true),
        ("#![crate_type=\"unknown\"]", false, false),
        ("#![crate_type=42]", false, false),
        ("#![crate_type(\"rlib\")]", false, false),
        ("#![unsafe(crate_type=\"rlib\")]", false, false),
    ] {
        for explicit in [false, true] {
            let mut db = SourceDb::new();
            let root = db.add(
                "output.rs",
                format!("{attribute}\npub fn value() -> u8 {{ 42 }}"),
            );
            if explicit {
                db.override_crate_output(root).unwrap();
            }
            // Driver state survives the fork used by built-in source snapshots.
            let mut db = db.fork();
            let hir = lower(&mut db, root, Symbol::intern("output"));
            assert_eq!(
                hir.diagnostics.is_empty(),
                if explicit { valid } else { supported },
                "{attribute}, explicit={explicit}: {:?}",
                hir.diagnostics
            );
        }
    }
}

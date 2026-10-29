//! Source identifier restrictions enforced by the command-line compiler.
//!
//! HIR currently interns source spellings verbatim. Proc-macro identifiers use
//! NFC, so accepting decomposed source names can change which binding an output
//! token resolves to. Keep this guard in the driver until semantic name paths
//! normalize consistently, without adding the host token runtime to HIR's graph.

use flux_hir::{Diagnostic, SourceDb};
use flux_syntax::SyntaxKind;

/// Reports unsupported non-NFC identifier spellings in physical source files.
/// String contents, documentation and normalized macro output are unaffected.
pub fn diagnostics(db: &SourceDb) -> Vec<Diagnostic> {
    let mut errors = Vec::new();
    for (id, file) in db.files() {
        if file.expansion_of.is_some() || file.text().is_ascii() {
            continue;
        }
        for token in file.parse.root().tokens() {
            if matches!(
                token.lexical_kind(),
                SyntaxKind::IDENT | SyntaxKind::LIFETIME_IDENT
            ) && !flux_proc_macro::is_identifier_normalized(token.text())
            {
                let mut diagnostic = Diagnostic {
                    file: id,
                    range: token.text_range(),
                    message: "non-nfc identifier spellings are not supported yet".into(),
                };
                db.resolve_diagnostic(&mut diagnostic);
                errors.push(diagnostic);
            }
        }
    }
    errors
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_nfc_source_identifiers_report_a_boundary() {
        let mut db = SourceDb::new();
        let source = "fn e\u{301}<'e\u{301}>() { let r#e\u{301} = 1; }";
        let file = db.add("unicode.rs", source);
        let errors = diagnostics(&db);
        assert_eq!(errors.len(), 3);
        for error in errors {
            assert_eq!(error.file, file);
            assert_eq!(
                error.message,
                "non-nfc identifier spellings are not supported yet"
            );
            assert!(source[error.range.start() as usize..error.range.end() as usize].contains('e'));
        }
    }

    #[test]
    fn normalized_identifiers_and_arbitrary_literal_text_remain_accepted() {
        let mut db = SourceDb::new();
        db.add(
            "unicode.rs",
            "/// e\u{301}\nfn café() { let a\u{327} = \"e\u{301}\"; let _ = a\u{327}; }",
        );
        assert!(diagnostics(&db).is_empty());
    }
}

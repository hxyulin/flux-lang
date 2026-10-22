//! Snapshot tests for MIR building and the initialization check.
//!
//! Every `tests/data/ok/*.rs` must build without diagnostics (and compile
//! with rustc), and every `err/*.rs` must report at least one; each has a
//! `.mir` file next to it with the MIR and diagnostics. Run with
//! `UPDATE_EXPECT=1` to write missing or changed snapshots, then review
//! the diff.

use std::fs;
use std::path::{Path, PathBuf};

fn fixtures(dir: &str) -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data")
        .join(dir);
    let mut files: Vec<_> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "rs"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no fixtures in {}", dir.display());
    files
}

fn check_dir(name: &str, expect_errors: bool) {
    let update = std::env::var_os("UPDATE_EXPECT").is_some();
    let mut failures = Vec::new();
    for path in fixtures(name) {
        let text = fs::read_to_string(&path).unwrap();
        let (db, root, hir) = flux_hir::lower_text(&path, &text);
        let parse = &db.file(root).parse;
        let typed = flux_mir::check_file(&hir);
        assert!(
            parse.errors().is_empty() && hir.diagnostics.is_empty() && typed.diagnostics.is_empty(),
            "{} must parse, lower and type-check cleanly:\n{:#?}\n{:#?}\n{:#?}",
            path.display(),
            parse.errors(),
            hir.diagnostics,
            typed.diagnostics
        );
        let mir = flux_mir::build_file(&hir, &typed);
        let actual = mir.debug_dump(&hir);
        if mir.diagnostics.is_empty() == expect_errors {
            failures.push(format!(
                "{}: expected {}diagnostics\n{actual}",
                path.display(),
                if expect_errors { "" } else { "no " },
            ));
        }
        let snapshot = path.with_extension("mir");
        match fs::read_to_string(&snapshot) {
            Ok(expected) if expected == actual => {}
            _ if update => fs::write(&snapshot, &actual).unwrap(),
            Ok(_) => failures.push(format!(
                "{} differs from {} (rerun with UPDATE_EXPECT=1 to update)",
                path.display(),
                snapshot.display()
            )),
            Err(_) => failures.push(format!(
                "{} is missing (rerun with UPDATE_EXPECT=1 to create it)",
                snapshot.display()
            )),
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n\n"));
}

#[test]
fn ok_fixtures() {
    check_dir("ok", false);
}

#[test]
fn err_fixtures() {
    check_dir("err", true);
}

/// The errors type checking reports for `text`, as `line:column:
/// message`.
fn type_errors(text: &str) -> Vec<String> {
    let (_db, _, hir) = flux_hir::lower_text("main.rs", text);
    assert!(hir.diagnostics.is_empty(), "{:#?}", hir.diagnostics);
    let typed = flux_mir::check_file(&hir);
    typed
        .diagnostics
        .iter()
        .map(|d| {
            let before = &text[..d.range.start() as usize];
            let line = before.matches('\n').count() + 1;
            let column = before.len() - before.rfind('\n').map_or(0, |i| i + 1) + 1;
            format!("{line}:{column}: {}", d.message)
        })
        .collect()
}

/// Array lengths that name constants need their values while types are
/// checked, so their evaluation errors stop type checking, as in rustc.
#[test]
fn array_length_errors() {
    let text = "\
const fn half(n: usize) -> usize {
    n / 0
}
const N: usize = half(4);
fn f(_: [u8; N]) {}
fn main() {}
";
    assert_eq!(
        type_errors(text),
        ["4:18: attempt to divide `4_usize` by zero"]
    );
    let text = "\
const A: usize = B;
const B: usize = A;
fn f(_: [u8; A]) {}
fn main() {}
";
    assert_eq!(
        type_errors(text),
        [
            "1:18: cycle detected when const-evaluating `A`",
            "2:18: cycle detected when const-evaluating `A`"
        ]
    );
    // A length whose constant's own value names it.
    let text = "\
const A: usize = [0u8; A].len();
fn main() {
    let _: [u8; A] = [];
}
";
    assert_eq!(
        type_errors(text),
        ["1:1: cycle detected when evaluating type-level constant `A`"]
    );
}

/// The messages of the errors in `text` as `flux mir` finds them where
/// lowering or type checking finds some: with the initialization and
/// borrow errors of the bodies that have none.
fn program_errors(text: &str) -> Vec<String> {
    let (_db, _, hir) = flux_hir::lower_text("main.rs", text);
    let typed = flux_mir::check_file(&hir);
    let mut errors = hir.diagnostics.clone();
    errors.extend(typed.diagnostics.iter().cloned());
    assert!(!errors.is_empty());
    let more = flux_mir::check_clean_bodies(&hir, &typed, &errors);
    errors.extend(more);
    errors.into_iter().map(|d| d.message).collect()
}

/// Bodies without errors of their own may name what an import that
/// failed should have brought, which MIR building cannot lower (and
/// which rustc does not report again): a length whose constant names
/// it is unknown, and a call of it is never compiled.
#[test]
fn names_of_failed_imports() {
    let text = "\
use missing::U8;
type IvLen = U8;
const IV_LEN: usize = IvLen::USIZE;
fn wrap(iv: [u8; IV_LEN]) -> usize {
    iv.len()
}
fn main() {
    let _ = wrap;
}
";
    assert_eq!(program_errors(text), ["unresolved import `missing`"]);
    let text = "\
use missing::Vec;
type VecType = Vec;
struct Big {
    data: VecType,
}
fn new() -> Big {
    Big {
        data: VecType::new(),
    }
}
fn main() {
    let _ = new;
}
";
    assert_eq!(program_errors(text), ["unresolved import `missing`"]);
    let text = "\
use missing::Thing;
fn src() -> Thing {
    loop {}
}
fn count(from: usize, condition: fn(char) -> bool) -> usize {
    src()[from..].find(|c| !condition(c)).unwrap_or(0)
}
fn main() {
    let _ = count;
}
";
    assert_eq!(program_errors(text), ["unresolved import `missing`"]);
}

#[path = "../../flux-syntax/tests/support/corpus.rs"]
mod corpus;

/// Every file under `FLUX_CORPUS` that lowers and type-checks cleanly as
/// a program builds MIR in which every value has the type of the place
/// it is stored in ([`flux_mir::validate`]), and without lowering
/// anything to `unreachable` because type checking should have ruled it
/// out ([`flux_mir::FileMir::bugs`], each a silent miscompilation if
/// built), checked here in release builds too.
#[test]
#[ignore = "needs FLUX_CORPUS"]
fn corpus_mir_is_well_typed() {
    const BUDGET: corpus::Budget = corpus::Budget::new(500, 2000.0);
    let results = corpus::for_each_corpus_file("built", BUDGET, |path, text| {
        let (db, root, hir) = flux_hir::lower_text(path, text);
        // The bytes of it and its modules' files.
        let bytes = (db.files())
            .filter(|&(id, _)| id >= root)
            .map(|(_, file)| file.text().len())
            .sum();
        if !hir.diagnostics.is_empty() {
            return (bytes, None);
        }
        let typed = flux_mir::check_file(&hir);
        if !typed.diagnostics.is_empty() {
            return (bytes, None);
        }
        let mir = flux_mir::build_file(&hir, &typed);
        let mut errors = flux_mir::validate::validate_file(&hir, &typed.sigs, &mir);
        errors.extend(mir.bugs.iter().map(|d| {
            let path = db.file(d.file).path.display();
            format!("{path} at {:?}: {}", d.range, d.message)
        }));
        (bytes, Some(errors))
    });
    let built = results.iter().filter(|(_, r)| r.is_some()).count();
    let failures: Vec<String> = results
        .iter()
        .filter_map(|(path, r)| {
            let errors = r.as_ref().filter(|e| !e.is_empty())?;
            Some(format!("{}:\n  {}", path.display(), errors.join("\n  ")))
        })
        .collect();
    println!("{built} files built MIR");
    assert!(
        failures.is_empty(),
        "ill-typed MIR, or code type checking should have ruled out:\n{}",
        failures.join("\n")
    );
}

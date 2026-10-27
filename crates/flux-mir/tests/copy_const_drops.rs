//! Copy proofs exclude destructors without requiring concrete field layouts.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;

#[test]
fn copy_const_drops_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-copy-const-drops-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let prefix = r#"
use core::marker::PhantomData;
trait Header { type Endian: Copy; }
#[derive(Clone,Copy)] struct File;
impl Header for File { type Endian=u8; }
#[derive(Clone,Copy)]
struct Table<H:Header,R:Copy>{endian:H::Endian,marker:PhantomData<R>}
"#;
    for (name, declaration, accepted) in [
        (
            "variant_constructor",
            "enum Any<R:Copy>{One((Table<File,R>,PhantomData<R>))}",
            true,
        ),
        (
            "generic_copy_value",
            "const fn consume<T:Copy>(value:T) { let _copy=value; }",
            true,
        ),
        (
            "projected_struct",
            "const fn consume<R:Copy>(value:Table<File,R>) { let _copy=value; }",
            true,
        ),
        (
            "nested_tuple_array",
            "const fn consume<R:Copy>(value:([(Table<File,R>,PhantomData<R>);2],u8)) { let _copy=value; }",
            true,
        ),
        (
            "generic_copy_projection",
            "const fn consume<H:Header+Copy,R:Copy>(value:Table<H,R>) { let _copy=value; }",
            true,
        ),
        (
            "owned_generic",
            "const fn consume<T>(value:T) { let _copy=value; }",
            false,
        ),
        (
            "owned_tuple",
            "const fn consume<T>(value:(T,PhantomData<T>)) { let _copy=value; }",
            false,
        ),
        (
            "user_destructor",
            "struct Owned(u8);impl Drop for Owned{fn drop(&mut self){}} const fn consume(value:Owned) { let _copy=value; }",
            false,
        ),
        (
            "nested_destructor",
            "struct Owned(u8);impl Drop for Owned{fn drop(&mut self){}} const fn consume(value:[Owned;2]) { let _copy=value; }",
            false,
        ),
        (
            "copy_cannot_hide_destructor",
            "#[derive(Clone,Copy)]struct Owned(u8);impl Drop for Owned{fn drop(&mut self){}}",
            false,
        ),
    ] {
        if !case_filter::selected(name) {
            continue;
        }
        let text = format!("{prefix}{declaration}fn main(){{}}");
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, &text).unwrap();
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
        let (_, _, hir) = flux_hir::lower_text(&path, &text);
        let mut errors: Vec<_> = hir.diagnostics.iter().map(|d| d.message.clone()).collect();
        if errors.is_empty() {
            let typed = flux_mir::check_file(&hir);
            errors.extend(typed.diagnostics.iter().map(|d| d.message.clone()));
            if errors.is_empty() {
                errors.extend(
                    flux_mir::build_file(&hir, &typed)
                        .diagnostics
                        .into_iter()
                        .map(|d| d.message),
                );
            }
        }
        assert_eq!(errors.is_empty(), accepted, "flux {name}: {errors:?}");
    }
    std::fs::remove_dir_all(root).unwrap();
}

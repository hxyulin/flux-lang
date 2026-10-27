//! Pure nested ADT literals own promoted allocations; local, mutable and Drop data do not.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;
#[test]
fn aggregate_literals_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-aggregate-literals-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "nested_const",
            "enum O{X,M(u16)}struct Form{ops:&'static[O]}const TABLE:&[Form]=&[Form{ops:&[O::X,O::M(128)]}];fn main(){assert_eq!(TABLE[0].ops.len(),2);}",
            true,
        ),
        (
            "nested_static",
            "enum O{X,M(u16)}struct Form{ops:&'static[O]}static TABLE:&[Form]=&[Form{ops:&[O::X,O::M(128)]}];fn main(){}",
            true,
        ),
        (
            "record_return",
            "struct S{value:u32}fn get()->&'static S{&S{value:7}}fn main(){assert_eq!(get().value,7);}",
            true,
        ),
        (
            "tuple_constructor",
            "struct S(u32);fn get()->&'static S{&S(7)}fn main(){}",
            true,
        ),
        (
            "enum_payload",
            "enum E{A(u32),B}fn get()->&'static E{&E::A(7)}fn main(){}",
            true,
        ),
        (
            "unit_mixed_enum",
            "enum E{A(u32),B}fn get()->&'static E{&E::B}fn main(){}",
            true,
        ),
        (
            "nested_reference",
            "struct S{value:&'static[u32]}fn get()->&'static S{&S{value:&[7,11]}}fn main(){}",
            true,
        ),
        (
            "runtime_record",
            "struct S{value:u32}fn get(v:u32)->&'static S{&S{value:v}}fn main(){}",
            false,
        ),
        (
            "runtime_payload",
            "enum E{A(u32),B}fn get(v:u32)->&'static E{&E::A(v)}fn main(){}",
            false,
        ),
        (
            "local_reference",
            "struct S<'a>{value:&'a u32}fn get()->&'static S<'static>{let n=7;&S{value:&n}}fn main(){}",
            false,
        ),
        (
            "drop_record",
            "struct S{value:u32}impl Drop for S{fn drop(&mut self){}}fn get()->&'static S{&S{value:7}}fn main(){}",
            false,
        ),
        (
            "drop_payload",
            "enum E{A(u32),B}impl Drop for E{fn drop(&mut self){}}fn get()->&'static E{&E::A(7)}fn main(){}",
            false,
        ),
        (
            "interior_record",
            "use std::cell::Cell;struct S{value:Cell<u32>}fn get()->&'static S{&S{value:Cell::new(7)}}fn main(){}",
            false,
        ),
        (
            "mutable_record",
            "struct S{value:u32}fn get()->&'static mut S{&mut S{value:7}}fn main(){}",
            false,
        ),
    ] {
        if !case_filter::selected(name) {
            continue;
        }
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let oracle = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("oracle.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            oracle.status.success(),
            accepted,
            "{name}: {}",
            String::from_utf8_lossy(&oracle.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text(&path, source);
        let mut errors = hir.diagnostics.clone();
        if errors.is_empty() {
            let typed = flux_mir::check_file(&hir);
            errors.extend(typed.diagnostics.clone());
            if errors.is_empty() {
                errors.extend(flux_mir::build_file(&hir, &typed).diagnostics);
            }
        }
        assert_eq!(errors.is_empty(), accepted, "{name}: {errors:?}");
    }
}

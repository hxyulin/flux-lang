//! Repeated arrays and implicit literal receiver borrows retain their promoted allocation.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;
#[test]
fn repeat_and_autoref_promotions_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-repeat-autoref-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "repeat_enum",
            "#[derive(Clone,Copy)]enum T{A,B(u16)}fn get()->&'static[T]{&[T::B(7);3]}fn main(){}",
            true,
        ),
        (
            "repeat_typed_const",
            "const N:usize=2+1;fn get()->&'static[u16;N]{&[513;N]}fn main(){}",
            true,
        ),
        (
            "repeat_default_const",
            "struct S<const N:usize=3>([u16;N]);fn get()->&'static S{&S([513;3])}fn main(){}",
            true,
        ),
        (
            "empty_iter",
            "fn get()->std::slice::Iter<'static,u32>{[].iter()}fn main(){}",
            true,
        ),
        (
            "empty_generic",
            "fn get<T>()->std::slice::Iter<'static,T>{[].iter()}fn main(){}",
            true,
        ),
        (
            "literal_iter",
            "fn get()->std::slice::Iter<'static,u32>{[7,9].iter()}fn main(){}",
            true,
        ),
        (
            "repeat_iter",
            "fn get()->std::slice::Iter<'static,u16>{[513;3].iter()}fn main(){}",
            true,
        ),
        (
            "returned_record",
            "struct S<'a>{iter:std::slice::Iter<'a,u32>}fn get()->S<'static>{S{iter:[].iter()}}fn main(){}",
            true,
        ),
        (
            "empty_mut_iter",
            "fn get()->std::slice::IterMut<'static,u32>{[].iter_mut()}fn main(){}",
            true,
        ),
        (
            "runtime_repeat",
            "fn get(x:u32)->&'static[u32]{&[x;3]}fn main(){}",
            false,
        ),
        (
            "runtime_iter",
            "fn get(x:u32)->std::slice::Iter<'static,u32>{[x;3].iter()}fn main(){}",
            false,
        ),
        (
            "runtime_method_receiver",
            "fn make()->[u32;3]{[7;3]}fn get()->std::slice::Iter<'static,u32>{make().iter()}fn main(){}",
            false,
        ),
        (
            "drop_iter",
            "struct D;impl Drop for D{fn drop(&mut self){}}fn get()->std::slice::Iter<'static,D>{[D].iter()}fn main(){}",
            false,
        ),
        (
            "cell_iter",
            "use std::cell::Cell;fn get()->std::slice::Iter<'static,Cell<u32>>{[Cell::new(7)].iter()}fn main(){}",
            false,
        ),
        (
            "mut_nonempty",
            "fn get()->std::slice::IterMut<'static,u32>{[7;3].iter_mut()}fn main(){}",
            false,
        ),
        (
            "local_iter",
            "fn get()->std::slice::Iter<'static,u32>{let a=[7;3];a.iter()}fn main(){}",
            false,
        ),
        (
            "borrowed_repeat",
            "fn get(x:&u32)->&'static[&'static u32]{&[x;3]}fn main(){}",
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

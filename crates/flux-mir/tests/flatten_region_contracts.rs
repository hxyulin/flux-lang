//! Associated declaration equalities retain the caller's item lifetimes.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;
#[test]
fn flatten_region_contracts_match_rustc() {
    let root = std::env::temp_dir().join(format!(
        "flux-flatten-region-contracts-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "flatten_field",
            "struct F<'a>{iter:core::iter::Flatten<core::slice::Iter<'a,Vec<u32>>>}fn main(){}",
            true,
        ),
        (
            "map_flatten_field",
            "struct F<'a,T>{iter:core::iter::Map<core::iter::Flatten<core::slice::Iter<'a,Vec<u32>>>,fn(&u32)->&T>}fn main(){}",
            true,
        ),
        (
            "flatten_first",
            "fn first<'a>(values:&'a[Vec<u32>])->Option<&'a u32>{values.iter().flatten().next()}fn main(){}",
            true,
        ),
        (
            "flatten_expired",
            "fn first<'a>(values:&'a[Vec<u32>])->Option<&'a u32>{values.iter().flatten().next()}fn main(){let x;{let data=vec![vec![7]];x=first(&data);}println!(\"{:?}\",x);}",
            false,
        ),
        (
            "independent_rows",
            "fn f<'a,'b>(a:&'a[Vec<u32>],b:&'b[Vec<u32>])->(Option<&'a u32>,Option<&'b u32>){(a.iter().flatten().next(),b.iter().flatten().next())}fn main(){}",
            true,
        ),
        (
            "swapped_rows",
            "fn f<'a,'b>(a:&'a[Vec<u32>],b:&'b[Vec<u32>])->(Option<&'b u32>,Option<&'a u32>){(a.iter().flatten().next(),b.iter().flatten().next())}fn main(){}",
            false,
        ),
        (
            "mutable_first",
            "fn f<'a>(a:&'a mut[Vec<u32>])->Option<&'a mut u32>{a.iter_mut().flatten().next()}fn main(){}",
            true,
        ),
        (
            "mutable_static_escape",
            "fn f(a:&mut[Vec<u32>])->Option<&'static mut u32>{a.iter_mut().flatten().next()}fn main(){}",
            false,
        ),
        (
            "owned_ref_elements",
            "fn f<'a>(a:Vec<Vec<&'a u32>>)->Option<&'a u32>{a.into_iter().flatten().next()}fn main(){}",
            true,
        ),
        (
            "conditional_selection_missing",
            "struct Outer<'a>(&'a u32);impl<'a:'static>Iterator for Outer<'a>{type Item=Vec<u32>;fn next(&mut self)->Option<Self::Item>{None}}struct F<'a>{iter:core::iter::Flatten<Outer<'a>>}fn main(){}",
            false,
        ),
        (
            "conditional_selection_proved",
            "struct Outer<'a>(&'a u32);impl<'a:'static>Iterator for Outer<'a>{type Item=Vec<u32>;fn next(&mut self)->Option<Self::Item>{None}}struct F<'a:'static>{iter:core::iter::Flatten<Outer<'a>>}fn main(){}",
            true,
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

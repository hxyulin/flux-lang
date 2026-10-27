//! Nested callable captures are summarized by their enclosing closure body.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;
#[test]
fn closure_result_origins_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-closure-origins-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, valid) in [
        (
            "enum_map",
            include_str!("../../../tests/run/closure_result_origins.rs"),
            true,
        ),
        (
            "nested_map_refs",
            "fn pick<'a>(input:&'a[u32],marker:&u32)->Option<&'a u32>{let one=|i|{let _=marker;&input[i]};let mut out=None;for value in [0usize,1].iter().map(|&i|one(i)){out=Some(value);}out}fn main(){assert_eq!(*pick(&[3,7],&9).unwrap(),7);}",
            true,
        ),
        (
            "nested_tuple_output",
            "fn pick<'a,'b>(a:&'a u32,b:&'b u32,marker:&u32)->(&'a u32,&'b u32){let first=||{let _=marker;a};let second=||b;let mut values=[0].iter().map(|_|(first(),second()));values.next().unwrap()}fn main(){let a=3;let b=7;assert_eq!(*pick(&a,&b,&9).1,7);}",
            true,
        ),
        (
            "closure_field_in_adt",
            "struct Holder<F>(F);fn f<'a,F:Fn()->&'a u32>(holder:Holder<F>)->&'a u32{holder.0()}fn main(){let x=7;let inner=||&x;let outer=||inner();assert_eq!(*f(Holder(outer)),7);}",
            true,
        ),
        (
            "two_callable_fields",
            "struct Pair<F,G>(F,G);fn f<'a,'b,F:Fn()->&'a u32,G:Fn()->&'b u32>(pair:Pair<F,G>)->(&'a u32,&'b u32){(pair.0(),pair.1())}fn main(){let x=3;let y=7;let first=||&x;let a=||first();let second=||&y;let b=||second();assert_eq!(*f(Pair(a,b)).1,7);}",
            true,
        ),
        (
            "true_output_escape",
            "fn bad()->&'static u32{let local=7;let inner=||&local;let outer=||inner();[0].iter().map(|_|outer()).next().unwrap()}fn main(){}",
            false,
        ),
        (
            "stored_nested_closure_escape",
            "struct Holder<F>(F);fn bad<'a>() -> impl Fn()->u32 + 'a{let local=7;let inner=||local;let outer=||inner();Holder(outer).0}fn main(){}",
            false,
        ),
        (
            "borrowed_output_before_drop",
            "fn main(){let output;{let input=[7];let inner=|i|&input[i];output=[0usize].iter().map(|&i|inner(i)).next().unwrap();}println!(\"{}\",output);}",
            false,
        ),
        (
            "callback_retains_input",
            "fn invoke<F:FnMut(&u32)>(mut f:F){let local=7;f(&local);}fn bad<'a>(slot:&mut Option<&'a u32>){let mut inner=|x|*slot=Some(x);let outer=|x|inner(x);invoke(outer);}fn main(){}",
            false,
        ),
        (
            "wrong_tuple_origin",
            "fn bad<'a>(long:&'a u32)->(&'a u32,&'a u32){let local=7;let first=||long;let second=||&local;let mut values=[0].iter().map(|_|(first(),second()));values.next().unwrap()}fn main(){}",
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
            valid,
            "rustc {name}: {}",
            String::from_utf8_lossy(&oracle.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text(&path, source);
        assert!(hir.diagnostics.is_empty(), "{name}: {:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        let mut errors = typed.diagnostics.clone();
        if errors.is_empty() {
            errors.extend(flux_mir::build_file(&hir, &typed).diagnostics);
        }
        assert_eq!(errors.is_empty(), valid, "flux {name}: {errors:?}");
    }
}

#[test]
fn missing_callback_signature_remains_conservative() {
    let source = "fn take<I:Iterator>(mut iter:I)->Option<I::Item>{iter.next()}fn pick<'a>(input:&'a[u32],marker:&u32)->Option<&'a u32>{let one=|i|{let _=marker;&input[i]};take([0usize].iter().map(|&i|one(i)))}fn main(){assert_eq!(*pick(&[7],&9).unwrap(),7);}";
    let (_, _, hir) = flux_hir::lower_text("unknown_callback.rs", source);
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let mut typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let baseline = flux_mir::build_file(&hir, &typed);
    assert!(
        baseline.diagnostics.is_empty(),
        "{:?}",
        baseline.diagnostics
    );
    let source_file = hir
        .items
        .functions
        .iter()
        .find(|(_, function)| function.name.as_str() == "main")
        .unwrap()
        .1
        .loc
        .file;
    let target = hir
        .items
        .functions
        .iter()
        .find(|(_, f)| f.name.as_str() == "take" && f.loc.file == source_file)
        .unwrap()
        .0;
    let old = std::mem::take(&mut typed.sigs.regions.fns);
    for (id, regions) in old.iter().filter(|(id, _)| *id != target) {
        typed.sigs.regions.fns.insert(id, regions.clone());
    }
    let unknown = flux_mir::build_file(&hir, &typed);
    assert!(
        !unknown.diagnostics.is_empty(),
        "unknown callback unexpectedly lost all capture-to-result constraints"
    );
}

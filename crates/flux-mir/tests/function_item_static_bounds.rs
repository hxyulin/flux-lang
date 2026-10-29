//! Associated declaration equalities retain the caller's item lifetimes.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;
#[test]
fn function_item_static_bounds_match_rustc() {
    let root = std::env::temp_dir().join(format!(
        "flux-function-item-static-bounds-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "static_type_fnptr",
            "fn id<T:'static>(x:&T)->&T{x}fn get<T:'static>(){let _:fn(&T)->&T=id::<T>;}fn main(){get::<u32>();}",
            true,
        ),
        (
            "named_input_static_type",
            "fn id<'a,T:'static>(x:&'a T)->&'a T{x}fn get<T:'static>(){let _:fn(&T)->&T=id::<T>;}fn main(){get::<u32>();}",
            true,
        ),
        (
            "static_type_callback",
            "fn id<T:'static>(x:&T)->&T{x}fn apply<T:'static,F:for<'a>Fn(&'a T)->&'a T>(f:F,x:&T)->&T{f(x)}fn main(){let x=7;assert_eq!(*apply(id::<u32>,&x),7);}",
            true,
        ),
        (
            "predicate_only_early",
            "trait Read<'a>{}struct Reader<'a>(&'a u8);impl<'a>Read<'a> for Reader<'a>{}fn convert<'de,'s,R:Read<'de>>(_: &R,s:&'s[u8])->&'s[u8]{s}fn apply<'a,'r,R:Read<'a>,F:for<'s>FnOnce(&'r R,&'s[u8])->&'s[u8]>(read:&'r R,f:F)->usize{let bytes=[1u8,2,3];f(read,&bytes).len()}fn main(){let v=7;let reader=Reader(&v);assert_eq!(apply(&reader,convert),3);}",
            true,
        ),
        (
            "predicate_only_early_static_required",
            "trait Read<'a>{}struct Reader<'a>(&'a u8);impl<'a>Read<'a> for Reader<'a>{}fn convert<'s,R:Read<'static>>(_: &R,s:&'s[u8])->&'s[u8]{s}fn apply<'a,'r,R:Read<'a>,F:for<'s>FnOnce(&'r R,&'s[u8])->&'s[u8]>(read:&'r R,f:F)->usize{let bytes=[1u8,2,3];f(read,&bytes).len()}fn main(){let v=7;let reader=Reader(&v);assert_eq!(apply(&reader,convert),3);}",
            false,
        ),
        (
            "early_output_cannot_follow_late",
            "trait Read<'a>{fn data(&self)->&'a[u8];}struct Reader<'a>(&'a[u8]);impl<'a>Read<'a> for Reader<'a>{fn data(&self)->&'a[u8]{self.0}}fn convert<'de,'s,R:Read<'de>>(r:&R,_:&'s[u8])->&'de[u8]{r.data()}fn apply<'a,'r,R:Read<'a>,F:for<'s>FnOnce(&'r R,&'s[u8])->&'s[u8]>(read:&'r R,f:F)->usize{let bytes=[1u8,2,3];f(read,&bytes).len()}fn main(){let bytes=[7];let reader=Reader(&bytes);assert_eq!(apply(&reader,convert),1);}",
            false,
        ),
        (
            "missing_static",
            "fn id<T:'static>(x:&T)->&T{x}fn get<T>(){let _:fn(&T)->&T=id::<T>;}fn main(){}",
            false,
        ),
        (
            "actual_early_input",
            "fn id<'a,T:'a>(x:&'a T)->&'a T{x}fn get<'a,T:'a>(){let _:fn(&T)->&T=id::<T>;}fn main(){}",
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

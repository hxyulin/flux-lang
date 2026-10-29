//! Standard bounds for adapter formatting, result observations and owner moves.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "contract case selection only")]
mod cases;
#[test]
fn adapter_contracts_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-adapter-contracts-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "map_no_callback_debug",
            "fn yes<T:std::fmt::Debug>(){}fn generic<I:std::fmt::Debug,F>(){yes::<std::iter::Map<I,F>>();}fn main(){}",
            true,
        ),
        (
            "map_missing_iterator_debug",
            "struct I;impl Iterator for I{type Item=u32;fn next(&mut self)->Option<u32>{None}}fn main(){let _=format!(\"{:?}\",I.map(|x|x));}",
            false,
        ),
        (
            "map_no_output_debug",
            "struct N;fn main(){let _=format!(\"{:?}\",[1].iter().map(|_|N));}",
            true,
        ),
        (
            "cloned_bounds",
            "fn yes<T:std::fmt::Debug>(){}fn generic<I:std::fmt::Debug>(){yes::<std::iter::Cloned<I>>();}fn main(){}",
            true,
        ),
        (
            "flatten_no_item_debug",
            "struct N;#[derive(Debug)]struct Inner;impl Iterator for Inner{type Item=N;fn next(&mut self)->Option<N>{None}}#[derive(Debug)]struct Outer;impl Iterator for Outer{type Item=Inner;fn next(&mut self)->Option<Inner>{None}}fn main(){let _=format!(\"{:?}\",Outer.flatten());}",
            true,
        ),
        (
            "flatten_missing_inner_debug",
            "struct Inner;impl Iterator for Inner{type Item=u32;fn next(&mut self)->Option<u32>{None}}#[derive(Debug)]struct Outer;impl Iterator for Outer{type Item=Inner;fn next(&mut self)->Option<Inner>{None}}fn main(){let _=format!(\"{:?}\",Outer.flatten());}",
            false,
        ),
        (
            "flatten_missing_outer_debug",
            "#[derive(Debug)]struct Inner;impl Iterator for Inner{type Item=u32;fn next(&mut self)->Option<u32>{None}}struct Outer;impl Iterator for Outer{type Item=Inner;fn next(&mut self)->Option<Inner>{None}}fn main(){let _=format!(\"{:?}\",Outer.flatten());}",
            false,
        ),
        (
            "function_pointer_debug_no_value_bounds",
            "struct N;fn yes<T:std::fmt::Debug>(){}fn main(){yes::<fn(N)->N>();yes::<unsafe extern \"C\" fn(u32)->u64>();}",
            true,
        ),
        (
            "higher_ranked_pointer_debug",
            "fn yes<T:std::fmt::Debug>(){}fn main(){yes::<for<'a>fn(&'a mut u32)->&'a mut u32>();}",
            true,
        ),
        (
            "function_item_not_debug",
            "fn f(){}fn main(){let _=format!(\"{:?}\",f);}",
            false,
        ),
        (
            "closure_not_debug",
            "fn main(){let _=format!(\"{:?}\",||1);}",
            false,
        ),
        (
            "copied_owned_error",
            "struct E;fn f(x:Result<&u32,E>)->Result<u32,E>{x.copied()}fn main(){}",
            true,
        ),
        (
            "copied_mutable",
            "struct E;fn f(x:Result<&mut u32,E>)->Result<u32,E>{x.copied()}fn main(){}",
            true,
        ),
        (
            "copied_not_clone",
            "fn f(x:Result<&String,()>)->Result<String,()>{x.copied()}fn main(){}",
            false,
        ),
        (
            "inspect_nonclone_callback",
            "struct N;fn main(){let n=N;let x:Result<(),u32>=Err(3);let _=x.inspect_err(move |_|drop(n));}",
            true,
        ),
        (
            "inspect_reference_escape",
            "fn f()->&'static u32{let e:Result<(),u32>=Err(3);let mut out=None;let _=e.inspect_err(|error|out=Some(error));out.unwrap()}fn main(){}",
            false,
        ),
        (
            "inspect_wrong_return",
            "fn main(){let x:Result<(),u32>=Err(3);let _=x.inspect_err(|_|1);}",
            false,
        ),
        (
            "slice_mut_range_requires_mutable",
            "fn main(){let a=[1];let _=a.as_mut_ptr_range();}",
            false,
        ),
        (
            "slice_mut_range_immutable_borrow",
            "fn main(){let mut a=[1];let r=&a;let _=a.as_mut_ptr_range();println!(\"{:?}\",r);}",
            false,
        ),
        (
            "unwrap_nonclone",
            "struct N;fn f(a:std::sync::Arc<N>)->Result<N,std::sync::Arc<N>>{std::sync::Arc::try_unwrap(a)}fn main(){}",
            true,
        ),
        (
            "unwrap_unsized",
            "fn f(a:std::sync::Arc<[u32]>){let _=std::sync::Arc::try_unwrap(a);}fn main(){}",
            false,
        ),
        (
            "unwrap_borrow_escape",
            "fn f()->&'static u32{let n=1;let a=std::sync::Arc::new(&n);std::sync::Arc::try_unwrap(a).unwrap()}fn main(){}",
            false,
        ),
        (
            "unwrap_consumes_owner",
            "fn main(){let a=std::sync::Arc::new(1);let _=std::sync::Arc::try_unwrap(a);println!(\"{}\",a);}",
            false,
        ),
    ] {
        if !cases::selected(name) {
            continue;
        }
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let output = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("r.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            output.status.success(),
            accepted,
            "Rust {name}: {}",
            String::from_utf8_lossy(&output.stderr)
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
        assert_eq!(errors.is_empty(), accepted, "Flux {name}: {errors:?}");
    }
    std::fs::remove_dir_all(root).unwrap();
}

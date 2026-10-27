//! Omitted alias defaults retain the declaration's lifetime substitutions.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;
#[test]
fn default_alias_regions_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-default-regions-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepts) in [
        (
            "keep",
            "type A<'a,T=&'a u32>=(T,);fn keep<'a>(x:&'a u32)->A<'a>{(x,)}fn main(){let x=7;assert_eq!(*keep(&x).0,7);}",
            true,
        ),
        (
            "escape",
            "type A<'a,T=&'a u32>=(T,);fn bad()->A<'static>{let x=7;(&x,)}fn main(){}",
            false,
        ),
        (
            "dependent",
            "type A<T,U=T>=(T,U);fn keep<'a>(x:&'a u32)->A<&'a u32>{(x,x)}fn main(){}",
            true,
        ),
        (
            "dependent_escape",
            "type A<T,U=T>=(T,U);fn bad()->A<&'static u32>{let x=7;(&x,&x)}fn main(){}",
            false,
        ),
        (
            "mutable",
            "type A<'a,T=&'a mut u32>=(T,);fn keep<'a>(x:&'a mut u32)->A<'a>{(x,)}fn main(){}",
            true,
        ),
        (
            "mutable_escape",
            "type A<'a,T=&'a mut u32>=(T,);fn bad()->A<'static>{let mut x=7;(&mut x,)}fn main(){}",
            false,
        ),
        (
            "const_block",
            "struct B<const N:usize={1+2}>([u8;N]);fn main(){let b:B=B([1;3]);assert_eq!(b.0.len(),3);}",
            true,
        ),
        (
            "const_alias_block",
            "type A<const N:usize={2+2}>=[u8;N];fn main(){let a:A=[1;4];assert_eq!(a.len(),4);}",
            true,
        ),
        (
            "const_dependent_mixed",
            "struct B<T,const N:usize=2,const M:usize=N>(T,[u8;N],[u8;M]);type A<T=u16,const N:usize=3>=B<T,N>;fn main(){let a:A=B(7,[1;3],[2;3]);let _:u16=a.0;}",
            true,
        ),
        (
            "const_signed",
            "struct B<const N:i8=-1>;impl<const N:i8>B<N>{fn get(&self)->i8{N}}fn main(){let b:B=B;assert_eq!(b.get(),-1);}",
            true,
        ),
        (
            "const_typed_bool_char",
            "struct B<const X:bool=true,const C:char='x'>;impl<const X:bool,const C:char>B<X,C>{fn flag()->bool{X}fn ch()->char{C}}fn main(){assert!(B::<true>::flag());assert_eq!(B::<true>::ch(),'x');}",
            true,
        ),
        (
            "const_block_wrong_type",
            "struct B<const N:usize={true}>;fn main(){let _:B=B;}",
            false,
        ),
        (
            "const_block_forward",
            "struct B<const N:usize={M},const M:usize=2>;fn main(){}",
            false,
        ),
        (
            "const_generic_arithmetic",
            "struct B<const N:usize=2,const M:usize={N+1}>;fn main(){}",
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
            .arg(root.join("probe.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            oracle.status.success(),
            accepts,
            "rustc {name}: {}",
            String::from_utf8_lossy(&oracle.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text(&path, source);
        let mut errors = hir.diagnostics.clone();
        if errors.is_empty() {
            let typed = flux_mir::check_file(&hir);
            errors.extend(typed.diagnostics.iter().cloned());
            if errors.is_empty() {
                errors.extend(flux_mir::build_file(&hir, &typed).diagnostics);
            }
        }
        assert_eq!(errors.is_empty(), accepts, "Flux {name}: {errors:?}");
    }
    std::fs::remove_dir_all(root).unwrap();
}

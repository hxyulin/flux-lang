//! Structural constants and function-item callable objects used by flux-ty.
#[test]
fn selfhost_ty_semantics_match_rustc() {
    let root =
        std::env::temp_dir().join(format!("flux-ty-selfhost-semantics-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "const_slice_len",
            "const INPUT:&[u8]=&[1,2,3]; const N:usize=INPUT.len(); const M:usize=<[u8]>::len(INPUT); const fn count(xs:&[u8])->usize { xs.len() } const K:usize=count(INPUT); const Z:usize=<[u8]>::len(&[]); const A:[u8;N]=[0;M]; fn main(){assert_eq!(N,3);assert_eq!(K,3);assert_eq!(Z,0);assert_eq!(A.len(),3);}",
            true,
        ),
        (
            "newtype",
            "#[derive(PartialEq, Eq)] struct Ty(u32); const ERROR: Ty=Ty(0); fn probe(x:Ty)->u32 { match x { ERROR=>7, _=>9 } } fn main(){assert_eq!(probe(Ty(0)),7); assert_eq!(probe(Ty(1)),9);}",
            true,
        ),
        (
            "aggregate",
            "#[derive(PartialEq)] struct Inner(bool,u8); #[derive(PartialEq)] struct Ty(Inner,[u8;2]); const VALUE:Ty=Ty(Inner(true,2),[3,4]); fn probe(x:Ty)->u32 { match x { VALUE=>7, _=>9 } } fn main(){assert_eq!(probe(Ty(Inner(true,2),[3,4])),7); assert_eq!(probe(Ty(Inner(false,2),[3,4])),9);}",
            true,
        ),
        (
            "exhaustive",
            "#[derive(PartialEq)] struct Flag(bool); const YES:Flag=Flag(true); const NO:Flag=Flag(false); fn probe(x:Flag)->bool { match x { YES=>true, NO=>false } } fn main(){}",
            true,
        ),
        (
            "nonexhaustive",
            "#[derive(PartialEq)] struct Flag(bool); const YES:Flag=Flag(true); fn probe(x:Flag)->bool { match x { YES=>true } } fn main(){}",
            false,
        ),
        (
            "handwritten_eq",
            "struct Ty(u32); impl PartialEq for Ty { fn eq(&self,_:&Self)->bool { true } } const VALUE:Ty=Ty(0); fn main(){let x=Ty(3); match x { VALUE=>(), _=>() }}",
            false,
        ),
        (
            "function_object",
            "fn read(x:&u64)->u64{*x} fn apply(f:&dyn Fn(&u64)->u64)->u64{let x=17; f(&x)} fn main(){assert_eq!(apply(&read),17);}",
            true,
        ),
        (
            "independent",
            "fn read(x:&u64,y:&u64)->u64{*x+*y} fn apply(f:&dyn Fn(&u64,&u64)->u64)->u64{let x=3; let y=7; f(&x,&y)} fn main(){assert_eq!(apply(&read),10);}",
            true,
        ),
        (
            "mutable",
            "fn add(x:&mut u64)->u64{*x+=3;*x} fn main(){let f:&dyn Fn(&mut u64)->u64=&add; let mut x=4; assert_eq!(f(&mut x),7);}",
            true,
        ),
        (
            "function_pointer",
            "fn read(x:&u64)->u64{*x} fn main(){let p:fn(&u64)->u64=read;let f:&dyn Fn(&u64)->u64=&p;let x=8;assert_eq!(f(&x),8);}",
            true,
        ),
        (
            "generic_callable_output",
            "#[derive(Clone,Copy)] enum Region { Static, Param(u32) } fn instantiate<R:Copy>(region:&mut dyn FnMut(Region)->R,args:&dyn Fn(u32)->Vec<R>,out:&mut Vec<R>){out.push(region(Region::Static));out.extend(args(2));} fn main(){let mut map=|r|r;let argument=|n|vec![Region::Param(n)];let mut out=Vec::new();instantiate(&mut map,&argument,&mut out);assert_eq!(out.len(),2);}",
            true,
        ),
        (
            "nested_function_object",
            "fn read(x:&u64)->u64{*x} fn main(){let make=|| {let f:&dyn Fn(&u64)->u64=&read;let x=8;f(&x)};assert_eq!(make(),8);}",
            true,
        ),
        (
            "nested_static_function",
            "fn keep(_: &'static u64) {} fn main(){let make=|| {let _: &dyn Fn(&u64)=&keep;};make();}",
            false,
        ),
        (
            "static_function",
            "fn keep(_: &'static u64) {} fn main(){let _: &dyn Fn(&u64)=&keep;}",
            false,
        ),
    ] {
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
            accepted,
            "rustc {name}: {}",
            String::from_utf8_lossy(&oracle.stderr)
        );
        let (db, _, hir) = flux_hir::lower_text(&path, source);
        assert!(db.syntax_errors().is_empty());
        assert!(hir.diagnostics.is_empty(), "{name}: {:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        let mut errors = typed.diagnostics.clone();
        if errors.is_empty() {
            errors.extend(flux_mir::build_file(&hir, &typed).diagnostics);
        }
        assert_eq!(errors.is_empty(), accepted, "{name}: {errors:?}");
    }
    std::fs::remove_dir_all(root).unwrap();
}

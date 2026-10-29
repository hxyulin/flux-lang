//! Named regions in tracked trait arguments survive calls and impl selection.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;

#[test]
fn named_trait_type_arguments_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-trait-type-regions-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "shared_input",
            "trait Read<T>{fn read(self,x:T)->T;}impl<'a>Read<&'a u32>for(){fn read(self,x:&'a u32)->&'a u32{x}}fn via<'a,R:Read<&'a u32>>(r:R,x:&'a u32)->&'a u32{r.read(x)}fn main(){assert_eq!(*via((),&7),7);}",
            true,
        ),
        (
            "shared_escape",
            "trait Read<T>{fn read(self,x:T)->T;}fn via<'a,R:Read<&'a u32>>(r:R,x:&'a u32)->&'static u32{r.read(x)}fn main(){}",
            false,
        ),
        (
            "mutable_input",
            "trait Read<T>{fn read(self,x:T)->T;}fn via<'a,R:Read<&'a mut u32>>(r:R,x:&'a mut u32)->&'a mut u32{r.read(x)}fn main(){}",
            true,
        ),
        (
            "static_bound_mismatch",
            "trait Read<T>{fn read(self,x:T)->T;}fn need<R:Read<&'static u32>>(_:R){}fn via<'a,R:Read<&'a u32>>(r:R){need(r)}fn main(){}",
            false,
        ),
        (
            "impl_forwarding",
            "trait Read<T>{fn read(self,x:T)->T;}struct Wrap<R>(R);impl<'a,R:Read<&'a u32>>Read<&'a u32>for Wrap<R>{fn read(self,x:&'a u32)->&'a u32{self.0.read(x)}}fn main(){}",
            true,
        ),
        (
            "from_borrow",
            "fn convert<'a,T:From<&'a str>>(x:&'a str)->T{T::from(x)}fn main(){let s:String=convert(\"abc\");assert_eq!(s,\"abc\");}",
            true,
        ),
        (
            "from_borrow_escape",
            "fn convert<'a,T:From<&'static str>>(x:&'a str)->T{T::from(x)}fn main(){}",
            false,
        ),
        (
            "independent_arguments",
            "trait Pick<A,B>{fn pick(self,a:A,b:B)->(A,B);}fn via<'a,'b,R:Pick<&'a u32,&'b u32>>(r:R,a:&'a u32,b:&'b u32)->(&'a u32,&'b u32){r.pick(a,b)}fn main(){}",
            true,
        ),
        (
            "swapped_arguments",
            "trait Pick<A,B>{fn pick(self,a:A,b:B)->(A,B);}fn via<'a,'b,R:Pick<&'a u32,&'b u32>>(r:R,a:&'a u32,b:&'b u32)->(&'b u32,&'a u32){r.pick(a,b)}fn main(){}",
            false,
        ),
        (
            "nested_mutable_invariance",
            "trait Read<T>{fn read(self,x:T)->T;}fn via<'a,'b,R:Read<&'a mut &'a u32>>(r:R,x:&'a mut &'a u32)->&'b mut &'b u32 where 'a:'b{r.read(x)}fn main(){}",
            false,
        ),
        (
            "caller_region_equality",
            "trait Mark<T>{}fn need<'b,R:Mark<&'b u32>>(_:R,_:&'b mut &'b u32){}fn via<'a,'b,R:Mark<&'a u32>>(r:R,x:&'b mut &'b u32)where 'a:'b,'b:'a{need(r,x)}fn main(){}",
            true,
        ),
        (
            "caller_region_one_way",
            "trait Mark<T>{}fn need<'b,R:Mark<&'b u32>>(_:R,_:&'b mut &'b u32){}fn via<'a,'b,R:Mark<&'a u32>>(r:R,x:&'b mut &'b u32)where 'a:'b{need(r,x)}fn main(){}",
            false,
        ),
        (
            "default_method",
            "trait Read<T>{fn read(self,x:T)->T;fn again(self,x:T)->T where Self:Sized{self.read(x)}}fn via<'a,R:Read<&'a u32>>(r:R,x:&'a u32)->&'a u32{r.again(x)}fn main(){}",
            true,
        ),
        (
            "output_argument",
            "trait Get<T>{fn get(self)->T;}fn via<'a,R:Get<&'a u32>>(r:R)->&'a u32{r.get()}fn main(){}",
            true,
        ),
        (
            "output_argument_escape",
            "trait Get<T>{fn get(self)->T;}fn via<'a,R:Get<&'a u32>>(r:R)->&'static u32{r.get()}fn main(){}",
            false,
        ),
        (
            "nested_argument",
            "trait Read<T>{fn read(self,x:T)->T;}fn via<'a,R:Read<Option<&'a u32>>>(r:R,x:Option<&'a u32>)->Option<&'a u32>{r.read(x)}fn main(){}",
            true,
        ),
        (
            "nested_argument_escape",
            "trait Read<T>{fn read(self,x:T)->T;}fn via<'a,R:Read<Option<&'a u32>>>(r:R,x:Option<&'a u32>)->Option<&'static u32>{r.read(x)}fn main(){}",
            false,
        ),
        (
            "alias_argument",
            "type Ref<'a>=&'a u32;trait Read<T>{fn read(self,x:T)->T;}fn via<'a,R:Read<Ref<'a>>>(r:R,x:Ref<'a>)->Ref<'a>{r.read(x)}fn main(){}",
            true,
        ),
        (
            "alias_argument_escape",
            "type Ref<'a>=&'a u32;trait Read<T>{fn read(self,x:T)->T;}fn via<'a,R:Read<Ref<'a>>>(r:R,x:Ref<'a>)->Ref<'static>{r.read(x)}fn main(){}",
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
            "rustc {name}: {}",
            String::from_utf8_lossy(&oracle.stderr)
        );
        let (db, _, hir) = flux_hir::lower_text(&path, source);
        assert!(
            db.syntax_errors().is_empty(),
            "{name}: {:?}",
            db.syntax_errors()
        );
        assert!(hir.diagnostics.is_empty(), "{name}: {:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        assert!(
            typed.diagnostics.is_empty(),
            "{name}: {:?}",
            typed.diagnostics
        );
        let mir = flux_mir::build_file(&hir, &typed);
        assert_eq!(
            mir.diagnostics.is_empty(),
            accepted,
            "{name}: {:?}",
            mir.diagnostics
        );
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn opaque_trait_arguments_preserve_separate_region_tables() {
    let source =
        "trait Read<T>{}impl<T>Read<T>for(){}fn opaque<'a>()->impl Read<&'a str>{()}fn main(){}";
    let root =
        std::env::temp_dir().join(format!("flux-opaque-trait-regions-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("opaque.rs");
    std::fs::write(&path, source).unwrap();
    let oracle = std::process::Command::new("rustc")
        .args(["--edition=2024", "--emit=metadata"])
        .arg(&path)
        .arg("-o")
        .arg(root.join("oracle.rmeta"))
        .output()
        .unwrap();
    assert!(
        oracle.status.success(),
        "{}",
        String::from_utf8_lossy(&oracle.stderr)
    );
    let (_, _, hir) = flux_hir::lower_text(&path, source);
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let mir = flux_mir::build_file(&hir, &typed);
    assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn duplicate_trait_argument_rows_match_rustc_ambiguity() {
    let root = std::env::temp_dir().join(format!(
        "flux-duplicate-trait-regions-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "distinct",
            "trait Mark<T>{}fn f<'a,'b,R:Mark<&'a u32>+Mark<&'b u32>>(_:R){}fn main(){}",
            false,
        ),
        (
            "mutual_outlives",
            "trait Mark<T>{}fn f<'a:'b,'b:'a,R:Mark<&'a u32>+Mark<&'b u32>>(_:R){}fn main(){}",
            false,
        ),
        (
            "identical",
            "trait Mark<T>{}fn f<'a,R:Mark<&'a u32>+Mark<&'a u32>>(_:R){}fn main(){}",
            true,
        ),
        (
            "different_types",
            "trait Mark<T>{}fn f<'a,'b,R:Mark<&'a u32>+Mark<&'b i32>>(_:R){}fn main(){}",
            true,
        ),
        (
            "concrete_subject",
            "trait Mark<T>{}struct S;impl<T>Mark<T>for S{}fn f<'a,'b>()where S:Mark<&'a u32>+Mark<&'b u32>{}fn main(){}",
            false,
        ),
        (
            "distinct_owner_rows",
            "trait Mark<'a>{}fn f<'a,'b,R:Mark<'a>+Mark<'b>>(_:R){}fn main(){}",
            false,
        ),
        (
            "mutual_owner_rows",
            "trait Mark<'a>{}fn f<'a:'b,'b:'a,R:Mark<'a>+Mark<'b>>(_:R){}fn main(){}",
            false,
        ),
        (
            "identical_owner_rows",
            "trait Mark<'a>{}fn f<'a,R:Mark<'a>+Mark<'a>>(_:R){}fn main(){}",
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
            "rustc {name}: {}",
            String::from_utf8_lossy(&oracle.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text(&path, source);
        assert!(hir.diagnostics.is_empty(), "{name}: {:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        assert_eq!(
            typed.diagnostics.is_empty(),
            accepted,
            "{name}: {:?}",
            typed.diagnostics
        );
        if !accepted {
            assert!(
                typed
                    .diagnostics
                    .iter()
                    .any(|d| d.message == "ambiguous trait lifetime arguments"),
                "{name}: {:?}",
                typed.diagnostics
            );
        }
    }
    std::fs::remove_dir_all(root).unwrap();
}

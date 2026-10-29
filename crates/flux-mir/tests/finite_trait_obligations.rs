//! Finite ordinary impl nesting does not consume a global recursion budget.
use flux_hir::{SourceCrate, SourceDb};
use flux_intern::Symbol;
use flux_syntax::Edition;
#[test]
fn finite_trait_obligations_match_rustc() {
    let root = std::env::temp_dir().join(format!(
        "flux-finite-trait-obligations-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "depth_4",
            "static GLOBAL:u32=7;trait Read<'a>{fn read(&self)->&'a u32;}struct Leaf<'a>(&'a u32);impl<'a>Read<'a>for Leaf<'a>{fn read(&self)->&'a u32{self.0}}struct Wrap<P>(P);impl<'a,P:Read<'a>>Read<'a>for Wrap<P>{fn read(&self)->&'a u32{self.0.read()}}struct Pair<P,Q>(P,Q);impl<'a,P:Read<'a>,Q:Read<'a>>Read<'a>for Pair<P,Q>{fn read(&self)->&'a u32{self.0.read()}}fn need<'a,P:Read<'a>>(p:&P,_:&'a u32)->&'a u32{p.read()}fn main(){let value=13;let parser=Wrap(Wrap(Wrap(Wrap(Leaf(&value)))));assert_eq!(*need(&parser,&value),13);}",
            true,
        ),
        (
            "depth_12",
            "static GLOBAL:u32=7;trait Read<'a>{fn read(&self)->&'a u32;}struct Leaf<'a>(&'a u32);impl<'a>Read<'a>for Leaf<'a>{fn read(&self)->&'a u32{self.0}}struct Wrap<P>(P);impl<'a,P:Read<'a>>Read<'a>for Wrap<P>{fn read(&self)->&'a u32{self.0.read()}}struct Pair<P,Q>(P,Q);impl<'a,P:Read<'a>,Q:Read<'a>>Read<'a>for Pair<P,Q>{fn read(&self)->&'a u32{self.0.read()}}fn need<'a,P:Read<'a>>(p:&P,_:&'a u32)->&'a u32{p.read()}fn main(){let value=13;let parser=Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Leaf(&value)))))))))))));assert_eq!(*need(&parser,&value),13);}",
            true,
        ),
        (
            "depth_32",
            "static GLOBAL:u32=7;trait Read<'a>{fn read(&self)->&'a u32;}struct Leaf<'a>(&'a u32);impl<'a>Read<'a>for Leaf<'a>{fn read(&self)->&'a u32{self.0}}struct Wrap<P>(P);impl<'a,P:Read<'a>>Read<'a>for Wrap<P>{fn read(&self)->&'a u32{self.0.read()}}struct Pair<P,Q>(P,Q);impl<'a,P:Read<'a>,Q:Read<'a>>Read<'a>for Pair<P,Q>{fn read(&self)->&'a u32{self.0.read()}}fn need<'a,P:Read<'a>>(p:&P,_:&'a u32)->&'a u32{p.read()}fn main(){let value=13;let parser=Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Leaf(&value)))))))))))))))))))))))))))))))));assert_eq!(*need(&parser,&value),13);}",
            true,
        ),
        (
            "depth_64",
            "static GLOBAL:u32=7;trait Read<'a>{fn read(&self)->&'a u32;}struct Leaf<'a>(&'a u32);impl<'a>Read<'a>for Leaf<'a>{fn read(&self)->&'a u32{self.0}}struct Wrap<P>(P);impl<'a,P:Read<'a>>Read<'a>for Wrap<P>{fn read(&self)->&'a u32{self.0.read()}}struct Pair<P,Q>(P,Q);impl<'a,P:Read<'a>,Q:Read<'a>>Read<'a>for Pair<P,Q>{fn read(&self)->&'a u32{self.0.read()}}fn need<'a,P:Read<'a>>(p:&P,_:&'a u32)->&'a u32{p.read()}fn main(){let value=13;let parser=Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Leaf(&value)))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))));assert_eq!(*need(&parser,&value),13);}",
            true,
        ),
        (
            "deep_caller_bound",
            "static GLOBAL:u32=7;trait Read<'a>{fn read(&self)->&'a u32;}struct Leaf<'a>(&'a u32);impl<'a>Read<'a>for Leaf<'a>{fn read(&self)->&'a u32{self.0}}struct Wrap<P>(P);impl<'a,P:Read<'a>>Read<'a>for Wrap<P>{fn read(&self)->&'a u32{self.0.read()}}struct Pair<P,Q>(P,Q);impl<'a,P:Read<'a>,Q:Read<'a>>Read<'a>for Pair<P,Q>{fn read(&self)->&'a u32{self.0.read()}}fn need<'a,P:Read<'a>>(p:&P,_:&'a u32)->&'a u32{p.read()}fn generic<'a,P:Read<'a>>(p:P,x:&'a u32)->&'a u32{let parser=Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(p))))))))))))))))))))))))))))))));need(&parser,x)}fn main(){}",
            true,
        ),
        (
            "deep_escape",
            "static GLOBAL:u32=7;trait Read<'a>{fn read(&self)->&'a u32;}struct Leaf<'a>(&'a u32);impl<'a>Read<'a>for Leaf<'a>{fn read(&self)->&'a u32{self.0}}struct Wrap<P>(P);impl<'a,P:Read<'a>>Read<'a>for Wrap<P>{fn read(&self)->&'a u32{self.0.read()}}struct Pair<P,Q>(P,Q);impl<'a,P:Read<'a>,Q:Read<'a>>Read<'a>for Pair<P,Q>{fn read(&self)->&'a u32{self.0.read()}}fn need<'a,P:Read<'a>>(p:&P,_:&'a u32)->&'a u32{p.read()}fn escape(value:&u32)->&'static u32{let parser=Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Leaf(value)))))))))))))))))))))))))))))))));need(&parser,&GLOBAL)}fn main(){}",
            false,
        ),
        (
            "sibling_distinct_rows_False",
            "static GLOBAL:u32=7;trait Read<'a>{fn read(&self)->&'a u32;}struct Leaf<'a>(&'a u32);impl<'a>Read<'a>for Leaf<'a>{fn read(&self)->&'a u32{self.0}}struct Wrap<P>(P);impl<'a,P:Read<'a>>Read<'a>for Wrap<P>{fn read(&self)->&'a u32{self.0.read()}}struct Pair<P,Q>(P,Q);impl<'a,P:Read<'a>,Q:Read<'a>>Read<'a>for Pair<P,Q>{fn read(&self)->&'a u32{self.0.read()}}fn need<'a,P:Read<'a>>(p:&P,_:&'a u32)->&'a u32{p.read()}fn main(){let value=13;let parser=Pair(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Leaf(&GLOBAL))))))))))))),Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Leaf(&value))))))))))))));let _result:&'static u32=need(&parser,&GLOBAL);}",
            false,
        ),
        (
            "sibling_distinct_rows_True",
            "static GLOBAL:u32=7;trait Read<'a>{fn read(&self)->&'a u32;}struct Leaf<'a>(&'a u32);impl<'a>Read<'a>for Leaf<'a>{fn read(&self)->&'a u32{self.0}}struct Wrap<P>(P);impl<'a,P:Read<'a>>Read<'a>for Wrap<P>{fn read(&self)->&'a u32{self.0.read()}}struct Pair<P,Q>(P,Q);impl<'a,P:Read<'a>,Q:Read<'a>>Read<'a>for Pair<P,Q>{fn read(&self)->&'a u32{self.0.read()}}fn need<'a,P:Read<'a>>(p:&P,_:&'a u32)->&'a u32{p.read()}fn main(){let value=13;let parser=Pair(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Leaf(&value))))))))))))),Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Leaf(&GLOBAL))))))))))))));let _result:&'static u32=need(&parser,&GLOBAL);}",
            false,
        ),
        (
            "deep_mutable_good",
            "trait Change<T>{fn change(&mut self,x:T);}struct Leaf<'a>(&'a u32);impl<'a>Change<&'a mut &'a u32>for Leaf<'a>{fn change(&mut self,_:&'a mut &'a u32){}}struct Wrap<P>(P);impl<T,P:Change<T>>Change<T>for Wrap<P>{fn change(&mut self,x:T){self.0.change(x)}}fn need<T,P:Change<T>>(p:&mut P,x:T){p.change(x)}fn use_it<'a>(tag:&'a u32,x:&'a mut &'a u32){let mut parser=Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Leaf(tag)))))))))))))))));need(&mut parser,x)}fn main(){}",
            true,
        ),
        (
            "deep_mutable_invariance",
            "trait Change<T>{fn change(&mut self,x:T);}struct Leaf<'a>(&'a u32);impl<'a>Change<&'a mut &'a u32>for Leaf<'a>{fn change(&mut self,_:&'a mut &'a u32){}}struct Wrap<P>(P);impl<T,P:Change<T>>Change<T>for Wrap<P>{fn change(&mut self,x:T){self.0.change(x)}}fn need<T,P:Change<T>>(p:&mut P,x:T){p.change(x)}fn bad<'a,'b>(tag:&'a u32,x:&'a mut &'b u32){let mut parser=Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Leaf(tag)))))))))))))))));need(&mut parser,x)}fn main(){}",
            false,
        ),
        (
            "cyclic_inductive",
            "trait Read<'a>{}struct Loop;impl<'a>Read<'a>for Loop where Loop:Read<'a>{}fn need<'a,P:Read<'a>>(_:P){}fn main(){need(Loop);}",
            false,
        ),
        (
            "growing_inductive",
            "trait Read<'a>{}struct Wrap<P>(P);impl<'a,T>Read<'a>for T where Wrap<T>:Read<'a>{}fn need<'a,P:Read<'a>>(_:P){}fn main(){need(7u32);}",
            false,
        ),
        (
            "conditional_leaf",
            "static GLOBAL:u32=7;trait Read<'a>{fn read(&self)->&'a u32;}struct Leaf<'a>(&'a u32);impl<'a>Read<'static>for Leaf<'a> where 'a:'static{fn read(&self)->&'static u32{self.0}}struct Wrap<P>(P);impl<'a,P:Read<'a>>Read<'a>for Wrap<P>{fn read(&self)->&'a u32{self.0.read()}}struct Pair<P,Q>(P,Q);impl<'a,P:Read<'a>,Q:Read<'a>>Read<'a>for Pair<P,Q>{fn read(&self)->&'a u32{self.0.read()}}fn need<'a,P:Read<'a>>(p:&P,_:&'a u32)->&'a u32{p.read()}fn bad(value:&u32)->&'static u32{let parser=Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Leaf(value)))))))))))))))));need(&parser,&GLOBAL)}fn main(){}",
            false,
        ),
    ] {
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let output = std::process::Command::new("rustc")
            .args(["--edition=2021", "--emit=metadata"])
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
        let mut db = SourceDb::new();
        let file = db.add_with_edition(&path, source, Edition::E2021);
        let hir = flux_hir::lower_sources(
            &mut db,
            &[SourceCrate {
                root: file,
                name: Symbol::intern(name),
                dependencies: vec![],
            }],
        );
        let typed = flux_ty::check_file(&hir);
        if !accepted && !matches!(name, "cyclic_inductive" | "growing_inductive") {
            assert!(
                typed.diagnostics.is_empty(),
                "{name}: negative must reach MIR: {:?}",
                typed.diagnostics
            );
        }
        let mut errors = hir.diagnostics.clone();
        errors.extend(typed.diagnostics.clone());
        if errors.is_empty() {
            errors.extend(flux_mir::build_file(&hir, &typed).diagnostics);
        }
        assert_eq!(errors.is_empty(), accepted, "Flux {name}: {errors:?}");
    }
    std::fs::remove_dir_all(root).unwrap();
}

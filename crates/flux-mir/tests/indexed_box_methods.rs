//! Mutable method dispatch distinguishes owning Box dereferences from shared owners.
#[test]
fn mutable_indexed_receiver_rejections_match_rustc() {
    let prelude = "trait Selection{fn init(&mut self);}struct Counter;impl Selection for Counter{fn init(&mut self){}}";
    for (name, body) in [
        (
            "nonmut_vec",
            "fn main(){let xs:Vec<Box<dyn Selection>>=vec![Box::new(Counter)];xs[0].init();}",
        ),
        (
            "shared_vec",
            "fn call(xs:&Vec<Box<dyn Selection>>){xs[0].init();}fn main(){}",
        ),
        (
            "rc_owner",
            "fn main(){let mut xs:Vec<std::rc::Rc<dyn Selection>>=vec![std::rc::Rc::new(Counter)];xs[0].init();}",
        ),
        (
            "shared_user_deref",
            "struct Shared(Box<dyn Selection>);impl std::ops::Deref for Shared{type Target=dyn Selection;fn deref(&self)->&Self::Target{&*self.0}}fn main(){let mut xs=vec![Shared(Box::new(Counter))];xs[0].init();}",
        ),
        (
            "index_only",
            "struct Indexed(Box<dyn Selection>);impl std::ops::Index<usize> for Indexed{type Output=Box<dyn Selection>;fn index(&self,_:usize)->&Self::Output{&self.0}}fn main(){let mut xs=Indexed(Box::new(Counter));xs[0].init();}",
        ),
    ] {
        let source = format!("{prelude}{body}");
        let dir =
            std::env::temp_dir().join(format!("flux-indexed-box-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("main.rs");
        std::fs::write(&path, &source).unwrap();
        let result = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(dir.join("out.rmeta"))
            .output()
            .unwrap();
        assert!(!result.status.success(), "rustc accepted {name}");
        let (_, _, hir) = flux_hir::lower_text(&path, &source);
        assert!(hir.diagnostics.is_empty(), "{name}: {:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        if typed.diagnostics.is_empty() {
            let mir = flux_mir::build_file(&hir, &typed);
            assert!(!mir.diagnostics.is_empty(), "flux accepted {name}");
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}

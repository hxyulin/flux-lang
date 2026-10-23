//! Same-receiver candidates must retain ambiguity, move and borrow checks.
use std::path::Path;
use std::process::Command;

#[test]
fn borrowed_candidate_oracles() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = std::env::temp_dir().join(format!("flux-borrowed-candidates-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, source, error) in [
        (
            "exact_018",
            "trait A {fn pick(&self)->u8;} trait B {fn pick(self)->u8;} struct S; impl A for S {fn pick(&self)->u8{1}} impl<T> B for &T {fn pick(self)->u8{2}} fn borrowed<T:A>(x:&T)->u8{x.pick()} fn owned<T:A>(x:T)->u8{x.pick()} fn main(){println!(\"{} {}\",borrowed(&S),owned(S));}",
            None,
        ),
        (
            "runtime",
            include_str!("../../../tests/run/method_borrowed_candidate_priority.rs"),
            None,
        ),
        (
            "concrete_ambiguity",
            "trait A {fn pick(&self);} trait B {fn pick(self);} struct S; impl A for S {fn pick(&self){}} impl B for &S {fn pick(self){}} fn main(){(&S).pick();}",
            Some("multiple applicable items"),
        ),
        (
            "borrowed_param_ambiguity",
            "trait A {fn pick(&self);} trait B {fn pick(&self);} trait C {fn pick(self);} impl<T> C for &T {fn pick(self){}} fn a<T:A+B>(x:&T){x.pick();} fn main(){}",
            Some("multiple applicable items"),
        ),
        (
            "mutable_param_ambiguity",
            "trait A {fn pick(&mut self);} trait B {fn pick(&mut self);} fn a<T:A+B>(x:&mut T){x.pick();} fn main(){}",
            Some("multiple applicable items"),
        ),
        (
            "generic_move",
            "trait Take {fn take(self);} fn a<T:Take>(x:T){x.take();x.take();} fn main(){}",
            Some("use of moved value: `x`"),
        ),
        (
            "noncopy_move",
            "struct S(u8); impl S {fn take(self){}} fn a(x:&S){x.take();} fn main(){}",
            Some("cannot move out of"),
        ),
        (
            "borrowed_output_escape",
            "trait View {fn view(&self)->&u8;} trait Other {fn view(self)->u8;} impl<T> Other for &T {fn view(self)->u8{0}} fn a<T:View>(x:T)->&'static u8{x.view()} fn main(){}",
            Some("cannot return value referencing"),
        ),
        (
            "reborrow_overlap",
            "trait View {fn view(&mut self)->&mut u8;} trait Other {fn view(self)->u8;} impl<T> Other for &mut T {fn view(self)->u8{0}} fn a<T:View>(x:&mut T){let a=x.view();let b=x.view();println!(\"{} {}\",a,b);} fn main(){}",
            Some("cannot borrow `*x` as mutable more than once"),
        ),
        (
            "method_obligation",
            "trait A {fn pick(&self) where Self:Copy;} trait B {fn pick(self);} impl<T> B for &T {fn pick(self){}} fn a<T:A>(x:&T){x.pick();} fn main(){}",
            Some("Copy"),
        ),
    ] {
        let file = dir.join(format!("{name}.rs"));
        std::fs::write(&file, source).unwrap();
        let rustc = Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&file)
            .arg("-o")
            .arg(dir.join(format!("{name}.rmeta")))
            .output()
            .unwrap();
        let flux = Command::new(env!("CARGO_BIN_EXE_flux"))
            .current_dir(&root)
            .arg("mir")
            .arg(&file)
            .output()
            .unwrap();
        for (compiler, output) in [("rustc", rustc), ("flux", flux)] {
            let diagnostics = String::from_utf8_lossy(&output.stderr);
            assert_eq!(
                output.status.success(),
                error.is_none(),
                "{compiler} {name}: {diagnostics}"
            );
            if let Some(error) = error {
                assert!(
                    diagnostics.contains(error),
                    "{compiler} {name}: {diagnostics}"
                );
            }
        }
    }
}

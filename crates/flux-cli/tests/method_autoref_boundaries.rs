//! Compile-only receiver selection and borrow boundaries, checked against rustc.
use std::path::Path;
use std::process::Command;

#[test]
fn autoref_rejections_match_rustc() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = std::env::temp_dir().join(format!("flux-autoref-negative-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, source, expected) in [
        (
            "negative_length",
            "fn main() { let s: &[u8] = &[1]; let _: [u8; -1] = s[..].try_into().unwrap(); }",
            "cannot apply unary operator `-` to type `usize`",
        ),
        (
            "non_copy",
            "struct N(u8); fn main() { let a = [N(1)]; let s: &[N] = &a; let _: [N; 1] = s[..].try_into().unwrap(); }",
            "the trait bound `[N; 1]: TryFrom<&[N]>` is not satisfied",
        ),
        (
            "escape",
            "struct S(u8); trait View { type Output; fn view(self) -> Self::Output; } impl<'a> View for &'a S { type Output = &'a u8; fn view(self) -> &'a u8 { &self.0 } } fn escape() -> &'static u8 { let s = S(1); s.view() } fn main() {}",
            "cannot return value referencing local variable `s`",
        ),
        (
            "overlap",
            "struct S(u8); trait Edit { type Output; fn edit(self) -> Self::Output; } impl<'a> Edit for &'a mut S { type Output = &'a mut u8; fn edit(self) -> &'a mut u8 { &mut self.0 } } fn main() { let mut s = S(1); let a = s.edit(); let b = s.edit(); println!(\"{} {}\", a, b); }",
            "cannot borrow `s` as mutable more than once at a time",
        ),
        (
            "immutable",
            "struct S; trait Edit { fn edit(self); } impl Edit for &mut S { fn edit(self) {} } fn main() { let s = S; s.edit(); }",
            "cannot borrow `s` as mutable, as it is not declared as mutable",
        ),
        (
            "ambiguity",
            "struct S; trait A { fn pick(self); } trait B { fn pick(&self); } impl A for &S { fn pick(self) {} } impl B for S { fn pick(&self) {} } fn main() { S.pick(); }",
            "multiple applicable items in scope",
        ),
        (
            "scope",
            "struct S; mod hidden { pub trait Pick { fn pick(self); } impl Pick for &super::S { fn pick(self) {} } } fn main() { S.pick(); }",
            "no method named `pick` found",
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
        assert!(!rustc.status.success(), "rustc accepted {name}");
        let rustc_diagnostics = String::from_utf8_lossy(&rustc.stderr);
        assert!(
            rustc_diagnostics.contains(expected),
            "{name}: {rustc_diagnostics}"
        );
        let flux = Command::new(env!("CARGO_BIN_EXE_flux"))
            .current_dir(&root)
            .arg("mir")
            .arg(&file)
            .output()
            .unwrap();
        assert!(!flux.status.success(), "flux accepted {name}");
        let diagnostics = String::from_utf8_lossy(&flux.stderr);
        assert!(diagnostics.contains(expected), "{name}: {diagnostics}");
    }
}

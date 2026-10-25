//! Normalize projections before splitting region rows, preserving callable variance.
use std::process::Command;

#[test]
fn projected_pointer_regions_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-projected-pointer-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, item, container, value, output, accepted) in [
        (
            "option_specialize",
            "&'a fn(&u32)->&u32",
            "Option<Self::Item>",
            "Some(self.p)",
            "Option<&'a fn(&'static u32)->&'static u32>",
            true,
        ),
        (
            "array_specialize",
            "&'a fn(&u32)->&u32",
            "[Self::Item;1]",
            "[self.p]",
            "[&'a fn(&'static u32)->&'static u32;1]",
            true,
        ),
        (
            "nested_rows",
            "(&'a u32,&'a fn(&u32)->&u32,&'b u32)",
            "Option<(u32,Self::Item)>",
            "Some((7,(self.left,self.p,self.right)))",
            "Option<(u32,(&'a u32,&'a fn(&'static u32)->&'static u32,&'b u32))>",
            true,
        ),
        (
            "nested_first_escape",
            "(&'a u32,&'a fn(&u32)->&u32,&'b u32)",
            "Option<(u32,Self::Item)>",
            "Some((7,(self.left,self.p,self.right)))",
            "Option<(u32,(&'b u32,&'a fn(&'static u32)->&'static u32,&'b u32))>",
            false,
        ),
        (
            "nested_last_escape",
            "(&'a u32,&'a fn(&u32)->&u32,&'b u32)",
            "Option<(u32,Self::Item)>",
            "Some((7,(self.left,self.p,self.right)))",
            "Option<(u32,(&'a u32,&'a fn(&'static u32)->&'static u32,&'a u32))>",
            false,
        ),
        (
            "wrong_output",
            "&'a fn(&u32)->&u32",
            "Option<Self::Item>",
            "Some(self.p)",
            "Option<&'a fn(&u32)->&'static u32>",
            false,
        ),
        (
            "invariant_specialize",
            "&'a mut fn(&u32)->&u32",
            "Option<Self::Item>",
            "None",
            "Option<&'a mut fn(&'static u32)->&'static u32>",
            false,
        ),
        (
            "invariant_identity",
            "&'a mut fn(&u32)->&u32",
            "Option<Self::Item>",
            "None",
            "Option<&'a mut fn(&u32)->&u32>",
            true,
        ),
    ] {
        let source = format!(
            "trait Next {{ type Item; fn next(&mut self)->{container}; }}\n\
             struct Source<'a,'b> {{ left:&'a u32, p:{pointer}, right:&'b u32 }}\n\
             impl<'a,'b> Next for Source<'a,'b> {{\n\
                 type Item={item}; fn next(&mut self)->{container} {{ {value} }}\n\
             }}\n\
             fn get<'a,'b>(source:&mut Source<'a,'b>)->{output} {{ source.next() }}\n\
             fn main() {{}}\n",
            pointer = if name.starts_with("invariant") {
                "&'a mut fn(&u32)->&u32"
            } else {
                "&'a fn(&u32)->&u32"
            },
        );
        check(&root, name, &source, accepted);
    }
    std::fs::remove_dir_all(root).unwrap();
}

fn check(root: &std::path::Path, name: &str, source: &str, accepted: bool) {
    let path = root.join(format!("{name}.rs"));
    std::fs::write(&path, source).unwrap();
    let oracle = Command::new("rustc")
        .args(["--edition=2024", "--emit=metadata"])
        .arg(&path)
        .arg("-o")
        .arg(root.join(format!("{name}.rmeta")))
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
        "MIR {name}: {:?}",
        mir.diagnostics
    );
}

#[test]
fn projected_pointer_borrow_cannot_escape() {
    let root = std::env::temp_dir().join(format!(
        "flux-projected-pointer-escape-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    check(
        &root,
        "escape",
        r#"
        trait Next { type Item; fn next(&mut self) -> Option<Self::Item>; }
        struct Source<'a> { p: &'a fn(&u32) -> &u32 }
        impl<'a> Next for Source<'a> {
            type Item = &'a fn(&u32) -> &u32;
            fn next(&mut self) -> Option<Self::Item> { Some(self.p) }
        }
        fn get<'a>(source: &mut Source<'a>) -> Option<&'a fn(&'static u32) -> &'static u32> {
            source.next()
        }
        fn id(x: &u32) -> &u32 { x }
        fn main() {
            static VALUE: u32 = 7;
            let escaped;
            {
                let pointer: fn(&u32) -> &u32 = id;
                escaped = get(&mut Source { p: &pointer }).unwrap();
            }
            assert_eq!(*escaped(&VALUE), 7);
        }
    "#,
        false,
    );
    std::fs::remove_dir_all(root).unwrap();
}

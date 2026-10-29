//! Recursive ADT fields resolve Self in their declaration owner.
use flux_hir::{SourceCrate, SourceDb};
use flux_intern::Symbol;
use flux_syntax::Edition;
#[test]
fn adt_self_owner_matches_rustc() {
    let root = std::env::temp_dir().join(format!("flux-adt-self-owner-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "enum_self",
            "enum Tree<I,C=u32>{Base(I),Stack{base:Box<Self>,stack:Vec<C>},Alt(Vec<Self>)}fn main(){}",
            true,
        ),
        (
            "derive_enum_self",
            "#[derive(Debug)]enum Tree<I,C=u32>{Base(I),Stack{base:Box<Self>,stack:Vec<C>},Alt(Vec<Self>)}fn main(){}",
            true,
        ),
        (
            "struct_self",
            "struct Tree<T>{value:T,children:Vec<Self>}fn main(){}",
            true,
        ),
        (
            "borrowed_self",
            "struct Tree<'a>{value:&'a u32,children:Vec<Self>}fn main(){let x=7;let t=Tree{value:&x,children:Vec::new()};assert_eq!(*t.value,7);}",
            true,
        ),
        (
            "const_generic_self",
            "struct Tree<const N:usize>{value:[u8;N],children:Vec<Self>}fn main(){let t=Tree::<2>{value:[1,2],children:Vec::new()};assert_eq!(t.value[1],2);}",
            true,
        ),
        (
            "nested_container_self",
            "enum Tree<T>{Leaf(T),Nodes(Vec<Box<Self>>)}fn main(){}",
            true,
        ),
        (
            "inline_const_owner",
            "struct A;impl A{fn f()->usize{const{core::mem::size_of::<Self>()}}}fn main(){assert_eq!(A::f(),0);}",
            true,
        ),
        (
            "self_outside_owner",
            "fn main(){let _:Option<Self>=None;}",
            false,
        ),
        (
            "named_item_cannot_capture_self",
            "struct A;impl A{fn f(){const N:usize=core::mem::size_of::<Self>();}}fn main(){}",
            false,
        ),
        (
            "borrow_escape",
            "struct Tree<'a>{value:&'a u32,children:Vec<Self>}fn bad()->Tree<'static>{let x=7;Tree{value:&x,children:Vec::new()}}fn main(){}",
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
        let mut errors = hir.diagnostics.clone();
        errors.extend(typed.diagnostics.clone());
        if errors.is_empty() {
            errors.extend(flux_mir::build_file(&hir, &typed).diagnostics);
        }
        assert_eq!(errors.is_empty(), accepted, "Flux {name}: {errors:?}");
    }
    std::fs::remove_dir_all(root).unwrap();
}

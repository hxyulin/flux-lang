//! Associated constants are callable values, with the same checks as local pointers.
use flux_hir::{SourceCrate, SourceDb};
use flux_intern::Symbol;
use flux_syntax::Edition;

#[test]
fn associated_constant_calls_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-associated-calls-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "generic",
            "fn id<T>(x:T)->T{x}struct H<T>(T);impl<T> H<T>{const F:fn(T)->T=id::<T>;}fn f<T>(x:T)->T{H::<T>::F(x)}fn main(){assert_eq!(f(3),3);}",
            true,
        ),
        (
            "trait_constant",
            "trait C{const F:fn(u32)->u32;}struct H;impl C for H{const F:fn(u32)->u32=|x|x+1;}fn f<T:C>()->u32{T::F(2)}fn main(){assert_eq!(f::<H>(),3);}",
            true,
        ),
        (
            "static_output",
            "static X:u32=3;fn get()->&'static u32{&X}struct H;impl H{const F:fn()->&'static u32=get;}fn f()->&'static u32{H::F()}fn main(){}",
            true,
        ),
        (
            "borrowed_pointer",
            "fn id(x:u32)->u32{x}struct H;impl H{const F:&'static fn(u32)->u32=&(id as fn(u32)->u32);}fn main(){assert_eq!(H::F(3),3);}",
            true,
        ),
        (
            "returned_borrowed_pointer",
            "fn id(x:u32)->u32{x}static ID:fn(u32)->u32=id;fn get()->&'static fn(u32)->u32{&ID}struct H;impl H{const F:fn()->&'static fn(u32)->u32=get;}fn main(){assert_eq!(H::F()(3),3);}",
            true,
        ),
        (
            "linked_output",
            "fn id(x:&u32)->&u32{x}struct H;impl H{const F:fn(&u32)->&u32=id;}fn f(x:&u32)->&u32{H::F(x)}fn main(){}",
            true,
        ),
        (
            "unsafe_pointer",
            "unsafe fn id(x:u32)->u32{x}struct H;impl H{const F:unsafe fn(u32)->u32=id;}fn main(){assert_eq!(unsafe{H::F(3)},3);}",
            true,
        ),
        (
            "c_pointer",
            "extern \"C\" fn id(x:u32)->u32{x}struct H;impl H{const F:extern \"C\" fn(u32)->u32=id;}fn main(){assert_eq!(H::F(3),3);}",
            true,
        ),
        (
            "owner_bound",
            "trait Bound{}impl Bound for u32{}fn id<T>(x:T)->T{x}struct H<T>(T);impl<T:Bound>H<T>{const F:fn(T)->T=id::<T>;}fn f<T:Bound>(x:T)->T{H::<T>::F(x)}fn main(){}",
            true,
        ),
        (
            "missing_owner_bound",
            "trait Bound{}fn id<T>(x:T)->T{x}struct H<T>(T);impl<T:Bound>H<T>{const F:fn(T)->T=id::<T>;}fn f<T>(x:T)->T{H::<T>::F(x)}fn main(){}",
            false,
        ),
        (
            "noncallable",
            "struct H;impl H{const F:u32=3;}fn main(){H::F();}",
            false,
        ),
        (
            "wrong_argument",
            "struct H;impl H{const F:fn(u32)->u32=|x|x;}fn main(){H::F(false);}",
            false,
        ),
        (
            "wrong_output",
            "struct H;impl H{const F:fn(u32)->u32=|x|x;}fn main(){let _:bool=H::F(3);}",
            false,
        ),
        (
            "unsafe_outside_block",
            "unsafe fn id(x:u32)->u32{x}struct H;impl H{const F:unsafe fn(u32)->u32=id;}fn main(){H::F(3);}",
            false,
        ),
        (
            "const_call",
            "struct H;impl H{const F:fn(u32)->u32=|x|x;}const X:u32=H::F(3);fn main(){}",
            false,
        ),
        (
            "borrowed_output_escape",
            "fn id(x:&u32)->&u32{x}struct H;impl H{const F:fn(&u32)->&u32=id;}fn f(x:&u32)->&'static u32{H::F(x)}fn main(){}",
            false,
        ),
    ] {
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let rust = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("r.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            rust.status.success(),
            accepted,
            "Rust {name}: {}",
            String::from_utf8_lossy(&rust.stderr)
        );
        let mut db = SourceDb::new();
        let file = db.add_with_edition(
            format!("/associated-calls/{name}.rs"),
            source,
            Edition::E2024,
        );
        let hir = flux_hir::lower_sources(
            &mut db,
            &[SourceCrate {
                root: file,
                name: Symbol::intern(name),
                dependencies: vec![],
            }],
        );
        let typed = flux_ty::check_file(&hir);
        let mut errors: Vec<_> = hir
            .diagnostics
            .iter()
            .chain(&typed.diagnostics)
            .map(|d| d.message.clone())
            .collect();
        if errors.is_empty() {
            errors.extend(
                flux_mir::build_file(&hir, &typed)
                    .diagnostics
                    .into_iter()
                    .map(|d| d.message),
            );
        }
        assert_eq!(errors.is_empty(), accepted, "Flux {name}: {errors:?}");
    }
    std::fs::remove_dir_all(root).unwrap();
}

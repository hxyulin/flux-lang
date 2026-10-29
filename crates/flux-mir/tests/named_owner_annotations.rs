//! Named annotations preserve implicit impl-owner region slots.
use flux_hir::{SourceCrate, SourceDb};
use flux_intern::Symbol;
use flux_syntax::Edition;
#[test]
fn named_owner_rows_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-named-owner-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "reference_impl",
            "trait Read<'a>{fn read(self)->&'a[u8];}struct D<'a>(&'a[u8]);impl<'a>Read<'a>for &mut D<'a>{fn read(self)->&'a[u8]{let bytes:&'a[u8]=self.0;bytes}}fn main(){}",
            true,
        ),
        (
            "method_region_after_anonymous_owner",
            "trait Read{fn read<'b>(self,x:&'b[u8])->&'b[u8];}struct D;impl Read for &mut D{fn read<'b>(self,x:&'b[u8])->&'b[u8]{let bytes:&'b[u8]=x;bytes}}fn main(){}",
            true,
        ),
        (
            "mixed_owner_method",
            "trait Read<'a>{fn read<'b>(self,x:&'b[u8])->(&'a[u8],&'b[u8]);}struct D<'a>(&'a[u8]);impl<'a>Read<'a>for &mut D<'a>{fn read<'b>(self,x:&'b[u8])->(&'a[u8],&'b[u8]){let left:&'a[u8]=self.0;let right:&'b[u8]=x;(left,right)}}fn main(){}",
            true,
        ),
        (
            "multiple_anonymous_owner",
            "trait Read<'a>{fn read(self)->&'a[u8];}impl<'a>Read<'a>for (&mut &'a[u8],&u8){fn read(self)->&'a[u8]{let bytes:&'a[u8]=*self.0;bytes}}fn main(){}",
            true,
        ),
        (
            "wrong_method_link",
            "trait Read<'a>{fn read<'b>(self,x:&'b[u8])->&'a[u8];}struct D<'a>(&'a[u8]);impl<'a>Read<'a>for &mut D<'a>{fn read<'b>(self,x:&'b[u8])->&'a[u8]{let bytes:&'a[u8]=x;bytes}}fn main(){}",
            false,
        ),
        (
            "short_self_borrow",
            "trait Read<'a>{fn read(self)->&'a u8;}struct D(u8);impl<'a>Read<'a>for &mut D{fn read(self)->&'a u8{let value:&'a u8=&self.0;value}}fn main(){}",
            false,
        ),
        (
            "mutable_invariance",
            "fn bad<'a,'b>(x:&mut &'a u8,y:&'b u8){let result:&mut &'a u8=x;*result=y;}fn main(){}",
            false,
        ),
        (
            "box_borrow_wrong_target",
            "fn main(){let x=Box::new(1u8);let _: &u32=std::borrow::Borrow::<u32>::borrow(&x);}",
            false,
        ),
        (
            "box_borrow_mut_shared",
            "fn main(){let x=Box::new(1u8);let _: &mut u8=std::borrow::BorrowMut::<u8>::borrow_mut(&x);}",
            false,
        ),
        (
            "box_borrow_escape",
            "fn bad()->&'static u8{let x=Box::new(1u8);std::borrow::Borrow::<u8>::borrow(&x)}fn main(){}",
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

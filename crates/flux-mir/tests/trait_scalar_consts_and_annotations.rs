//! Lifetime traits retain scalar const contracts and local annotation rows.
use flux_hir::{SourceCrate, SourceDb};
use flux_intern::Symbol;
use flux_syntax::Edition;
#[test]
fn contracts_match_rustc() {
    let root = std::env::temp_dir().join(format!(
        "flux-scalar-trait-contracts-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "scalar_types",
            "trait Read<'a>{const FLAG:bool;const BYTE:u8;const CH:char;const FLOAT:f64;fn get(&self)->&'a str;}fn main(){}",
            true,
        ),
        (
            "default_scalar",
            "trait Read<'a>{const FLAG:bool=true;fn get(&self)->&'a str;}struct R<'a>(&'a str);impl<'a>Read<'a>for R<'a>{fn get(&self)->&'a str{self.0}}fn main(){assert!(R::FLAG);}",
            true,
        ),
        (
            "generic_trait",
            "trait Read<'a,T>{const FLAG:bool;fn get(&self)->&'a T;}fn main(){}",
            true,
        ),
        (
            "two_origin_read",
            "enum Ref<'a,'s>{Data(&'a str),Scratch(&'s str)}trait Read<'a>{const FLAG:bool;fn read<'s>(&'s mut self,scratch:&'s mut String)->Ref<'a,'s>;}struct R<'a>(&'a str);impl<'a>Read<'a>for R<'a>{const FLAG:bool=false;fn read<'scratch>(&'scratch mut self,_scratch:&'scratch mut String)->Ref<'a,'scratch>{Ref::Data(self.0)}}fn use_read<'a,'s>(r:&'s mut impl Read<'a>,s:&'s mut String)->Ref<'a,'s>{r.read(s)}fn main(){}",
            true,
        ),
        (
            "wrong_const_type",
            "trait Read<'a>{const FLAG:bool;fn get(&self)->&'a str;}struct R<'a>(&'a str);impl<'a>Read<'a>for R<'a>{const FLAG:u8=1;fn get(&self)->&'a str{self.0}}fn main(){}",
            false,
        ),
        (
            "missing_const",
            "trait Read<'a>{const FLAG:bool;fn get(&self)->&'a str;}struct R<'a>(&'a str);impl<'a>Read<'a>for R<'a>{fn get(&self)->&'a str{self.0}}fn main(){}",
            false,
        ),
        (
            "wrong_read_origin",
            "enum Ref<'a,'s>{Data(&'a str),Scratch(&'s str)}trait Read<'a>{const FLAG:bool;fn read<'s>(&'s mut self,scratch:&'s mut String)->Ref<'a,'s>;}struct R<'a>(&'a str);impl<'a>Read<'a>for R<'a>{const FLAG:bool=false;fn read<'s>(&'s mut self,scratch:&'s mut String)->Ref<'a,'s>{Ref::Data(scratch.as_str())}}fn main(){}",
            false,
        ),
        (
            "short_scratch_escape",
            "enum Ref<'a,'s>{Data(&'a str),Scratch(&'s str)}trait Read<'a>{const FLAG:bool;fn read<'s>(&'s mut self,scratch:&'s mut String)->Ref<'a,'s>;}fn escape<'a>(r:&mut impl Read<'a>)->&'a str{let mut scratch=String::new();match r.read(&mut scratch){Ref::Data(x)|Ref::Scratch(x)=>x}}fn main(){}",
            false,
        ),
        (
            "named_input",
            "fn read<'a>(input:&'a[u8])->&'a[u8]{let bytes:&'a[u8]=input;bytes}fn main(){}",
            true,
        ),
        (
            "named_owner",
            "struct R<'a>(&'a[u8]);impl<'a>R<'a>{fn read(&self)->&'a[u8]{let bytes:&'a[u8]=self.0;bytes}}fn main(){}",
            true,
        ),
        (
            "named_trait_impl_owner",
            "trait Read<'a>{fn get(&self)->&'a[u8];}struct R<'a>(&'a[u8]);impl<'a>Read<'a>for R<'a>{fn get(&self)->&'a[u8]{let bytes:&'a[u8]=self.0;bytes}}fn main(){}",
            true,
        ),
        (
            "named_alias",
            "type Slice<'a>=&'a[u8];fn read<'a>(input:Slice<'a>)->Slice<'a>{let bytes:Slice<'a>=input;bytes}fn main(){}",
            true,
        ),
        (
            "named_destructure",
            "fn read<'a>(input:(&'a u8,&'a u8))->&'a u8{let (a,b):(&'a u8,&'a u8)=input;let _=b;a}fn main(){}",
            true,
        ),
        (
            "named_assignment",
            "fn read<'a>(x:&'a u8,y:&'a u8,which:bool)->&'a u8{let result:&'a u8;if which{result=x}else{result=y}result}fn main(){}",
            true,
        ),
        (
            "named_reassignment",
            "fn read<'a>(x:&'a u8,y:&'a u8)->&'a u8{let mut result:&'a u8=x;let _=result;result=y;result}fn main(){}",
            true,
        ),
        (
            "unrelated_owner",
            "fn wrong<'a,'b>(x:&'a u8)->&'b u8{let v:&'b u8=x;v}fn main(){}",
            false,
        ),
        (
            "local_escape",
            "fn wrong<'a>()->&'a u8{let x=1;let v:&'a u8=&x;v}fn main(){}",
            false,
        ),
        (
            "named_late_assignment_escape",
            "fn wrong<'a>(x:&'a u8){let mut result:&'a u8=x;{let y=2;result=&y;}println!(\"{}\",result);}fn main(){}",
            false,
        ),
        (
            "unknown_lifetime",
            "fn wrong(x:&u8){let v:&'unknown u8=x;let _=v;}fn main(){}",
            false,
        ),
        (
            "named_mutable_invariance",
            "fn wrong<'a,'b>(x:&'a mut &'a u8)->&'a mut &'b u8{let v:&'a mut &'b u8=x;v}fn main(){}",
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

#[test]
fn scalar_const_profile_requires_complete_empty_rows() {
    let source = "trait Read<'a>{const FLAG:bool;fn get(&self)->&'a str;}fn main(){}";
    let (_, _, hir) = flux_hir::lower_text("/scalar-profile.rs", source);
    let typed = flux_ty::check_file(&hir);
    assert!(hir.diagnostics.is_empty());
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let (id, data) = hir
        .items
        .traits
        .iter()
        .find(|(_, t)| t.name.as_str() == "Read" && t.generics.lifetimes.len() == 1)
        .unwrap();
    assert!(
        flux_ty::regions::safe_method_trait_profile(&hir, &typed.sigs, id).is_ok(),
        "{:?}",
        flux_ty::regions::safe_method_trait_profile(&hir, &typed.sigs, id)
    );
    let konst = data.consts[0];
    let mut sigs = typed.sigs.clone();
    sigs.regions.consts.insert(
        konst,
        vec![flux_ty::regions::Region::Param(0)].into_boxed_slice(),
    );
    assert!(flux_ty::regions::safe_method_trait_profile(&hir, &sigs, id).is_err());
    let mut sigs = typed.sigs.clone();
    sigs.consts.insert(konst, flux_ty::Ty::STR);
    assert!(flux_ty::regions::safe_method_trait_profile(&hir, &sigs, id).is_err());
}

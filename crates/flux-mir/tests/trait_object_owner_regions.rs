//! Object lifetime bounds and receiver loans occupy independent signature rows.
use flux_hir::{SourceCrate, SourceDb};
use flux_intern::Symbol;
use flux_syntax::Edition;
const COMMON: &str = "trait Obj{}struct Borrow<'data>(&'data u32);impl Obj for Borrow<'_>{}trait AsObj<'owner>{fn view(&self)->&(dyn Obj+'owner);}impl<'owner,T:Obj+'owner>AsObj<'owner>for T{fn view(&self)->&(dyn Obj+'owner){self}}impl<'owner>AsObj<'owner>for dyn Obj+'owner{fn view(&self)->&(dyn Obj+'owner){self}}";
fn diagnostics(name: &str, source: &str) -> (flux_hir::Hir, flux_ty::TypedFile, Vec<String>) {
    let mut db = SourceDb::new();
    let file = db.add_with_edition(format!("/object-owner/{name}.rs"), source, Edition::E2021);
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
    (hir, typed, errors)
}
#[test]
fn object_owner_contracts_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-object-owner-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, body, accepted) in [
        (
            "blanket",
            "fn use_it<'a,T:AsObj<'a>>(x:&'a T)->&'a(dyn Obj+'a){x.view()}fn main(){}",
            true,
        ),
        (
            "independent_receiver",
            "fn use_it<'data:'borrow,'borrow,T:AsObj<'data>+?Sized>(x:&'borrow T)->&'borrow(dyn Obj+'data){x.view()}fn main(){}",
            true,
        ),
        (
            "borrowed_payload",
            "fn use_it<'data:'owner,'owner:'borrow,'borrow>(x:&'borrow Borrow<'data>)->&'borrow(dyn Obj+'owner){x.view()}fn main(){}",
            true,
        ),
        (
            "short_receiver",
            "fn bad<'a>(data:&'a u32)->&'a(dyn Obj+'a){let value=Borrow(data);value.view()}fn main(){}",
            false,
        ),
        (
            "missing_payload_outlives",
            "fn bad<'data,'owner:'borrow,'borrow>(x:&'borrow Borrow<'data>)->&'borrow(dyn Obj+'owner){x.view()}fn main(){}",
            false,
        ),
        (
            "static_only",
            "fn require_static(_: &(dyn Obj+'static)){}fn bad<'a,T:AsObj<'a>+?Sized>(x:&T){require_static(x.view())}fn main(){}",
            false,
        ),
        (
            "owner_output_swap",
            "trait Bad<'a>{fn view(&self)->&(dyn Obj+'a);}struct Pair<'a,'b>{a:Borrow<'a>,b:Borrow<'b>}impl<'a,'b>Bad<'a>for Pair<'a,'b>{fn view(&self)->&(dyn Obj+'a){&self.b}}fn main(){}",
            false,
        ),
        (
            "fixed_receiver",
            "trait Bad<'a>{fn view<'s>(&'s self)->&'s(dyn Obj+'a);}impl<'a>Bad<'a>for Borrow<'a>{fn view(&'a self)->&'a(dyn Obj+'a){self}}fn main(){}",
            false,
        ),
        (
            "default_object_argument",
            "trait Visit<'doc>{fn visit(&mut self,node:&'doc dyn Obj){forward(self,node)}}fn forward<'doc,V:Visit<'doc>+?Sized>(v:&mut V,node:&'doc dyn Obj){let _=(v,node);}fn main(){}",
            true,
        ),
        (
            "object_argument_escape",
            "trait Visit<'doc>{fn visit(&mut self,node:&'doc dyn Obj);}fn bad<'a,V:Visit<'a>>(v:&mut V){let value=3;v.visit(&Borrow(&value))}fn main(){}",
            false,
        ),
        (
            "closed_object_type_argument",
            "trait Node<T>{}trait Get<'a>{fn get(&self)->&(dyn Node<u32>+'a);}fn main(){}",
            true,
        ),
        (
            "closed_associated_object",
            "trait Node{type Out;}trait Get<'a>{fn get(&self)->&(dyn Node<Out=u32>+'a);}fn main(){}",
            true,
        ),
    ] {
        let source = format!("{COMMON}{body}");
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, &source).unwrap();
        let out = std::process::Command::new("rustc")
            .args(["--edition=2021", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("r.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            out.status.success(),
            accepted,
            "Rust {name}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let (_, _, errors) = diagnostics(name, &source);
        assert_eq!(errors.is_empty(), accepted, "Flux {name}: {errors:?}");
    }
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn object_profile_reconstructs_both_rows() {
    let source = format!("{COMMON}fn main(){{}}");
    let (hir, typed, errors) = diagnostics("metadata", &source);
    assert!(errors.is_empty(), "{errors:?}");
    let (id, data) = hir
        .items
        .traits
        .iter()
        .find(|(_, t)| t.name.as_str() == "AsObj")
        .unwrap();
    assert!(flux_ty::regions::required_method_trait_profile(&hir, &typed.sigs, id).is_ok());
    let method = data.functions[0];
    let mut sigs = typed.sigs.clone();
    let row = sigs.regions.fns.get_mut(method).unwrap();
    assert_eq!(row.ret.len(), 2);
    row.ret.swap(0, 1);
    assert!(flux_ty::regions::required_method_trait_profile(&hir, &sigs, id).is_err());
}
#[test]
fn unrepresented_object_shapes_stay_explicit() {
    for source in [
        "trait Get<'a>{fn get(&self)->&(dyn Fn()->u32+'a);}fn main(){}",
        "trait Node<T>{}trait Get<'a>{fn get(&self)->&(dyn Node<fn(&'a u32)>+'a);}fn main(){}",
        "trait Get<'a>{fn get(&self)->fn(&'a u32);}fn main(){}",
    ] {
        let (_, _, errors) = diagnostics("unsupported", source);
        assert!(!errors.is_empty(), "admitted {source}");
        assert!(
            errors.iter().any(|e| e.contains("not supported")),
            "{errors:?}"
        );
    }
}

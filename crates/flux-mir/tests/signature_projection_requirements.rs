//! Signature projections retain selected impl prerequisites before normalization.
use flux_hir::{SourceCrate, SourceDb};
use flux_intern::Symbol;
use flux_syntax::Edition;

#[test]
fn signature_projection_requirements_match_rustc() {
    let root =
        std::env::temp_dir().join(format!("flux-signature-projections-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "family_header_and_declaration_equalities",
            "trait Marker{type Eq;}trait Part<'d>{fn read(&self)->&'d u32;}trait Obj<'d>:Marker<Eq=u32>{type P<'f>:Part<'d>where Self:'f,'d:'f;type I<'f>:Iterator<Item=Self::P<'f>>where Self:'f,'d:'f;fn iter(&self)->Self::I<'_>;fn find(&self)->Option<Self::P<'_>>;}fn main(){}",
            true,
        ),
        (
            "family_where_self_and_declaration_equalities",
            "trait Marker{type Eq;}trait Obj where Self:Marker<Eq=u32>{type P<'f>where Self:'f;type I<'f>:Iterator<Item=Self::P<'f>>where Self:'f;fn iter(&self)->Self::I<'_>;fn find(&self)->Option<Self::P<'_>>;}fn main(){}",
            true,
        ),
        (
            "family_generic_input",
            "trait G{type O<'a>where Self:'a;}fn f<'a,T:G>(_:T::O<'a>){}fn main(){}",
            true,
        ),
        (
            "family_generic_output",
            "trait G{type O<'a>where Self:'a;}fn f<'a,T:G>()->Option<T::O<'a>>{None}fn main(){}",
            true,
        ),
        (
            "family_static_output_declaration",
            "trait G{type O<'a>where Self:'a;}fn f<T:G>(x:T)->Option<T::O<'static>>{let _=x;None}fn main(){}",
            true,
        ),
        (
            "family_static_output_call",
            "trait G{type O<'a>where Self:'a;}impl<'s>G for &'s u32{type O<'a>=&'a u32 where Self:'a;}fn f<T:G>(x:T)->Option<T::O<'static>>{let _=x;None}fn g(x:&u32){let _:Option<&'static u32>=f(x);}fn main(){}",
            false,
        ),
        (
            "named_owner_associated_output",
            "trait Source<'a>{type Error;}struct W<A>(A);trait Convert<'a,E>{type Output;fn convert(self)->Self::Output;}impl<'a,A:Source<'a>>Convert<'a,A::Error>for W<A>{type Output=Self;fn convert(self)->Self{self}}fn main(){}",
            true,
        ),
        (
            "borrowed_owner_input_output",
            "trait Stream{type Out;}trait Append<I:'static>where &'static I:Stream{fn append(x:&'static I)->(&'static I,Option<<&'static I as Stream>::Out>);}fn main(){}",
            true,
        ),
        (
            "static_borrowed_owner_predicate",
            "trait Stream{type Out;}trait Append<I:'static>where &'static I:Stream{fn append(x:&'static I)->Option<<&'static I as Stream>::Out>;}fn main(){}",
            true,
        ),
        (
            "missing_static_scalar",
            r###"trait S{type Item;}trait C{type Item;}impl<T:S+'static>C for T{type Item=u32;}fn f<T:S>(x:T)->Option<<T as C>::Item>{let _=x;None}fn main(){}"###,
            false,
        ),
        (
            "explicit_static_scalar",
            r###"trait S{type Item;}trait C{type Item;}impl<T:S+'static>C for T{type Item=u32;}fn f<T:S+'static>(x:T)->Option<<T as C>::Item>{let _=x;None}fn main(){}"###,
            true,
        ),
        (
            "implied_static_input",
            r###"trait S{type Item;}trait C{type Item;}impl<T:S+'static>C for T{type Item=u32;}fn f<T:S>(_:&'static T)->Option<<T as C>::Item>{None}fn main(){}"###,
            true,
        ),
        (
            "missing_static_borrowed",
            r###"trait S{type Item;}trait C{type Item;}impl<T:S+'static>C for T{type Item=&'static u32;}fn f<T:S>(x:T)->Option<<T as C>::Item>{let _=x;None}fn main(){}"###,
            false,
        ),
        (
            "explicit_static_borrowed",
            r###"trait S{type Item;}trait C{type Item;}impl<T:S+'static>C for T{type Item=&'static u32;}fn f<T:S+'static>(x:T)->Option<<T as C>::Item>{let _=x;None}fn main(){}"###,
            true,
        ),
        (
            "zero_length_array",
            r###"trait S{type Item;}trait C{type Item;}impl<T:S+'static>C for T{type Item=u32;}fn f<T:S>(_:T)->[<T as C>::Item;0]{[]}fn main(){}"###,
            false,
        ),
        (
            "input_scalar",
            r###"trait S{type Item;}trait C{type Item;}impl<T:S+'static>C for T{type Item=u32;}fn f<T:S>(_:Option<<T as C>::Item>){}fn main(){}"###,
            false,
        ),
        (
            "input_scalar_static",
            r###"trait S{type Item;}trait C{type Item;}impl<T:S+'static>C for T{type Item=u32;}fn f<T:S+'static>(_:Option<<T as C>::Item>){}fn main(){}"###,
            true,
        ),
        (
            "required_method",
            r###"trait S{type Item;}trait C{type Item;}impl<T:S+'static>C for T{type Item=u32;}trait D{fn f<T:S>(x:T)->Option<<T as C>::Item>;}fn main(){}"###,
            false,
        ),
        (
            "required_method_static",
            r###"trait S{type Item;}trait C{type Item;}impl<T:S+'static>C for T{type Item=u32;}trait D{fn f<T:S+'static>(x:T)->Option<<T as C>::Item>;}fn main(){}"###,
            true,
        ),
        (
            "owner_missing",
            r###"trait S{type Item;}trait C{type Item;}impl<T:S+'static>C for T{type Item=u32;}struct W<T>(T);impl<T:S>W<T>{fn f(self)->Option<<T as C>::Item>{None}}fn main(){}"###,
            false,
        ),
        (
            "owner_static",
            r###"trait S{type Item;}trait C{type Item;}impl<T:S+'static>C for T{type Item=u32;}struct W<T>(T);impl<T:S+'static>W<T>{fn f(self)->Option<<T as C>::Item>{None}}fn main(){}"###,
            true,
        ),
        (
            "original_membership",
            r###"trait S{type Item;}trait C{type Item;}impl<T:S+'static>C for T{type Item=u32;}fn f<T:S+C>(x:T)->Option<<T as C>::Item>{let _=x;None}fn main(){}"###,
            true,
        ),
        (
            "nested_option_tuple",
            r###"trait S{type Item;}trait C{type Item;}impl<T:S+'static>C for T{type Item=u32;}fn f<T:S>(x:T)->Option<(<T as C>::Item,u32)>{let _=x;None}fn main(){}"###,
            false,
        ),
        (
            "conditional_missing",
            r###"trait S{}trait Mark{}trait C{type Item;}impl<T:S+Mark+'static>C for T{type Item=u32;}fn f<T:S+'static>(_:T)->Option<<T as C>::Item>{None}fn main(){}"###,
            false,
        ),
        (
            "conditional_valid",
            r###"trait S{}trait Mark{}trait C{type Item;}impl<T:S+Mark+'static>C for T{type Item=u32;}fn f<T:S+Mark+'static>(_:T)->Option<<T as C>::Item>{None}fn main(){}"###,
            true,
        ),
        (
            "borrowed_selected_input",
            r###"trait S{}trait C{type Item;}impl<'a,T:S+'static>C for &'a T{type Item=u32;}fn f<'a,T:S>(_:&'a T)->Option<<&'a T as C>::Item>{None}fn main(){}"###,
            false,
        ),
        (
            "borrowed_selected_input_static",
            r###"trait S{}trait C{type Item;}impl<'a,T:S+'static>C for &'a T{type Item=u32;}fn f<'a,T:S+'static>(_:&'a T)->Option<<&'a T as C>::Item>{None}fn main(){}"###,
            true,
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
            format!("/signature-projections/{name}.rs"),
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

#[test]
fn missing_signature_selection_rows_fail_closed() {
    let source = "trait S{}trait C{type Item;}impl<'a,T:S+'static>C for &'a T{type Item=u32;}fn f<'a,T:S+'static>(_:&'a T)->Option<<&'a T as C>::Item>{None}fn main(){}";
    let (_, _, hir) = flux_hir::lower_text("selection_rows.rs", source);
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let original = flux_ty::check_file(&hir);
    assert!(
        original.diagnostics.is_empty(),
        "{:?}",
        original.diagnostics
    );
    assert!(flux_mir::build_file(&hir, &original).diagnostics.is_empty());
    let function = hir
        .items
        .functions
        .iter()
        .find(|(_, data)| data.name.as_str() == "f")
        .unwrap()
        .0;
    for corruption in 0..4 {
        let mut typed = flux_ty::check_file(&hir);
        let rows = &mut typed
            .sigs
            .regions
            .fns
            .get_mut(function)
            .unwrap()
            .selected_inputs;
        match corruption {
            0 => *rows = Box::default(),
            1 => rows[1] = Box::default(),
            2 => rows[1][0].args[0] = Box::default(),
            3 => rows[1][0].args[0] = vec![flux_ty::regions::Region::Static; 2].into(),
            _ => unreachable!(),
        }
        let mir = flux_mir::build_file(&hir, &typed);
        assert!(
            mir.diagnostics
                .iter()
                .any(|d| d.message.contains("projection requirements")),
            "corruption {corruption}: {:?}",
            mir.diagnostics
        );
    }
}

#[test]
fn missing_original_bound_rows_fail_closed() {
    let source = "trait S{type Out;}struct W;impl S for W{type Out=u32;}trait P<T>{type Out;}fn f<T:P<<W as S>::Out,Out=u8>>(x:<T as P<u32>>::Out)->u8{x}fn main(){}";
    let (_, _, hir) = flux_hir::lower_text("original_bound_rows.rs", source);
    let original = flux_ty::check_file(&hir);
    assert!(
        original.diagnostics.is_empty(),
        "{:?}",
        original.diagnostics
    );
    assert!(flux_mir::build_file(&hir, &original).diagnostics.is_empty());
    let function = hir
        .items
        .functions
        .iter()
        .find(|(_, data)| data.name.as_str() == "f")
        .unwrap()
        .0;
    for corruption in 0..3 {
        let mut typed = flux_ty::check_file(&hir);
        let metadata = typed.sigs.regions.fns.get_mut(function).unwrap();
        match corruption {
            0 => metadata.bound_selfs = Box::default(),
            1 => metadata.bound_traits[0].args = Box::default(),
            2 => {
                typed.sigs.fn_predicates.insert(function, Box::default());
            }
            _ => unreachable!(),
        }
        let diagnostics = flux_mir::build_file(&hir, &typed).diagnostics;
        assert!(
            diagnostics
                .iter()
                .any(|d| d.message.contains("projection requirements")),
            "corruption {corruption}: {diagnostics:?}"
        );
    }
}

//! Vtable metadata rebuilds each enclosing prefix without changing sized storage.
use flux_ty::{IntTy, Ty, TyKind};

#[test]
fn unsized_constructor_values_remain_rejected() {
    for source in [
        "struct W(dyn std::fmt::Debug);fn main(){let _constructor=W;}",
        "struct W<T:?Sized>(T);fn main(){let _constructor=W::<dyn std::fmt::Debug>;}",
    ] {
        let (_, _, hir) = flux_hir::lower_text("unsized-constructor.rs", source);
        let typed = flux_mir::check_file(&hir);
        let mut errors = hir.diagnostics.clone();
        errors.extend(typed.diagnostics.iter().cloned());
        if errors.is_empty() {
            errors.extend(flux_mir::build_file(&hir, &typed).diagnostics);
        }
        assert!(!errors.is_empty(), "accepted unsized constructor: {source}");
    }
}

#[test]
fn dynamic_prefix_layers_match_sized_instantiations() {
    let source = r#"
trait Value {}
struct Direct(dyn Value);
fn direct(_: &Direct){}
struct Header<T:?Sized>{small:u8,wide:u64,tail:T}
#[repr(C)]struct CHeader<T:?Sized>{small:u8,wide:u64,tail:T}
#[repr(C,align(64))]struct Aligned<T:?Sized>{small:u8,tail:T}
struct Outer<T:?Sized>{head:u32,inner:Header<T>}
#[repr(transparent)]struct Transparent<T:?Sized>(T);
fn header(_: &Header<dyn Value>){}
fn c_header(_: &CHeader<dyn Value>){}
fn aligned(_: &Aligned<dyn Value>){}
fn outer(_: &Outer<dyn Value>){}
fn transparent(_: &Transparent<dyn Value>){}
fn slice(_: &Header<[u8]>){}
fn main(){}
"#;
    let (_, file, hir) = flux_hir::lower_text("dynamic-prefix-layout.rs", source);
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let mir = flux_mir::build_file(&hir, &typed);
    assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
    let layouts = flux_mir::layout::Layouts::new(&hir.items, &typed.sigs, &mir.discriminants);
    let parameter = |name: &str| {
        let (id, _) = hir
            .items
            .functions
            .iter()
            .find(|(_, f)| f.loc.file == file && f.name.as_str() == name)
            .unwrap();
        let TyKind::Ref(_, pointee) = typed.sigs[id].params[0].kind() else {
            panic!("reference parameter");
        };
        pointee
    };
    for name in ["header", "c_header", "aligned", "outer", "transparent"] {
        let dynamic = parameter(name);
        let TyKind::Adt(adt, _) = dynamic.kind() else {
            panic!("struct parameter");
        };
        let source = Ty::adt(adt.strukt, adt.name, &[Ty::int(IntTy::U128)]);
        let descriptor = layouts
            .vtable_layout(dynamic)
            .unwrap_or_else(|| panic!("missing vtable layout for {name}: {dynamic:?}"));
        assert_eq!(descriptor.layers.len(), if name == "outer" { 2 } else { 1 });
        let (mut size, mut align) = (16u64, 16u64);
        for layer in descriptor.layers.iter().rev() {
            let tail_align = layer.pack.map_or(align, |pack| align.min(pack));
            let offset = layer.prefix_size.next_multiple_of(tail_align);
            align = layer.prefix_align.max(tail_align);
            size = (offset + size).next_multiple_of(align);
        }
        let physical = layouts.of(source);
        assert_eq!((size, align), (physical.size, physical.align), "{name}");
        let dynamic_layout = layouts.of(dynamic);
        let fields = flux_solve::fields(&typed.sigs, dynamic).unwrap();
        for index in 0..fields.len() - 1 {
            assert_eq!(
                dynamic_layout.field_offset(0, index as u32),
                physical.field_offset(0, index as u32),
                "{name} field {index}"
            );
        }
        let tail = typed.sigs.structural_tail(dynamic);
        assert!(layouts.vtable_layout(tail).unwrap().layers.is_empty());
    }
    let slice = parameter("slice");
    assert!(layouts.vtable_layout(slice).is_none());
    assert!(layouts.length_layout(slice).is_some());
}

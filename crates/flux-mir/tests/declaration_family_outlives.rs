//! Family output lifetimes come from the exact proven projection occurrence.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;
#[test]
fn forwarded_family_outlives_matches_rustc() {
    let root = std::env::temp_dir().join(format!("flux-family-outlives-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "reference_forward",
            "trait Raw{}trait Lock{type Write<'w>:Raw+'w where Self:'w;}impl<T:Lock+?Sized>Lock for &mut T{type Write<'w>=T::Write<'w>where Self:'w;}",
            true,
        ),
        (
            "box_forward",
            "trait Raw{}trait Lock{type Write<'w>:Raw+'w where Self:'w;}impl<T:Lock+?Sized>Lock for Box<T>{type Write<'w>=T::Write<'w>where Self:'w;}",
            true,
        ),
        (
            "two_family_parameters",
            "trait S{type Out<'a,'b>:'a where Self:'a,Self:'b;}struct W<T>(T);impl<T:S>S for W<T>{type Out<'a,'b>=T::Out<'a,'b>where Self:'a,Self:'b;}",
            true,
        ),
        (
            "family_reordered_correct",
            "trait S{type Out<'a,'b>:'a where Self:'a,Self:'b;}trait D{type Out<'a,'b>:'b where Self:'a,Self:'b;}struct W<T>(T);impl<T:S>D for W<T>{type Out<'a,'b>=T::Out<'b,'a>where Self:'a,Self:'b;}",
            true,
        ),
        (
            "family_reordered_wrong",
            "trait S{type Out<'a,'b>:'a where Self:'a,Self:'b;}struct W<T>(T);impl<T:S>S for W<T>{type Out<'a,'b>=T::Out<'b,'a>where Self:'a,Self:'b;}",
            false,
        ),
        (
            "owner_missing",
            "trait S{type Out<'a>:'a where Self:'a;}struct W<T>(T);impl<T>S for W<T>{type Out<'a>=T::Out<'a>where Self:'a;}",
            false,
        ),
        (
            "stricter_static_owner",
            "trait S{type Out<'a>:'a where Self:'a;}struct W<T>(T);impl<T:S>S for W<T>{type Out<'a>=T::Out<'a>where Self:'a,T:'static;}",
            false,
        ),
        (
            "unpromised_static_output",
            "trait S{type Out<'a>:'a where Self:'a;}trait D{type Out<'a>:'static where Self:'a;}struct W<T>(T);impl<T:S>D for W<T>{type Out<'a>=T::Out<'a>where Self:'a;}",
            false,
        ),
        (
            "selected_owner_static_missing",
            "trait S{type Out<'a>:'a where Self:'a;}struct A<T>(T);impl<T:'static>S for A<T>{type Out<'a>=&'a u32 where Self:'a;}struct W<T>(T);impl<T>S for W<T>{type Out<'a>=<A<T>as S>::Out<'a>where Self:'a;}",
            false,
        ),
        (
            "selected_owner_static_present",
            "trait S{type Out<'a>:'a where Self:'a;}struct A<T>(T);impl<T:'static>S for A<T>{type Out<'a>=&'a u32 where Self:'a;}struct W<T>(T);impl<T:'static>S for W<T>{type Out<'a>=<A<T>as S>::Out<'a>where Self:'a;}",
            true,
        ),
        (
            "independent_occurrences_wrong",
            "trait S{type Out<'a>:'a where Self:'a;}trait D{type Out<'a,'b>:'a where Self:'a,Self:'b;}struct W<T>(T);impl<T:S>D for W<T>{type Out<'a,'b>=(T::Out<'a>,T::Out<'b>)where Self:'a,Self:'b;}",
            false,
        ),
        (
            "borrow_escape",
            "trait S{type Out<'a>:'a where Self:'a;fn get(&self)->Self::Out<'_>;}struct W(u32);impl S for W{type Out<'a>=&'a u32 where Self:'a;fn get(&self)->Self::Out<'_>{&self.0}}fn bad()->&'static u32{let w=W(3);w.get()}",
            false,
        ),
        (
            "trait_owner_output_call",
            "trait Family<'a>{type Output:'a;}fn need<'a,T:'a>(_:&'a T){}fn use_output<'a,Y:Family<'a>>(x:&'a Y::Output){need(x)}",
            true,
        ),
        (
            "trait_owner_ranked_compare",
            "trait Family<'a>{type Output:'a;}fn get<'a,Y:Family<'a>>(_: &'a Y)->&'a Y::Output{panic!()}fn compare<Y:for<'a>Family<'a>>(a:&Y,b:&Y)->bool where for<'a><Y as Family<'a>>::Output:PartialEq{get(a)==get(b)}",
            true,
        ),
        (
            "trait_owner_wrong_static_call",
            "trait Family<'a>{type Output:'a;}fn need<T:'static>(_:&T){}fn bad<'a,Y:Family<'a>>(x:&'a Y::Output){need(x)}",
            false,
        ),
        (
            "trait_owner_ranked_partial_order",
            "trait Family<'a>{type Output:'a;}fn get<'a,Y:Family<'a>>(_: &'a Y)->&'a Y::Output{panic!()}fn compare<Y:for<'a>Family<'a>>(a:&Y,b:&Y)->Option<std::cmp::Ordering> where for<'a><Y as Family<'a>>::Output:PartialOrd{get(a).partial_cmp(get(b))}",
            true,
        ),
        (
            "gat_ranked_compare",
            "trait Family{type Output<'a>:'a where Self:'a;}fn get<'a,Y:Family>(_: &'a Y)->&'a Y::Output<'a>{panic!()}fn compare<Y:'static+Family>(a:&Y,b:&Y)->bool where for<'a>Y::Output<'a>:PartialEq{get(a)==get(b)}",
            true,
        ),
        (
            "trait_owner_wrong_occurrence",
            "trait Family<'a>{type Output:'a;}fn get<'a,Y:Family<'a>>(_: &'a Y)->&'a Y::Output{panic!()}fn compare<Y:for<'a>Family<'a>>(a:&Y,b:&Y)->bool where for<'a><Y as Family<'a>>::Output:PartialEq<<Y as Family<'static>>::Output>{get(a)==get(b)}",
            false,
        ),
        (
            "static_output_generic_call",
            "trait Family<'a>{type Output:'static;}fn keep<T:'static>(x:T)->T{x}fn forward<'a,Y:Family<'a>>(x:Y::Output)->Y::Output{keep(x)}",
            true,
        ),
        (
            "nonstatic_output_generic_call",
            "trait Family<'a>{type Output:'a;}fn keep<T:'static>(x:T)->T{x}fn forward<'a,Y:Family<'a>>(x:Y::Output)->Y::Output{keep(x)}",
            false,
        ),
        (
            "independent_output_generic_call",
            "struct Token<'a>(&'a mut &'a());trait Family<'a>{type Output:'a;}fn keep<'b,T:'b>(x:T,_:Token<'b>)->T{x}fn forward<'a,'b,Y:for<'x>Family<'x>>(x:<Y as Family<'a>>::Output,b:Token<'b>)-><Y as Family<'a>>::Output{keep(x,b)}",
            false,
        ),
        (
            "bounded_output_generic_call",
            "struct Token<'a>(&'a mut &'a());trait Family<'a>{type Output:'a;}fn keep<'b,T:'b>(x:T,_:Token<'b>)->T{x}fn forward<'a:'b,'b,Y:for<'x>Family<'x>>(x:<Y as Family<'a>>::Output,b:Token<'b>)-><Y as Family<'a>>::Output{keep(x,b)}",
            true,
        ),
        (
            "selected_zero_slot_output",
            "trait Family<'a>{type Output:'a;}struct Model;impl<'a>Family<'a>for Model{type Output=u32;}fn need<Y:for<'a>Family<'a>>()where for<'a><Y as Family<'a>>::Output:PartialEq{}fn use_it(){need::<Model>();}",
            true,
        ),
        (
            "selected_two_slot_output",
            "trait Family<'a>{type Output:'a;}struct Model;impl<'a>Family<'a>for Model{type Output=(&'a str,&'a str);}fn need<Y:for<'a>Family<'a>>()where for<'a><Y as Family<'a>>::Output:PartialEq{}fn use_it(){need::<Model>();}",
            true,
        ),
        (
            "selected_borrowed_owner_static",
            "trait Family<'a>{type Output:'a;}struct Model<'p>(&'p str);impl<'a,'p:'a>Family<'a>for Model<'p>{type Output=&'a str;}fn need<Y:for<'a>Family<'a>>()where for<'a><Y as Family<'a>>::Output:PartialEq{}fn use_it(){need::<Model<'static>>();}",
            true,
        ),
        (
            "selected_borrowed_owner_not_general",
            "trait Family<'a>{type Output:'a;}struct Model<'p>(&'p str);impl<'a,'p:'a>Family<'a>for Model<'p>{type Output=&'a str;}fn need<Y:for<'a>Family<'a>>()where for<'a><Y as Family<'a>>::Output:PartialEq{}fn bad<'p>(_:Model<'p>){need::<Model<'p>>();}",
            false,
        ),
    ] {
        if !case_filter::selected(name) {
            continue;
        }
        let source = format!("{source}fn main(){{}}");
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, &source).unwrap();
        let rustc = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("case.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            rustc.status.success(),
            accepted,
            "rustc {name}:{}",
            String::from_utf8_lossy(&rustc.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text(&path, &source);
        let mut errors = hir.diagnostics.clone();
        if errors.is_empty() {
            let typed = flux_mir::check_file(&hir);
            errors.extend(typed.diagnostics.clone());
            if errors.is_empty() {
                errors.extend(flux_mir::build_file(&hir, &typed).diagnostics);
            }
        }
        assert_eq!(errors.is_empty(), accepted, "flux {name}:{errors:?}");
    }
}

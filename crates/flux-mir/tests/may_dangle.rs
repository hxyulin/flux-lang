//! Trusted destructor promises never suppress owned-element Drop requirements.
#[test]
fn may_dangle_matches_rustc() {
    let root = std::env::temp_dir().join(format!("flux-may-dangle-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        ("vec_refs", "fn main(){let mut refs=Vec::new();let value=7;refs.push(&value);assert_eq!(*refs[0],7);}", true),
        ("nested_vec_refs", "fn main(){let mut refs=Vec::new();let value=7;refs.push(vec![vec![&value]]);assert_eq!(*refs[0][0][0],7);}", true),
        ("box_refs", "fn main(){let owner;let value=7;owner=Box::new(&value);assert_eq!(**owner,7);}", true),
        ("nested_box_vec_refs", "fn main(){let owner;let value=7;owner=Box::new(vec![Box::new(&value)]);assert_eq!(**owner[0],7);}", true),
        ("box_inspecting_drop", "struct Read<'a>(&'a u32);impl Drop for Read<'_>{fn drop(&mut self){assert_eq!(*self.0,7);}}fn main(){let owner;let value=7;owner=Box::new(Read(&value));}", false),
        ("unsafe_drop_without_marker", "struct S<T>(*const T);unsafe impl<T> Drop for S<T>{fn drop(&mut self){}}fn main(){}", false),
        ("vec_into_iter_refs", "fn main(){let iter;let value=7;iter=vec![&value].into_iter();assert_eq!(iter.as_slice()[0],&7);}", true),
        ("array_into_iter_refs", "fn main(){let iter;let value=7;iter=[&value].into_iter();assert_eq!(iter.as_slice()[0],&7);}", false),
        ("vec_inspecting_drop", "struct Read<'a>(&'a u32);impl Drop for Read<'_>{fn drop(&mut self){assert_eq!(*self.0,7);}}fn main(){let mut refs=Vec::new();let value=7;refs.push(Read(&value));}", false),
        ("nested_inspecting_drop", "struct Read<'a>(&'a u32);impl Drop for Read<'_>{fn drop(&mut self){assert_eq!(*self.0,7);}}fn main(){let mut refs=Vec::new();let value=7;refs.push(vec![Read(&value)]);}", false),
        ("array_inspecting_drop", "struct Read<'a>(&'a u32);impl Drop for Read<'_>{fn drop(&mut self){assert_eq!(*self.0,7);}}fn main(){let iter;let value=7;iter=[Read(&value)].into_iter();}", false),
        ("projected_refs", "trait Out{type Item;}struct W<'a>(&'a u32);impl<'a> Out for W<'a>{type Item=&'a u32;}struct C<T:Out>{item:T::Item}fn pack<'a>(r:&'a u32)->C<W<'a>>{C{item:r}}fn main(){let mut refs=Vec::new();let value=7;refs.push(vec![pack(&value)]);assert_eq!(*refs[0][0].item,7);}", true),
        ("projected_drop", "struct Read<'a>(&'a u32);impl Drop for Read<'_>{fn drop(&mut self){assert_eq!(*self.0,7);}}trait Out{type Item;}struct W<'a>(&'a u32);impl<'a> Out for W<'a>{type Item=Read<'a>;}struct C<T:Out>{item:T::Item}fn pack<'a>(r:&'a u32)->C<W<'a>>{C{item:Read(r)}}fn main(){let mut refs=Vec::new();let value=7;refs.push(vec![pack(&value)]);}", false),
        ("later_use", "fn main(){let mut refs=Vec::new();let mut value=7;refs.push(&value);value=9;assert_eq!(*refs[0],7);}", false),
        ("user_type_marker", "struct S<T>(*const T);unsafe impl<#[may_dangle] T> Drop for S<T>{fn drop(&mut self){}}fn main(){}", false),
        ("user_lifetime_marker", "struct S<'a>(&'a u32);unsafe impl<#[may_dangle] 'a> Drop for S<'a>{fn drop(&mut self){}}fn main(){}", false),
        ("user_feature_escape", "#![feature(dropck_eyepatch)]\nstruct S<T>(*const T);unsafe impl<#[may_dangle] T> Drop for S<T>{fn drop(&mut self){}}fn main(){}", false),
    ] {
        let path=root.join(format!("{name}.rs")); std::fs::write(&path,source).unwrap();
        let oracle=std::process::Command::new("rustc").args(["--edition=2024","--emit=metadata"]).arg(&path).arg("-o").arg(root.join("oracle.rmeta")).output().unwrap();
        assert_eq!(oracle.status.success(),accepted,"{name}: {}",String::from_utf8_lossy(&oracle.stderr));
        let (_,_,hir)=flux_hir::lower_text(&path,source);
        let mut errors=hir.diagnostics.clone();
        if errors.is_empty() { let typed=flux_mir::check_file(&hir); errors.extend(typed.diagnostics.clone()); if errors.is_empty(){errors.extend(flux_mir::build_file(&hir,&typed).diagnostics);} }
        assert_eq!(errors.is_empty(),accepted,"{name}: {errors:?}");
    }
}

/// Model the trusted library boundary directly. The public frontend rejection
/// cases above remain stable Rust; only this oracle uses rustc's internal feature.
#[test]
fn internal_lifetime_and_reordered_parameter_promises() {
    let root = std::env::temp_dir().join(format!("flux-internal-drop-promises-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, declaration, body, accepted) in [
        ("lifetime", "struct Holder<'a,'b>{a:*const &'a u32,b:*const &'b u32}impl<'b,'a> Drop for Holder<'a,'b>{fn drop(&mut self){}}fn make<'a,'b>(a:&'a u32,b:&'b u32)->Holder<'a,'b>{Holder{a:&a,b:&b}}", "let long=1;let owner;let short=2;owner=make(&long,&short);", true),
        ("unmarked_lifetime", "struct Holder<'a,'b>{a:*const &'a u32,b:*const &'b u32}impl<'b,'a> Drop for Holder<'a,'b>{fn drop(&mut self){}}fn make<'a,'b>(a:&'a u32,b:&'b u32)->Holder<'a,'b>{Holder{a:&a,b:&b}}", "let long=1;let owner;let short=2;owner=make(&short,&long);", false),
        ("type", "struct Holder<T,U>{a:*const T,b:*const U}impl<U,T> Drop for Holder<T,U>{fn drop(&mut self){}}fn make<T,U>(a:T,b:U)->Holder<T,U>{Holder{a:&a,b:&b}}", "let long=1;let owner;let short=2;owner=make(&long,&short);", true),
        ("unmarked_type", "struct Holder<T,U>{a:*const T,b:*const U}impl<U,T> Drop for Holder<T,U>{fn drop(&mut self){}}fn make<T,U>(a:T,b:U)->Holder<T,U>{Holder{a:&a,b:&b}}", "let long=1;let owner;let short=2;owner=make(&short,&long);", false),
        ("owned_lifetime", "struct Read<'a>(&'a u32);impl Drop for Read<'_>{fn drop(&mut self){assert_eq!(*self.0,7);}}struct Holder<'b>{owner:std::marker::PhantomData<Read<'b>>}impl<'b> Drop for Holder<'b>{fn drop(&mut self){}}fn make<'b>(_:&'b u32)->Holder<'b>{Holder{owner:std::marker::PhantomData}}", "let owner;let short=7;owner=make(&short);", false),
    ] {
        let source = format!("{declaration}fn main(){{{body}}}");
        let (_, _, mut hir) = flux_hir::lower_text(&root.join(format!("{name}.rs")), &source);
        assert!(hir.diagnostics.is_empty(), "{name}: {:?}", hir.diagnostics);
        let id = hir.items.impls.iter().find_map(|(id, imp)| {
            imp.self_ty.to_string().starts_with("Holder").then_some(id)
        }).unwrap();
        let imp = &mut hir.items.impls[id];
        imp.is_unsafe = true;
        for param in &mut imp.generics.lifetimes { param.may_dangle = param.name.as_str() == "'b"; }
        for param in &mut imp.generics.params { param.may_dangle = param.name.as_str() == "U"; }
        let typed = flux_mir::check_file(&hir);
        let mut errors = typed.diagnostics.clone();
        if errors.is_empty() { errors.extend(flux_mir::build_file(&hir, &typed).diagnostics); }
        assert_eq!(errors.is_empty(), accepted, "{name}: {errors:?}");
        let marked = source.replace("impl<'b", "unsafe impl<#[may_dangle] 'b").replace("impl<U,T>", "unsafe impl<#[may_dangle] U,T>");
        let path = root.join(format!("{name}-oracle.rs"));
        std::fs::write(&path, format!("#![feature(dropck_eyepatch)]\n{marked}")).unwrap();
        let oracle = std::process::Command::new("rustc").env("RUSTC_BOOTSTRAP", "1")
            .args(["--edition=2024", "--crate-name", "internal_drop_oracle", "--emit=metadata"]).arg(path)
            .arg("-o").arg(root.join(format!("{name}.rmeta"))).output().unwrap();
        assert_eq!(oracle.status.success(), accepted, "{name}: {}", String::from_utf8_lossy(&oracle.stderr));
    }
}

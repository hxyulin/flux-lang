//! Type and const family equality arguments remain distinct from trait inputs.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod cases;
#[test]
fn gat_equality_bindings_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-gat-equalities-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let family = "trait G{type A<T>;}struct Id;impl G for Id{type A<T>=T;}";
    for(name,source,accepted) in [
 ("type_equality",format!("{family}fn f<T,S:G<A<T>=T>>(x:S::A<T>)->T{{x}}fn main(){{assert_eq!(f::<u32,Id>(7),7);}}"),true),
 ("captured_reference",format!("{family}fn f<T,S:G<A<T>=T>>(x:S::A<T>)->T{{x}}fn main(){{let n=7;assert_eq!(*f::<&u32,Id>(&n),7);}}"),true),
 ("captured_escape",format!("{family}fn f<T,S:G<A<T>=T>>(x:S::A<T>)->T{{x}}fn main(){{let got;{{let n=7;got=f::<&u32,Id>(&n);}}println!(\"{{got}}\");}}"),false),
 ("borrowed_field",format!("{family}struct Wrap<T,S:G>(S::A<T>);impl<T,S:G<A<T>=T>>Wrap<T,S>{{fn get(&self)->&T{{&self.0}}}}fn main(){{let n=7;let x=Wrap::<&u32,Id>(&n);assert_eq!(**x.get(),7);}}"),true),
 ("const_argument","trait G{type A<const N:usize>;}struct S;impl G for S{type A<const N:usize>=[u32;N];}fn f<S:G<A<3>=[u32;3]>>(x:S::A<3>)->[u32;3]{x}fn main(){assert_eq!(f::<S>([1,2,3])[2],3);}".into(),true),
 ("type_const_arguments","trait G{type A<T,const N:usize>;}struct S;impl G for S{type A<T,const N:usize>=[T;N];}fn f<T,S:G<A<T,N>=[T;N]>,const N:usize>(x:S::A<T,N>)->[T;N]{x}fn main(){assert_eq!(f::<u32,S,3>([1,2,3])[1],2);}".into(),true),
 ("supertrait_binding","trait G{type A<T>;}trait H:G{}struct S;impl G for S{type A<T>=T;}impl H for S{}fn f<T,S:H<A<T>=T>>(x:S::A<T>)->T{x}fn main(){assert_eq!(f::<u32,S>(7),7);}".into(),true),
 ("owner_argument","trait G<P>{type A<T>;}struct S;impl<P>G<P> for S{type A<T>=(P,T);}fn f<T,S:G<u32,A<T>=(u32,T)>>(x:S::A<T>)->(u32,T){x}fn main(){assert_eq!(f::<u64,S>((7,9)).1,9);}".into(),true),
 ("required_static_missing","trait G{type A<T> where T:'static;}fn f<T,S:G<A<T>=u32>>(){}fn main(){}".into(),false),
 ("required_static_valid","trait G{type A<T> where T:'static;}struct S;impl G for S{type A<T>=u32 where T:'static;}fn f<T:'static,S:G<A<T>=u32>>(){}fn main(){f::<u32,S>();}".into(),true),
 ("required_static_capture","trait G{type A<T> where T:'static;}struct S;impl G for S{type A<T>=u32 where T:'static;}fn f<T:'static,S:G<A<T>=u32>>(_:T){}fn main(){let n=7;f::<&u32,S>(&n);}".into(),false),
 ("sized_family_argument",format!("{family}fn f<S:G<A<str>=u32>>(){{}}fn main(){{}}"),false),
 ("unsized_family_argument","trait G{type A<T:?Sized>;}struct S;impl G for S{type A<T:?Sized>=u32;}fn f<S:G<A<str>=u32>>(x:S::A<str>)->u32{x}fn main(){assert_eq!(f::<S>(7),7);}".into(),true),
 ("required_clone_missing","trait G{type A<T:Clone>;}fn f<T,S:G<A<T>=u32>>(){}fn main(){}".into(),false),
 ("required_clone_valid","trait G{type A<T:Clone>;}struct S;impl G for S{type A<T:Clone>=T;}fn f<T:Clone,S:G<A<T>=T>>(x:S::A<T>)->T{x}fn main(){assert_eq!(f::<u32,S>(7),7);}".into(),true),
 ("supertrait_clone_missing","trait G{type A<T:Clone>;}trait H<T>:G<A<T>=u32>{}fn main(){}".into(),false),
 ("struct_static_missing","trait G{type A<T>where T:'static;}struct W<T,S:G<A<T>=u32>>(T,S);fn main(){}".into(),false),
 ("impl_static_missing","trait G{type A<T>where T:'static;}struct W<T,S>(T,S);impl<T,S:G<A<T>=u32>>W<T,S>{}fn main(){}".into(),false),
 ("swapped_capture_escape",r#"trait G{type A<T,U>;}struct S;impl G for S{type A<T,U>=(U,T);}fn f<T,U,S:G<A<T,U>=(T,U)>>(x:S::A<T,U>)->T{x.0}fn main(){let a=7;let out;{let b=9;out=f::<&u32,&u32,S>((&b,&a));}println!("{out}");}"#.into(),false),
 ("captured_mutable_invariance",format!("{family}fn set<T,S:G<A<T>=T>>(x:&mut S::A<T>,y:T){{*x=y;}}fn main(){{let a=7;let mut x=&a;{{let b=9;set::<&u32,Id>(&mut x,&b);}}assert_eq!(*x,7);}}"),false),
 ("swapped_producer_escape",r#"trait G{type A<T,U>;}struct S;impl G for S{type A<T,U>=(U,T);}fn make<'a,'b>(a:&'a u32,b:&'b u32)-><S as G>::A<&'a u32,&'b u32>{(b,a)}fn f<T,U,S:G<A<T,U>=(T,U)>>(x:S::A<T,U>)->T{x.0}fn main(){let a=7;let out;{let b=9;out=f::<&u32,&u32,S>(make(&a,&b));}println!("{out}");}"#.into(),false),
 ("supertrait_static_missing","trait G{type A<T>where T:'static;}trait H<T>:G<A<T>=u32>{}fn main(){}".into(),false),
 ("supertrait_equality","trait G{type A<T>;}trait H<T>:G<A<T>=T>{}struct S;impl G for S{type A<T>=T;}impl<T>H<T>for S{}fn main(){}".into(),true),
 ("bool_const_family","trait G{type A<const B:bool>;}struct S;impl G for S{type A<const B:bool>=u32;}fn f<S:G<A<true>=u32>>(x:S::A<true>)->u32{x}fn main(){assert_eq!(f::<S>(7),7);}".into(),true),
 ("wrong_family",format!("{family}fn f<T,S:G<A<u32>=T>>(x:S::A<u64>)->T{{x}}fn main(){{}}"),false),
 ("wrong_rhs",format!("{family}fn f<S:G<A<u32>=u64>>(_:S){{}}fn main(){{f(Id);}}"),false),
 ("missing_arguments",format!("{family}fn f<S:G<A=u32>>(_:S){{}}fn main(){{}}"),false),
 ("too_many_arguments",format!("{family}fn f<S:G<A<u32,u64>=u32>>(_:S){{}}fn main(){{}}"),false),
 ("wrong_const_kind","trait G{type A<const N:usize>;}fn f<S:G<A<false>=u32>>(_:S){}fn main(){}".into(),false),
 ("stricter_impl","trait G{type A<T>;}struct S;impl G for S{type A<T:Copy>=T;}fn main(){}".into(),false),
 ]{
        if !cases::selected(name) {
            continue;
        }
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, &source).unwrap();
        let oracle = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path).arg("-o").arg(root.join(format!("{name}.rmeta")))
            .output().unwrap();
        assert_eq!(oracle.status.success(), accepted, "{name}: {}", String::from_utf8_lossy(&oracle.stderr));
        let borrow_rejection = matches!(name, "captured_escape" | "captured_mutable_invariance" | "swapped_capture_escape" | "swapped_producer_escape" | "required_static_capture");
        let (db, _, hir) = flux_hir::lower_text(&path, &source);
        let mut errors = db.syntax_errors();
        errors.extend(hir.diagnostics.clone());
        if errors.is_empty() {
            let typed = flux_mir::check_file(&hir);
            errors.extend(typed.diagnostics.clone());
            if borrow_rejection {
                assert!(errors.is_empty(), "{name}: expected clean types, got {errors:?}");
            }
            if errors.is_empty() {
                errors.extend(flux_mir::build_file(&hir, &typed).diagnostics);
            }
        }
        assert_eq!(errors.is_empty(), accepted, "{name}: {errors:?}");
        if borrow_rejection {
            assert!(errors.iter().all(|d| !d.message.contains("not supported yet")), "{name}: expected actual borrow rejection, got {errors:?}");
        }
 }
}

#[test]
fn lifetime_bearing_equalities_fail_explicitly() {
    let root = std::env::temp_dir().join(format!(
        "flux-gat-equality-boundaries-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source) in [
        (
            "family_lifetime",
            "trait G{type A<'a>;}fn f<S:G<A<'static>=u32>>(){}fn main(){}",
        ),
        (
            "written_reference",
            "trait G{type A<T>;}fn f<'a,S:G<A<&'a u32>=u32>>(){}fn main(){}",
        ),
        (
            "owner_lifetime",
            "trait G<'a>{type A<T>;}fn f<'a,S:G<'a,A<u32>=u32>>(){}fn main(){}",
        ),
    ] {
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let rustc = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join(format!("{name}.rmeta")))
            .output()
            .unwrap();
        assert!(
            rustc.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&rustc.stderr)
        );
        let (db, _, hir) = flux_hir::lower_text(&path, source);
        assert!(db.syntax_errors().is_empty());
        assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        assert!(
            typed.diagnostics.iter().any(|d| d.message
                == "lifetime-bearing associated family equality arguments are not supported yet"),
            "{name}: {:?}",
            typed.diagnostics
        );
    }
}

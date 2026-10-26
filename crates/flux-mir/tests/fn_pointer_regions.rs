//! Function-pointer binders retain input/output lifetime links across storage.
#[test]
fn function_pointer_regions_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-fn-pointer-regions-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "boxed_object_default",
            "trait Tr{}fn read(_: &Box<dyn Tr>){}fn main(){let _:fn(&Box<dyn Tr>)=read;}",
            true,
        ),
        (
            "boxed_object_escape",
            "trait Tr{}fn call<'a>(p:fn(&'a Box<dyn Tr>),x:&'a Box<dyn Tr+'a>){p(x);}fn main(){}",
            false,
        ),
        (
            "implied_output_conversion",
            "fn convert(p:for<'a,'b>fn(&'a &'b u32)->&'b u32)->for<'a,'b>fn(&'a &'b u32)->&'a u32{p}fn main(){}",
            false,
        ),
        (
            "implied_item_conversion",
            "fn inner<'a,'b>(x:&'a &'b u32)->&'b u32{*x}fn main(){let _:for<'a,'b>fn(&'a &'b u32)->&'a u32=inner;}",
            false,
        ),
        (
            "getter",
            "struct Pool{values:Vec<Vec<u32>>}fn get(p:&mut Pool)->&mut Vec<Vec<u32>>{&mut p.values}fn take(f:fn(&mut Pool)->&mut Vec<Vec<u32>>)->Vec<u32>{let mut p=Pool{values:vec![vec![7]]};f(&mut p).pop().unwrap()}fn main(){assert_eq!(take(get)[0],7);}",
            true,
        ),
        (
            "identity",
            "fn id(x:&u32)->&u32{x}fn main(){let p:fn(&u32)->&u32=id;let x=7;assert_eq!(*p(&x),7);}",
            true,
        ),
        (
            "named_two",
            "fn left<'a,'b>(x:&'a u32,_:&'b u32)->&'a u32{x}fn main(){let p:for<'a,'b>fn(&'a u32,&'b u32)->&'a u32=left;let x=7;let y=11;assert_eq!(*p(&x,&y),7);}",
            true,
        ),
        (
            "mutable_nested",
            "fn id<'a,'b>(x:&'b mut &'a u32)->&'b mut &'a u32{x}fn main(){let p:for<'a,'b>fn(&'b mut &'a u32)->&'b mut &'a u32=id;let x=7;let mut r=&x;assert_eq!(**p(&mut r),7);}",
            true,
        ),
        (
            "static_output",
            "static X:u32=7;fn get()->&'static u32{&X}fn main(){let p:fn()->&'static u32=get;assert_eq!(*p(),7);}",
            true,
        ),
        (
            "free_lifetime",
            "fn id(x:&u32)->&u32{x}fn call<'a>(p:fn(&'a u32)->&'a u32,x:&'a u32)->&'a u32{p(x)}fn main(){let x=7;assert_eq!(*call(id,&x),7);}",
            true,
        ),
        (
            "shorten_shared",
            "fn shorten<'a:'b,'b>(p:fn()->&'a u32)->fn()->&'b u32{p}fn main(){}",
            true,
        ),
        (
            "alpha_rename",
            "fn change(p:for<'a,'b>fn(&'a u32,&'b u32)->&'a u32)->for<'y,'x>fn(&'x u32,&'y u32)->&'x u32{p}fn main(){}",
            true,
        ),
        (
            "same_input",
            "fn same<'a>(_:&'a u32,_:&'a u32){}fn main(){let p:fn(&u32,&u32)=same;let x=7;let y=11;p(&x,&y);}",
            true,
        ),
        (
            "closure_identity",
            "fn main(){let p:fn(&u32)->&u32=|x|x;let x=7;assert_eq!(*p(&x),7);}",
            true,
        ),
        (
            "safe_to_unsafe",
            "fn id(x:&u32)->&u32{x}fn main(){let p:unsafe fn(&u32)->&u32=id;let x=7;assert_eq!(*unsafe{p(&x)},7);}",
            true,
        ),
        (
            "c_scalar",
            "extern \"C\" fn id(x:u32)->u32{x+1}fn main(){let p:extern \"C\" fn(u32)->u32=id;assert_eq!(p(7),8);}",
            true,
        ),
        (
            "erase_free",
            "fn erase<'a>(p:fn(&'a u32)->&'a u32)->fn(&u32)->&u32{p}fn main(){}",
            false,
        ),
        (
            "static_from_free",
            "fn extend<'a>(p:fn()->&'a u32)->fn()->&'static u32{p}fn main(){}",
            false,
        ),
        (
            "mutable_invariance",
            "fn shorten<'a:'b,'b>(p:for<'x>fn(&'x mut &'a u32)->&'x mut &'a u32)->for<'x>fn(&'x mut &'b u32)->&'x mut &'b u32{p}fn main(){}",
            false,
        ),
        (
            "static_clone",
            "fn fixed(x:&'static u32)->&'static u32{x}fn main(){let f=fixed.clone();let _:fn(&u32)->&u32=f;}",
            false,
        ),
        (
            "unsafe_to_safe",
            "unsafe fn id(x:&u32)->&u32{x}fn main(){let _:fn(&u32)->&u32=id;}",
            false,
        ),
        (
            "c_to_rust",
            "extern \"C\" fn id(x:u32)->u32{x}fn main(){let _:fn(u32)->u32=id;}",
            false,
        ),
        (
            "cast_identity",
            "fn id(x:&u32)->&u32{x}fn main(){let p=id as fn(&u32)->&u32;let x=7;assert_eq!(*p(&x),7);}",
            true,
        ),
        (
            "cast_static_input",
            "fn fixed(x:&'static u32)->&'static u32{x}fn main(){let _=fixed as fn(&u32)->&u32;}",
            false,
        ),
        (
            "free_static_input",
            "fn fixed(x:&'static u32)->&'static u32{x}fn call<'a>(p:fn(&'a u32)->&'a u32,x:&'a u32)->&'a u32{p(x)}fn main(){let x=7;let _=call(fixed,&x);}",
            false,
        ),
        (
            "input_implied",
            "fn inner<'a,'b>(x:&'b &'a u32)->&'a u32{*x}fn main(){let p:for<'a,'b>fn(&'b &'a u32)->&'a u32=inner;let x=7;let r=&x;assert_eq!(*p(&r),7);}",
            true,
        ),
        (
            "option_output",
            "fn some(x:&u32)->Option<&u32>{Some(x)}fn main(){let p:fn(&u32)->Option<&u32>=some;let x=7;assert_eq!(*p(&x).unwrap(),7);}",
            true,
        ),
        (
            "generic_callback",
            "fn id(x:&u32)->&u32{x}fn invoke<F:for<'a>Fn(&'a u32)->&'a u32>(f:F)->u32{let x=7;*f(&x)}fn main(){let p:fn(&u32)->&u32=id;assert_eq!(invoke(p),7);}",
            true,
        ),
        (
            "dyn_callback",
            "fn read(x:&u32)->u32{*x}fn main(){let p:fn(&u32)->u32=read;let f:&dyn Fn(&u32)->u32=&p;let x=7;assert_eq!(f(&x),7);}",
            true,
        ),
        (
            "generic_static_input",
            "fn fixed(_: &'static u32){}fn invoke<F:Fn(&u32)>(f:F){let x=7;f(&x)}fn main(){let p:fn(&'static u32)=fixed;invoke(p);}",
            false,
        ),
        (
            "dyn_static_input",
            "fn fixed(_: &'static u32){}fn main(){let p:fn(&'static u32)=fixed;let _f:&dyn Fn(&u32)=&p;}",
            false,
        ),
        (
            "generic_wrong_output",
            "fn id(x:&u32)->&u32{x}fn invoke<F:for<'a>Fn(&'a u32)->&'static u32>(f:F){}fn main(){let p:fn(&u32)->&u32=id;invoke(p);}",
            false,
        ),
        (
            "escaping",
            "fn id(x:&u32)->&u32{x}fn main(){let p:fn(&u32)->&u32=id;let escaped;{let x=7;escaped=p(&x);}assert_eq!(*escaped,7);}",
            false,
        ),
        (
            "wrong_output_link",
            "fn left<'a,'b>(x:&'a u32,_:&'b u32)->&'a u32{x}fn main(){let _:for<'a,'b>fn(&'a u32,&'b u32)->&'b u32=left;}",
            false,
        ),
        (
            "static_input",
            "fn fixed(x:&'static u32)->&'static u32{x}fn main(){let _:fn(&u32)->&u32=fixed;}",
            false,
        ),
        (
            "shortening_mut",
            "fn id<'a,'b>(x:&'b mut &'a u32)->&'b mut &'a u32{x}fn main(){let _:for<'a,'b>fn(&'b mut &'a u32)->&'b mut &'static u32=id;}",
            false,
        ),
        (
            "borrowed_nested_generalize",
            "fn change<'a,'p>(p:&'p fn(&'a u32)->&'a u32)->&'p fn(&u32)->&u32{p}fn main(){}",
            false,
        ),
        (
            "borrowed_nested_static",
            "fn change(p:&fn(&'static u32)->&'static u32)->&fn(&u32)->&u32{p}fn main(){}",
            false,
        ),
        (
            "borrowed_nested_specialize",
            "fn change<'a>(p:&'a fn(&u32)->&u32)->&'a fn(&'static u32)->&'static u32{p}fn main(){}",
            true,
        ),
        (
            "tuple_nested_static",
            "fn change(p:(fn(&'static u32)->&'static u32,u32))->(fn(&u32)->&u32,u32){p}fn main(){}",
            false,
        ),
        (
            "tuple_nested_specialize",
            "fn change(p:(fn(&u32)->&u32,u32))->(fn(&'static u32)->&'static u32,u32){p}fn main(){}",
            true,
        ),
        (
            "mutable_nested_pointer",
            "fn change<'a>(p:&'a mut fn(&u32)->&u32)->&'a mut fn(&'static u32)->&'static u32{p}fn main(){}",
            false,
        ),
        (
            "direct_nested_argument",
            "fn take(_: &(fn(&u32)->&u32)){}fn f(x:&'static u32)->&'static u32{x}fn main(){let p:fn(&'static u32)->&'static u32=f;take(&p);}",
            false,
        ),
        (
            "trait_bound_identity",
            "trait Marker{}impl Marker for fn(&'static u32){}fn need<T:Marker>(_:T){}fn f(_: &u32){}fn main(){let p:fn(&u32)=f;need(p);}",
            false,
        ),
        (
            "trait_bound_position",
            "trait Marker{}impl Marker for fn((&'static u32,&u32)){}fn need<T:Marker>(_:T){}fn main(){let p:fn((&u32,&'static u32))=|_|{};need(p);}",
            false,
        ),
        (
            "trait_pointer_positive",
            "trait Marker{}impl Marker for fn(&u32){}fn need<T:Marker>(_:T){}fn f(_: &u32){}fn main(){let p:fn(&u32)=f;need(p);}",
            true,
        ),
        (
            "trait_method_pointer_input_contravariant",
            "trait Tr{fn take(p:fn(&u32));}struct S;impl Tr for S{fn take(p:fn(&'static u32)){}}fn main(){}",
            true,
        ),
        (
            "trait_method_pointer_output",
            "trait Tr{fn get()->fn(&u32);}struct S;impl Tr for S{fn get()->fn(&'static u32){|_|{}}}fn main(){}",
            false,
        ),
        (
            "pointer_nested_argument",
            "fn accepts(_:fn(&u32)){}fn fixed(_: &'static u32){}fn main(){let p:fn(fn(&u32))=accepts;let f:fn(&'static u32)=fixed;p(f);}",
            false,
        ),
        (
            "item_nested_argument",
            "fn accepts(_:fn(&u32)){}fn main(){let _:fn(fn(&'static u32))=accepts;}",
            false,
        ),
        (
            "item_nested_argument_positive",
            "fn accepts(_:fn(&'static u32)){}fn main(){let _:fn(fn(&u32))=accepts;}",
            true,
        ),
        (
            "shadowed_binder",
            "fn f<'a>(_:for<'a>fn(&'a u32)){}fn main(){}",
            false,
        ),
        (
            "dyn_input",
            "trait Tr{}impl Tr for u32{}fn read(_: &dyn Tr){}fn main(){let p:fn(&dyn Tr)=read;let x=7u32;p(&x);}",
            true,
        ),
        (
            "unused_binder_identity",
            "trait Marker{}impl Marker for fn(&u32){}fn need<T:Marker>(){}fn main(){need::<for<'a,'unused>fn(&'a u32)>();}",
            true,
        ),
        (
            "permuted_binder_identity",
            "trait Marker{}impl Marker for for<'a,'b>fn(&'a u32,&'b u32)->&'a u32{}fn need<T:Marker>(){}fn main(){need::<for<'y,'x>fn(&'x u32,&'y u32)->&'x u32>();}",
            true,
        ),
        (
            "permuted_binder_wrong_output",
            "trait Marker{}impl Marker for for<'a,'b>fn(&'a u32,&'b u32)->&'a u32{}fn need<T:Marker>(){}fn main(){need::<for<'y,'x>fn(&'x u32,&'y u32)->&'y u32>();}",
            false,
        ),
        (
            "trait_free_static",
            "trait Marker{}impl Marker for fn(&'static u32){}fn need<T:Marker>(_:T){}fn check<'a>(p:fn(&'a u32)){need(p);}fn main(){}",
            true,
        ),
        (
            "trait_free_static_explicit",
            "trait Marker{}impl Marker for fn(&'static u32){}fn need<T:Marker>(){}fn check<'a>(){need::<fn(&'a u32)>();}fn main(){}",
            false,
        ),
        (
            "trait_output_static",
            "trait Marker{}impl Marker for fn()->&'static u32{}fn need<T:Marker>(){}fn check<'a>(){need::<fn()->&'a u32>();}fn main(){}",
            false,
        ),
        (
            "trait_free_positive",
            "trait Marker{}impl<'a> Marker for fn(&'a u32){}fn need<T:Marker>(_:T){}fn check<'a>(p:fn(&'a u32)){need(p);}fn main(){}",
            true,
        ),
        (
            "trait_free_to_static",
            "trait Marker{}impl<'a> Marker for fn(&'a u32){}fn need<T:Marker>(){}fn main(){need::<fn(&'static u32)>();}",
            true,
        ),
        (
            "bound_signature_static",
            "fn need<T:'static>(){}fn main(){need::<for<'a>fn(&'a u32)>();}",
            true,
        ),
        (
            "free_input_not_static",
            "fn need<T:'static>(){}fn check<'a>(){need::<fn(&'a u32)>();}fn main(){}",
            false,
        ),
        (
            "free_output_not_static",
            "fn need<T:'static>(){}fn check<'a>(){need::<fn()->&'a u32>();}fn main(){}",
            false,
        ),
        (
            "generic_input_not_static",
            "fn need<T:'static>(){}fn check<T>(){need::<fn(T)>();}fn main(){}",
            false,
        ),
        (
            "signature_send_sync",
            "use std::rc::Rc;fn need<T:Send+Sync>(){}fn main(){need::<fn(Rc<u32>)->Rc<u32>>();}",
            true,
        ),
        (
            "generic_pointer_trait",
            "trait Marker{}impl<T> Marker for fn(T){}fn need<T:Marker>(){}fn check<'a>(){need::<fn(&'a u32)>();}fn main(){}",
            true,
        ),
        (
            "generic_pointer_trait_static",
            "trait Marker{}impl<T:'static> Marker for fn(T){}fn need<T:Marker>(){}fn check<'a>(){need::<fn(&'a u32)>();}fn main(){}",
            false,
        ),
        (
            "generic_pointer_trait_bound",
            "trait Marker{}impl<T> Marker for fn(T){}fn need<T:Marker>(){}fn main(){need::<fn(&u32)>();}",
            false,
        ),
        (
            "bound_input_storage",
            "fn bind<'a>(_: &'a u32)->fn(&'a u32){|_|{}}fn main(){let f;{let x=7;f=bind(&x);}f(&11);}",
            true,
        ),
        (
            "closure_nested_input",
            "fn main(){let _:fn(fn(&'static u32))=|_:fn(&u32)|{};}",
            false,
        ),
        (
            "closure_nested_input_annotation",
            "fn main(){let _:fn(fn(&u32))=|_:fn(&'static u32)|{};}",
            false,
        ),
        (
            "pointer_callback_nested",
            "fn needs<F:Fn(fn(&'static u32))>(_:F){}fn take(_:fn(&u32)){}fn main(){let p:fn(fn(&u32))=take;needs(p);}",
            false,
        ),
        (
            "pointer_callback_nested_identity",
            "fn needs<F:Fn(fn(&u32))>(_:F){}fn take(_:fn(&'static u32)){}fn main(){let p:fn(fn(&'static u32))=take;needs(p);}",
            false,
        ),
        (
            "dyn_callback_nested",
            "fn take(_:fn(&u32)){}fn main(){let p:fn(fn(&u32))=take;let _f:&dyn Fn(fn(&'static u32))=&p;}",
            false,
        ),
        (
            "closure_callback_nested",
            "fn needs<F:Fn(fn(&'static u32))>(_:F){}fn main(){needs(|_:fn(&u32)|{});}",
            false,
        ),
        (
            "closure_callback_output",
            "fn needs<F:Fn()->fn(&u32)>(_:F){}fn main(){needs(||->fn(&'static u32){|_|{}});}",
            false,
        ),
        (
            "closure_callback_output_identity",
            "fn needs<F:Fn()->fn(&'static u32)>(_:F){}fn main(){needs(||->fn(&u32){|_|{}});}",
            false,
        ),
        (
            "item_callback_output_identity",
            "fn needs<F:Fn()->fn(&'static u32)>(_:F){}fn give()->fn(&u32){|_|{}}fn main(){needs(give);}",
            false,
        ),
        (
            "output_unconstrained",
            "fn main(){let _:for<'a>fn()->&'a u32;}",
            false,
        ),
    ] {
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let oracle = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join(format!("{name}.rmeta")))
            .output()
            .unwrap();
        assert_eq!(
            oracle.status.success(),
            accepted,
            "{name}: {}",
            String::from_utf8_lossy(&oracle.stderr)
        );
        let (db, _, hir) = flux_hir::lower_text(&path, source);
        let mut errors = db.syntax_errors();
        errors.extend(hir.diagnostics.clone());
        if errors.is_empty() {
            let typed = flux_mir::check_file(&hir);
            errors.extend(typed.diagnostics.clone());
            if errors.is_empty() {
                errors.extend(flux_mir::build_file(&hir, &typed).diagnostics);
            }
        }
        assert_eq!(errors.is_empty(), accepted, "{name}: {errors:?}");
    }
}

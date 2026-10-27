//! Associated families preserve declaration, impl-header, and occurrence lifetimes.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;

#[test]
fn associated_family_regions_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-gat-regions-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "concrete_lending",
            "trait L{type Ref<'a> where Self:'a;fn get<'a>(&'a self)->Self::Ref<'a>;}struct H(u32);impl L for H{type Ref<'a>=&'a u32;fn get<'a>(&'a self)->Self::Ref<'a>{&self.0}}fn main(){let h=H(7);assert_eq!(*h.get(),7);}",
            true,
        ),
        (
            "generic_lending",
            "trait L{type Ref<'a> where Self:'a;fn get<'a>(&'a self)->Self::Ref<'a>;}fn get<'a,T:L>(x:&'a T)->T::Ref<'a>{x.get()}fn main(){}",
            true,
        ),
        (
            "generic_escape",
            "trait L{type Ref<'a> where Self:'a;fn get<'a>(&'a self)->Self::Ref<'a>;}fn bad<T:L>(x:&T)->T::Ref<'static>{x.get()}fn main(){}",
            false,
        ),
        (
            "missing_required_self",
            "trait L{type Ref<'a>;fn get<'a>(&'a self)->Self::Ref<'a>;}fn main(){}",
            false,
        ),
        (
            "stricter_impl",
            "trait L{type Ref<'a> where Self:'a;}struct H<T>(T);impl<T>L for H<T>{type Ref<'a>=&'a T where T:'static;}fn main(){}",
            false,
        ),
        (
            "alpha_rename",
            "trait L{type Ref<'a> where Self:'a;fn get<'a>(&'a self)->Self::Ref<'a>;}struct H(u32);impl L for H{type Ref<'b>=&'b u32;fn get<'c>(&'c self)->Self::Ref<'c>{&self.0}}fn main(){assert_eq!(*H(7).get(),7);}",
            true,
        ),
        (
            "wrong_arity",
            "trait L{type Ref<'a> where Self:'a;}fn f<'a,'b,T:L>(){let _:Option<T::Ref<'a,'b>>=None;}fn main(){}",
            false,
        ),
        (
            "owner_file_independent",
            "trait R<'data>{type Ref<'file> where Self:'file,'data:'file;fn get<'file>(&'file self)->Self::Ref<'file>;}struct H<'data>(&'data u32);impl<'data>R<'data> for H<'data>{type Ref<'file>=&'data u32 where Self:'file,'data:'file;fn get<'file>(&'file self)->Self::Ref<'file>{self.0}}fn main(){let x=7;let y;{let h=H(&x);y=h.get();}assert_eq!(*y,7);}",
            true,
        ),
        (
            "owner_data_escape",
            "trait R<'data>{type Ref<'file> where Self:'file,'data:'file;fn get<'file>(&'file self)->Self::Ref<'file>;}struct H<'data>(&'data u32);impl<'data>R<'data> for H<'data>{type Ref<'file>=&'data u32 where Self:'file,'data:'file;fn get<'file>(&'file self)->Self::Ref<'file>{self.0}}fn main(){let y;{let x=7;let h=H(&x);y=h.get();}assert_eq!(*y,7);}",
            false,
        ),
        (
            "output_bound",
            "trait L{type Ref<'a>:'a where Self:'a;}struct H;impl L for H{type Ref<'a>=&'a u32;}fn main(){}",
            true,
        ),
        (
            "wrong_output_bound",
            "trait L{type Ref<'a>:'a where Self:'a;}struct H<'b>(&'b u32);impl<'b>L for H<'b>{type Ref<'a>=&'b u32 where Self:'a;}fn main(){}",
            true,
        ),
        (
            "unrelated_output_lifetime",
            "trait L{type Ref<'a>:'a where Self:'a;}struct H<'b>(&'b u32);impl<'b>L for H<'b>{type Ref<'a>=&'b u32;}fn main(){}",
            false,
        ),
        (
            "mutable_family",
            "trait L{type Ref<'a> where Self:'a;fn get<'a>(&'a mut self)->Self::Ref<'a>;}struct H(u32);impl L for H{type Ref<'a>=&'a mut u32;fn get<'a>(&'a mut self)->Self::Ref<'a>{&mut self.0}}fn main(){let mut h=H(7);*h.get()=9;assert_eq!(h.0,9);}",
            true,
        ),
        (
            "mutable_conflict",
            "trait L{type Ref<'a> where Self:'a;fn get<'a>(&'a mut self)->Self::Ref<'a>;}struct H(u32);impl L for H{type Ref<'a>=&'a mut u32;fn get<'a>(&'a mut self)->Self::Ref<'a>{&mut self.0}}fn main(){let mut h=H(7);let a=h.get();let b=h.get();*a+=*b;}",
            false,
        ),
        (
            "alpha_all_binders",
            "trait L{type Ref<'formal> where Self:'formal;fn get<'receiver>(&'receiver self)->Self::Ref<'receiver>;}struct H(u32);impl L for H{type Ref<'borrow>=&'borrow u32;fn get<'value>(&'value self)->Self::Ref<'value>{&self.0}}fn main(){assert_eq!(*H(7).get(),7);}",
            true,
        ),
        (
            "alpha_missing_required",
            "trait L{type Ref<'formal>;fn get<'receiver>(&'receiver self)->Self::Ref<'receiver>;}fn main(){}",
            false,
        ),
        (
            "factory_no_self_bound",
            "trait L{type Ref<'a>;fn make<'a>()->Self::Ref<'a>;}struct H;impl L for H{type Ref<'a>=&'a u32;fn make<'a>()->Self::Ref<'a>{&7}}fn main(){assert_eq!(*H::make(),7);}",
            true,
        ),
        (
            "trait_binder_shadow",
            "trait L<'a>{type Ref<'a> where Self:'a;}fn main(){}",
            false,
        ),
        (
            "impl_binder_shadow",
            "trait L{type Ref<'a> where Self:'a;}struct H<'a>(&'a u32);impl<'a>L for H<'a>{type Ref<'a>=&'a u32 where Self:'a;}fn main(){}",
            false,
        ),
        (
            "iterator_item_wrong_region",
            "trait R<'data>{type Ref<'file> where Self:'file,'data:'file;type Iter<'file>:Iterator<Item=Self::Ref<'file>> where Self:'file,'data:'file;}struct H<'data>(&'data[u32]);impl<'data>R<'data> for H<'data>{type Ref<'file>=&'file u32 where Self:'file,'data:'file;type Iter<'file>=std::slice::Iter<'data,u32> where Self:'file,'data:'file;}fn main(){}",
            false,
        ),
        (
            "iterator_item_matching",
            "trait R<'data>{type Ref<'file> where Self:'file,'data:'file;type Iter<'file>:Iterator<Item=Self::Ref<'file>> where Self:'file,'data:'file;}struct H<'data>(&'data[u32]);impl<'data>R<'data> for H<'data>{type Ref<'file>=&'data u32 where Self:'file,'data:'file;type Iter<'file>=std::slice::Iter<'data,u32> where Self:'file,'data:'file;}fn main(){}",
            true,
        ),
        (
            "selected_impl_static_bound",
            "trait L{type Ref<'a>where Self:'a;}struct H<'d>(&'d u32);impl<'d>L for H<'d>where 'd:'static{type Ref<'a>=&'d u32 where Self:'a;}fn bad<'x>(x:&'x u32)-><H<'x>as L>::Ref<'x>{x}fn main(){}",
            false,
        ),
        (
            "wrong_output_trait_region",
            "trait V<'a>{fn read(&self)->&'a u32;}struct P<'a>(&'a u32);impl<'a>V<'a> for P<'a>{fn read(&self)->&'a u32{self.0}}trait R<'data>{type Ref<'file>:V<'data> where Self:'file,'data:'file;}struct H<'data>(&'data u32);impl<'data>R<'data> for H<'data>{type Ref<'file>=P<'file> where Self:'file,'data:'file;}fn main(){}",
            false,
        ),
        (
            "stored_projection",
            "trait L{type Ref<'a>where Self:'a;}struct W<'a,T:L+'a>{value:T::Ref<'a>}fn keep<'a,T:L+'a>(value:T::Ref<'a>)->W<'a,T>{W{value}}fn main(){}",
            true,
        ),
        (
            "stored_invariant",
            "trait L{type Ref<'a>where Self:'a;}struct W<'a,T:L+'a>{value:T::Ref<'a>}fn bad<'a,'b,T:L+'a+'b>(x:W<'a,T>)->W<'b,T>where 'a:'b{x}fn main(){}",
            false,
        ),
        (
            "selected_type_static_bound",
            "trait F{type Out<'a>;}struct H<T>(T);impl<T:'static>F for H<T>{type Out<'a>=();}fn f<'a,T>(x:&'a T)-><H<T>as F>::Out<'a>{()}fn main(){}",
            false,
        ),
        (
            "selected_type_static_satisfied",
            "trait F{type Out<'a>;}struct H<T>(T);impl<T:'static>F for H<T>{type Out<'a>=();}fn f<'a,T:'static>(x:&'a T)-><H<T>as F>::Out<'a>{()}fn main(){}",
            true,
        ),
        (
            "family_copy_predicate",
            "trait F{type Out<T:Copy>:Copy;}struct S;impl F for S{type Out<U:Copy>=U;}fn main(){let x:<S as F>::Out<u32>=7;assert_eq!(x,7);}",
            true,
        ),
        (
            "family_type_lifetime_bound",
            "trait F{type Out<T>where T:'static;}struct S;impl F for S{type Out<U>=U where U:'static;}fn bad<'a>(x:&'a u32)-><S as F>::Out<&'a u32>{x}fn main(){}",
            false,
        ),
        (
            "family_unused_type_lifetime_bound",
            "trait F{type Out<T>where T:'static;}struct S;impl F for S{type Out<U>=u32 where U:'static;}fn bad<'a>(x:&'a u32)-><S as F>::Out<&'a u32>{7}fn main(){}",
            false,
        ),
        (
            "default_family_method",
            "trait L{type Ref<'a>where Self:'a;fn get<'a>(&'a self)->Self::Ref<'a>;fn again<'a>(&'a self)->Self::Ref<'a>{self.get()}}fn main(){}",
            true,
        ),
        (
            "default_owner_method",
            "trait V<'a>{fn read(&self)->&'a u32;}trait R<'data>{type Out:V<'data>;fn get(&self)->Self::Out;fn read(&self)->&'data u32{self.get().read()}}fn main(){}",
            true,
        ),
        (
            "required_bounds_intersection",
            "trait L{type Ref<'a>;fn get<'a>(&'a self)->Self::Ref<'a>;fn make<'a>()->Self::Ref<'a>;}fn main(){}",
            true,
        ),
        (
            "owner_binding_matching",
            "trait Tab<'d>{type Item;}trait R<'d>{type Val<'f>where Self:'f,'d:'f;type Table<'f>:Tab<'d,Item=Self::Val<'f>>where Self:'f,'d:'f;}struct H<'d>(&'d u32);struct T<'d>(&'d u32);impl<'d>Tab<'d>for T<'d>{type Item=&'d u32;}impl<'d>R<'d>for H<'d>{type Val<'f>=&'d u32 where Self:'f,'d:'f;type Table<'f>=T<'d>where Self:'f,'d:'f;}fn main(){}",
            true,
        ),
        (
            "owner_binding_wrong_region",
            "trait Tab<'d>{type Item;}trait R<'d>{type Val<'f>where Self:'f,'d:'f;type Table<'f>:Tab<'d,Item=Self::Val<'f>>where Self:'f,'d:'f;}struct H<'d>(&'d u32);struct T<'d>(&'d u32);impl<'d>Tab<'d>for T<'d>{type Item=&'d u32;}impl<'d>R<'d>for H<'d>{type Val<'f>=&'f u32 where Self:'f,'d:'f;type Table<'f>=T<'d>where Self:'f,'d:'f;}fn main(){}",
            false,
        ),
        (
            "family_type",
            "trait F{type Out<T>;}struct S;impl F for S{type Out<T>=Option<T>;}fn wrap<T>(x:T)-><S as F>::Out<T>{Some(x)}fn main(){assert_eq!(wrap(7),Some(7));}",
            true,
        ),
        (
            "family_const",
            "trait F{type Out<const N:usize>;}struct S;impl F for S{type Out<const N:usize>=[u8;N];}fn main(){let x:<S as F>::Out<3>=[7;3];assert_eq!(x.len(),3);}",
            true,
        ),
    ] {
        if !case_filter::selected(name) {
            continue;
        }
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let oracle = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("oracle.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            oracle.status.success(),
            accepted,
            "rustc {name}: {}",
            String::from_utf8_lossy(&oracle.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text(&path, source);
        let mut errors = hir.diagnostics.clone();
        if errors.is_empty() {
            let typed = flux_mir::check_file(&hir);
            errors.extend(typed.diagnostics.clone());
            if errors.is_empty() {
                errors.extend(flux_mir::build_file(&hir, &typed).diagnostics);
            }
        }
        assert_eq!(errors.is_empty(), accepted, "flux {name}: {errors:?}");
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn family_pointer_regions_remain_an_explicit_boundary() {
    let root =
        std::env::temp_dir().join(format!("flux-gat-pointer-boundary-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source) in [
        (
            "generic",
            "trait L{type Ref<'a>where Self:'a;}type P<T:L>=for<'a>fn(&'a T)-><T as L>::Ref<'a>;fn main(){}",
        ),
        (
            "concrete",
            "trait L{type Ref<'a>where Self:'a;}struct H;impl L for H{type Ref<'a>=&'a u32;}type P=for<'a>fn(&'a H)-><H as L>::Ref<'a>;fn main(){}",
        ),
        (
            "owner_only",
            "trait V<'a>{type Out;fn get(&self)->Self::Out;}type P<'a,T:V<'a>>=fn()-><T as V<'a>>::Out;fn main(){}",
        ),
    ] {
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let oracle = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("oracle.rmeta"))
            .output()
            .unwrap();
        assert!(
            oracle.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&oracle.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text(&path, source);
        assert!(hir.diagnostics.is_empty(), "{name}: {:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        assert!(
            typed.diagnostics.iter().any(|d| d.message
                == "associated family regions in function pointers are not supported yet"),
            "{name}: {:?}",
            typed.diagnostics
        );
    }
    std::fs::remove_dir_all(root).unwrap();
}

//! Defaulted nominal type arguments use declaration scope and preceding args.
#[test]
fn type_parameter_defaults_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-type-defaults-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepts) in [
        ("struct_partial", "struct S<T,U=u16>(T,U);fn main(){let s=S::<u8>(1,2);let _:u16=s.1;}", true),
        ("dependent", "struct S<T,U=T,V=(T,U)>(T,U,V);fn main(){let s=S::<u8>(1,2,(3,4));let _:u8=s.1;let _:(u8,u8)=s.2;}", true),
        ("expected", "struct S<T=u16>(T);fn main(){let s:S<u32>=S(2);let _:u32=s.0;}", true),
        ("bare_infer", "struct S<T=u16>(T);fn main(){let s=S(2u32);let _:u32=s.0;let t=S::<>(3u64);let _:u64=t.0;}", true),
        ("enum_partial", "enum E<T,U=T>{A(T),B(U)}fn main(){let e=E::<u8>::B(7);match e{E::A(x)|E::B(x)=>{let _:u8=x;}}}", true),
        ("associated", "struct S<T,U=u16>(T,U);impl<T,U:Default>S<T,U>{fn make(x:T)->Self{S(x,U::default())}}fn main(){let s=S::<u8>::make(1);let _:u16=s.1;}", true),
        ("type_alias", "struct S<T,U>(T,U);type A<T=u8,U=T>=S<T,U>;fn check(x:A){let _:u8=x.0;let _:u8=x.1;}fn main(){check(S(1,2));}", true),
        ("alias_record_infer", "struct S<T,U>{x:T,y:U}type A<T=u8,U=T>=S<T,U>;fn main(){let s=A{x:2u32,y:3u64};let _:u32=s.x;let _:u64=s.y;let t=A::<u16>{x:4,y:5};let _:u16=t.y;}", true),
        ("alias_variant", "enum E<T,U>{A(T),B(U)}type A<T=u8,U=T>=E<T,U>;fn main(){let e=A::<u16>::B(7);match e{E::A(x)|E::B(x)=>{let _:u16=x;}}}", true),
        ("scope", "mod m{pub struct State;pub struct S<T,U=State>(pub T,pub U);}struct State;fn main(){let s:m::S<u8>=m::S(1,m::State);let _:m::State=s.1;}", true),
        ("alias_scope", "mod m{pub struct State;pub struct S<T,U>(pub T,pub U);pub type A<T=u8,U=State>=S<T,U>;}struct State;fn main(){let s:m::A=m::S(1,m::State);let _:m::State=s.1;}", true),
        ("projection", "trait Tr{type Out;}impl Tr for u8{type Out=u16;}struct S<T:Tr,U=<T as Tr>::Out>(T,U);fn main(){let s=S::<u8>(1,2);let _:u16=s.1;}", true),
        ("alias_projection", "trait Tr{type Out;}impl Tr for u8{type Out=u16;}struct S<T,U>(T,U);type A<T:Tr,U=<T as Tr>::Out>=S<T,U>;fn main(){let s:A<u8>=S(1,2);let _:u16=s.1;}", true),
        ("trait_unchanged", "trait Tr<T:?Sized=Self>{fn same(&self,x:&T)->bool;}impl Tr for u8{fn same(&self,x:&u8)->bool{*self==*x}}fn main(){assert!(3u8.same(&3));}", true),
        ("const_literal", "struct B<const N:usize=1>{a:[u8;N]}impl B<1>{fn new()->Self{B{a:[7]}}}fn main(){let b:B=B::new();let _:[u8;1]=b.a;let c:B<8>=B{a:[3;8]};let _:[u8;8]=c.a;}", true),
        ("const_partial", "struct B<T,const N:usize=2>(T,[u8;N]);fn main(){let b=B::<u16>(3,[4;2]);let _:[u8;2]=b.1;}", true),
        ("const_dependent", "struct B<const N:usize=2,const M:usize=N>([u8;N],[u8;M]);fn main(){let b:B<3>=B([1;3],[2;3]);let _:[u8;3]=b.1;}", true),
        ("const_alias", "struct B<const N:usize>([u8;N]);type A<const N:usize=4>=B<N>;fn main(){let b:A=B([1;4]);let _:[u8;4]=b.0;}", true),
        ("const_enum", "enum E<const N:usize=2>{A([u8;N])}fn main(){let e:E=E::A([1;2]);match e{E::A(a)=>{let _:[u8;2]=a;}}}", true),
        ("const_scope", "mod m{const N:usize=3;pub struct B<const X:usize=N>(pub [u8;X]);}const N:usize=4;fn main(){let b:m::B=m::B([1;3]);let _:[u8;3]=b.0;}", true),
        ("const_bool_char", "struct B<const X:bool=true,const C:char='x'>;fn main(){let _:B=B;let _:B<false,'y'>=B;}", true),
        ("const_trait", "trait Tr<const N:usize=3>{fn value()->usize{N}}struct S;impl Tr for S{}fn main(){let _:usize=<S as Tr>::value();}", true),
        ("const_wrong_type", "struct B<const N:usize=true>;fn main(){}", false),
        ("const_char_wrong_type", "struct B<const N:char=1>;fn main(){}", false),
        ("const_forward", "struct B<const N:usize=M,const M:usize=2>;fn main(){}", false),
        ("const_self", "struct B<const N:usize=N>;fn main(){}", false),
        ("const_type_forward", "struct B<T=[u8;N],const N:usize=2>(T);fn main(){}", false),
        ("const_required_after", "struct B<const N:usize=1,const M:usize>;fn main(){}", false),
        ("const_function_default", "fn f<const N:usize=1>(){}fn main(){}", false),
        ("const_missing", "struct B<const N:usize=ABSENT>;fn main(){}", false),
        ("const_too_many", "struct B<const N:usize=1>;fn main(){let _:B<1,2>=B;}", false),
        ("const_too_few", "struct B<T,const N:usize=1>(T);fn f(_:B){}fn main(){}", false),
        ("too_few", "struct S<T,U=u16>(T,U);fn f(_:S<>){}fn main(){}", false),
        ("too_many", "struct S<T,U=u16>(T,U);fn f(_:S<u8,u16,u32>){}fn main(){}", false),
        ("expr_many", "struct S<T,U=u16>(T,U);fn main(){let _=S::<u8,u16,u32>(1,2);}", false),
        ("alias_few", "type A<T,U=u16>=(T,U);fn f(_:A){}fn main(){}", false),
        ("alias_many", "type A<T=u8>=(T,T);fn f(_:A<u8,u16>){}fn main(){}", false),
        ("function_default", "fn f<T=u8>(x:T)->T{x}fn main(){}", false),
        ("unqualified_projection", "trait Tr{type Out;}struct S<T:Tr,U=T::Out>(T,U);fn main(){}", false),
        ("forward", "struct S<T=U,U=u8>(T,U);fn main(){}", false),
        ("self", "struct S<T=Self>(T);fn main(){}", false),
        ("alias_forward", "type A<T=U,U=u8>=(T,U);fn main(){}", false),
        ("missing", "struct S<T=Missing>(T);fn main(){}", false),
        ("alias_missing", "type A<T=Missing>=(T,T);fn main(){}", false),
        ("cycle", "struct S<T=S>(T);fn main(){}", false),
        ("alias_cycle", "type A<T=A>=(T,T);fn main(){}", false),
        ("explicit_default_fixed", "struct S<T,U=T>(T,U);fn main(){let _:S<u8,u32>=S::<u8>(1,2);}", false),
    ] {
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let oracle = std::process::Command::new("rustc").args(["--edition=2024", "--emit=metadata"])
            .arg(&path).arg("-o").arg(root.join("probe.rmeta")).output().unwrap();
        assert_eq!(oracle.status.success(), accepts, "rustc {name}: {}", String::from_utf8_lossy(&oracle.stderr));
        let (db, _, hir) = flux_hir::lower_text(&path, source);
        assert!(db.syntax_errors().is_empty(), "{name}: {:?}", db.syntax_errors());
        let mut errors = hir.diagnostics.clone();
        if errors.is_empty() { errors.extend(flux_ty::check_file(&hir).diagnostics); }
        assert_eq!(errors.is_empty(), accepts, "Flux {name}: {errors:?}");
    }
    std::fs::remove_dir_all(root).unwrap();
}

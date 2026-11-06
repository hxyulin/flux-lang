//! Type-level constants use the same checked evaluator as ordinary const items.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;

#[test]
fn array_length_constants_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-array-length-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepts) in [
        (
            "trait_constant",
            "trait Size{const N:usize;}struct S;impl Size for S{const N:usize=3;}fn main(){let a:[u8;S::N]=[7;3];assert_eq!(a[2],7);}",
            true,
        ),
        (
            "qualified_trait_constant",
            "trait Size{const N:usize;}struct S;impl Size for S{const N:usize=3;}fn main(){let _: [u8;<S as Size>::N]=[7;3];}",
            true,
        ),
        (
            "generic_impl_concrete",
            "struct W<T>(T);impl<T> W<T>{const N:usize=4;}fn main(){let _:[u8;W::<u32>::N]=[1;4];}",
            true,
        ),
        (
            "specialized_impl",
            "struct W<const N:usize>;impl W<3>{const N:usize=5;}fn main(){let _:[u8;W::<3>::N]=[1;5];}",
            true,
        ),
        (
            "primitive_associated",
            "fn main(){let _:[u8;u8::BITS as usize]=[1;8];}",
            true,
        ),
        (
            "concrete_self",
            "struct S;impl S{const N:usize=3;fn f()->[u8;Self::N]{[7;Self::N]}}fn main(){assert_eq!(S::f().len(),3);}",
            true,
        ),
        (
            "inherited_constant",
            "trait A{const N:usize=3;}trait B:A{}struct S;impl A for S{}impl B for S{}fn main(){let _:[u8;<S as A>::N]=[1;3];}",
            true,
        ),
        (
            "module_constant",
            "mod m{pub const N:usize=2;}fn main(){let _:[u8;m::N]=[1;2];}",
            true,
        ),
        (
            "imported_constant",
            "mod m{pub const N:usize=2;}use m::N;fn main(){let _:[u8;N]=[1;2];}",
            true,
        ),
        (
            "arithmetic",
            "fn main(){let _:[u8;(2+3)*4/2-1]=[7;9];}",
            true,
        ),
        (
            "bit_operators",
            "fn main(){let _:[u8;((1<<4)|3)^((8>>1)&7)]=[7;23];}",
            true,
        ),
        (
            "block_locals",
            "fn main(){let _:[u8;{let n=2; n*n+1}]=[7;5];}",
            true,
        ),
        (
            "const_function",
            "const fn count(n:usize)->usize{if n>1 {n*2}else{1}}fn main(){let _:[u8;count(3)]=[7;6];}",
            true,
        ),
        (
            "loop",
            "const fn sum()->usize{let mut n=0;let mut i=0;while i<4{n+=i;i+=1;}n}fn main(){let _:[u8;sum()]=[7;6];}",
            true,
        ),
        (
            "index",
            "const V:[usize;3]=[2,3,4];fn main(){let _:[u8;V[1]+1]=[7;4];}",
            true,
        ),
        (
            "field",
            "struct C{n:usize}const C0:C=C{n:3};fn main(){let _:[u8;C0.n]=[7;3];}",
            true,
        ),
        (
            "nested_dependency",
            "const A:usize=2+1;const B:usize=[0u8;A].len()+1;fn main(){let _:[u8;B]=[7;4];}",
            true,
        ),
        (
            "const_parameter",
            "fn make<const N:usize>()->[u8;N]{[1;N]}fn main(){let _:[u8;3]=make::<3>();}",
            true,
        ),
        (
            "repeat_associated",
            "trait Size{const N:usize;}struct S;impl Size for S{const N:usize=3;}fn main(){let a=[7;S::N];assert_eq!(a.len(),3);}",
            true,
        ),
        (
            "alias_associated",
            "trait Size{const N:usize;}struct S;impl Size for S{const N:usize=3;}type A=[u8;S::N];fn main(){let _:A=[1;3];}",
            true,
        ),
        (
            "generic_impl_size",
            "struct W<T>(T);impl<T>W<T>{const N:usize=core::mem::size_of::<T>();}fn main(){let _:[u8;W::<u32>::N]=[7;4];let _:[u8;W::<u64>::N]=[7;8];}",
            true,
        ),
        (
            "const_impl_parameter",
            "struct W<const N:usize>;impl<const N:usize>W<N>{const LEN:usize=N+1;}fn main(){let _:[u8;W::<3>::LEN]=[7;4];}",
            true,
        ),
        (
            "alias_type_constant",
            "trait Size{const N:usize;}struct W;impl Size for W{const N:usize=3;}type Alias=W;fn main(){let _:[u8;Alias::N]=[7;3];}",
            true,
        ),
        (
            "nested_type_constant",
            "trait Size{const N:usize;}struct W<T>(T);impl<T>Size for W<T>{const N:usize=3;}fn main(){let _:[u8;<W<W<u8>> as Size>::N]=[7;3];}",
            true,
        ),
        (
            "wrong_element_count",
            "trait Size{const N:usize;}struct S;impl Size for S{const N:usize=3;}fn main(){let _:[u8;S::N]=[7;4];}",
            false,
        ),
        (
            "wrong_const_type",
            "struct S;impl S{const N:u32=3;}fn main(){let _:[u8;S::N]=[7;3];}",
            false,
        ),
        (
            "generic_type_operation",
            "fn f<T>()->[u8;core::mem::size_of::<T>()]{loop{}}fn main(){}",
            false,
        ),
        (
            "generic_const_operation",
            "fn f<const N:usize>()->[u8;N+1]{loop{}}fn main(){}",
            false,
        ),
        (
            "generic_associated",
            "trait Size{const N:usize;}fn f<T:Size>()->[u8;T::N]{loop{}}fn main(){}",
            false,
        ),
        (
            "generic_self",
            "struct S<T>(T);impl<T>S<T>{const N:usize=3;fn f()->[u8;Self::N]{[1;Self::N]}}fn main(){}",
            false,
        ),
        (
            "runtime_local",
            "fn main(){let n=3;let _:[u8;n+1]=[7;4];}",
            false,
        ),
        (
            "non_const_call",
            "fn count()->usize{3}fn main(){let _:[u8;count()]=[7;3];}",
            false,
        ),
        ("overflow", "fn main(){let _:[u8;usize::MAX+1]=[];}", false),
        ("division_zero", "fn main(){let _:[u8;3/0]=[];}", false),
        (
            "cycle",
            "struct S;impl S{const N:usize=Self::N;}fn main(){let _:[u8;S::N]=[];}",
            false,
        ),
        (
            "missing_associated",
            "struct S;fn main(){let _:[u8;S::N]=[];}",
            false,
        ),
        // keel's `Cover::ALL`: a field length that names a constant whose
        // own type has a computed length, resolved in a later round.
        (
            "constant_type_length",
            "#[derive(Clone,Copy)]enum C{A,B}impl C{const ALL:[Self;[\"A\",\"B\"].len()]=[Self::A,Self::B];}struct P{c:[u64;C::ALL.len()]}fn main(){let p=P{c:[1;2]};assert_eq!(p.c.len(),2);}",
            true,
        ),
        (
            "constant_type_length_chain",
            "#[derive(Clone,Copy)]enum C{A,B,D}impl C{const ALL:[Self;[\"A\",\"B\",\"D\"].len()]=[Self::A,Self::B,Self::D];}struct M{m:[C;C::ALL.len()+1]}impl M{const ONE:M=M{m:[C::A;4]};}struct T{t:[u8;M::ONE.m.len()*2]}fn main(){let t=T{t:[1;8]};assert_eq!(t.t.len(),8);}",
            true,
        ),
        // keel-opt's pass lists: references to named constants inside a
        // constant slice, read through when evaluating a length.
        (
            "named_constant_references",
            "const R:[&str;2]=[\"a\",\"b\"];const P:&[&[&str]]=&[&[\"x\"],&R];const fn total(p:&[&[&str]])->usize{let mut t=0;let mut i=0;while i<p.len(){t+=p[i].len();i+=1;}t}fn main(){let a=[0u8;total(P)];assert_eq!(a.len(),3);}",
            true,
        ),
        (
            "constant_type_unresolved",
            "const ALL:[Undefined;3]=[1,2,3];struct P{c:[u8;ALL.len()]}fn main(){}",
            false,
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
            .arg(root.join("probe.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            oracle.status.success(),
            accepts,
            "rustc {name}: {}",
            String::from_utf8_lossy(&oracle.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text(&path, source);
        let mut errors = hir.diagnostics.clone();
        if errors.is_empty() {
            let typed = flux_mir::check_file(&hir);
            errors.extend(typed.diagnostics.iter().cloned());
            if errors.is_empty() {
                errors.extend(flux_mir::build_file(&hir, &typed).diagnostics);
            }
        }
        assert_eq!(errors.is_empty(), accepts, "Flux {name}: {errors:?}");
    }
    std::fs::remove_dir_all(root).unwrap();
}

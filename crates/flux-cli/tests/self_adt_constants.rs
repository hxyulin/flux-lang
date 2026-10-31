//! Anonymous field constants retain their own non-generic ADT Self.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
use std::process::Command;
#[test]
fn field_constant_self_matches_rustc() {
    let root = std::env::temp_dir().join(format!("flux-adt-self-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source) in [
        (
            "conditional",
            "struct S{a:[u8;if Self::ON{Self::N*2}else{0}]}impl S{const ON:bool=true;const N:usize=3;}fn main(){let x=S{a:[7;6]};assert_eq!(x.a.len(),6);println!(\"{}\",x.a[5]);}",
        ),
        (
            "false_branch",
            "struct S{a:[u8;if Self::ON{0}else{Self::N}]}impl S{const ON:bool=false;const N:usize=5;}fn main(){assert_eq!(core::mem::size_of::<S>(),5);}",
        ),
        (
            "direct_path",
            "struct S{a:[u8;Self::N]}impl S{const N:usize=3;}fn main(){let x=S{a:[1;3]};assert_eq!(x.a.len(),3);}",
        ),
        (
            "self_type",
            "struct S{a:[u8;{let _:Option<Self>=None;Self::N}]}impl S{const N:usize=2;}fn main(){assert_eq!(core::mem::size_of::<S>(),2);}",
        ),
        (
            "pointer_size",
            "struct S{a:[u8;{let _=core::mem::size_of::<*const Self>();Self::N}]}impl S{const N:usize=2;}fn main(){assert_eq!(S{a:[7;2]}.a[1],7);}",
        ),
        (
            "borrowed_none",
            "struct S{a:[u8;{let _:Option<&Self>=None;Self::N}]}impl S{const N:usize=2;}fn main(){assert_eq!(S{a:[7;2]}.a[1],7);}",
        ),
        (
            "raw_none",
            "struct S{a:[u8;{let _:Option<*const Self>=None;Self::N}]}impl S{const N:usize=2;}fn main(){assert_eq!(S{a:[7;2]}.a[1],7);}",
        ),
        (
            "function_none",
            "struct S{a:[u8;{let _:Option<fn(Self)>=None;Self::N}]}impl S{const N:usize=2;}fn main(){assert_eq!(S{a:[7;2]}.a[1],7);}",
        ),
        (
            "return_function_none",
            "struct S{a:[u8;{let _:Option<fn()->Self>=None;Self::N}]}impl S{const N:usize=2;}fn main(){assert_eq!(S{a:[7;2]}.a[1],7);}",
        ),
        (
            "raw_cast",
            "struct S{a:[u8;{let _=0usize as *const Self;Self::N}]}impl S{const N:usize=2;}fn main(){assert_eq!(S{a:[7;2]}.a[1],7);}",
        ),
        (
            "generic_raw_cast",
            "const fn f<T>()->*const T{0usize as *const T}struct S{a:[u8;{let _=f::<Self>();Self::N}]}impl S{const N:usize=2;}fn main(){assert_eq!(S{a:[7;2]}.a[1],7);}",
        ),
        (
            "sized_noop",
            "const fn f<T>(){}struct S{a:[u8;{f::<Self>();Self::N}]}impl S{const N:usize=2;}fn main(){assert_eq!(S{a:[7;2]}.a[1],7);}",
        ),
        (
            "unsized_noop",
            "const fn f<T:?Sized>(){}struct S{a:[u8;{f::<Self>();Self::N}]}impl S{const N:usize=2;}fn main(){assert_eq!(S{a:[7;2]}.a[1],7);}",
        ),
        (
            "unused_alias",
            "struct S{a:[u8;{type Alias=S;Self::N}]}impl S{const N:usize=2;}fn main(){assert_eq!(S{a:[7;2]}.a[1],7);}",
        ),
        (
            "wild_uninit",
            "struct S{a:[u8;{let _:Option<Self>;Self::N}]}impl S{const N:usize=2;}fn main(){assert_eq!(S{a:[7;2]}.a[1],7);}",
        ),
        (
            "phantom",
            "struct S{a:[u8;{let _=core::marker::PhantomData::<Self>;Self::N}]}impl S{const N:usize=2;}fn main(){assert_eq!(S{a:[7;2]}.a[1],7);}",
        ),
        (
            "dead_layout",
            "struct S{a:[u8;{if false{let _=core::mem::size_of::<Self>();}Self::N}]}impl S{const N:usize=2;}fn main(){assert_eq!(S{a:[7;2]}.a[1],7);}",
        ),
        (
            "dead_nested_null",
            "const fn f<T>()->*const T{core::ptr::null::<T>()}struct S{a:[u8;{if false{let _=f::<Self>();}Self::N}]}impl S{const N:usize=2;}fn main(){assert_eq!(S{a:[7;2]}.a[1],7);}",
        ),
        (
            "nested_pointer_wrapper",
            "struct W<T>(*const T);struct S{a:[u8;{let _:Option<W<W<Self>>>=None;Self::N}]}impl S{const N:usize=2;}fn main(){assert_eq!(S{a:[7;2]}.a[1],7);}",
        ),
        (
            "named_owned_reject",
            "struct S{a:[u8;{let _:Option<S>=None;Self::N}]}impl S{const N:usize=2;}fn main(){assert_eq!(S{a:[7;2]}.a[1],7);}",
        ),
        (
            "owned_uninit_reject",
            "struct S{a:[u8;{let local:Option<Self>;Self::N}]}impl S{const N:usize=2;}fn main(){assert_eq!(S{a:[7;2]}.a[1],7);}",
        ),
        (
            "manually_drop_reject",
            "struct S{a:[u8;{let _:Option<core::mem::ManuallyDrop<Self>>=None;Self::N}]}impl S{const N:usize=2;}fn main(){assert_eq!(S{a:[7;2]}.a[1],7);}",
        ),
        (
            "maybe_uninit_reject",
            "struct S{a:[u8;{let _:Option<core::mem::MaybeUninit<Self>>=None;Self::N}]}impl S{const N:usize=2;}fn main(){assert_eq!(S{a:[7;2]}.a[1],7);}",
        ),
        (
            "zero_array_reject",
            "struct S{a:[u8;{let _:[Self;0]=[];Self::N}]}impl S{const N:usize=2;}fn main(){assert_eq!(S{a:[7;2]}.a[1],7);}",
        ),
        (
            "closure_parameter_reject",
            "struct S{a:[u8;{let _=|_:Self|{};Self::N}]}impl S{const N:usize=2;}fn main(){assert_eq!(S{a:[7;2]}.a[1],7);}",
        ),
        (
            "dead_owned_reject",
            "struct S{a:[u8;{if false{let _:Option<Self>=None;}Self::N}]}impl S{const N:usize=2;}fn main(){assert_eq!(S{a:[7;2]}.a[1],7);}",
        ),
        (
            "layout_reject",
            "struct S{a:[u8;{let _=core::mem::size_of::<Self>();Self::N}]}impl S{const N:usize=2;}fn main(){assert_eq!(S{a:[7;2]}.a[1],7);}",
        ),
        (
            "align_reject",
            "struct S{a:[u8;{let _=core::mem::align_of::<Self>();Self::N}]}impl S{const N:usize=2;}fn main(){assert_eq!(S{a:[7;2]}.a[1],7);}",
        ),
        (
            "needs_drop_reject",
            "struct S{a:[u8;{let _=core::mem::needs_drop::<Self>();Self::N}]}impl S{const N:usize=2;}fn main(){assert_eq!(S{a:[7;2]}.a[1],7);}",
        ),
        (
            "null_reject",
            "struct S{a:[u8;{let _=core::ptr::null::<Self>();Self::N}]}impl S{const N:usize=2;}fn main(){assert_eq!(S{a:[7;2]}.a[1],7);}",
        ),
        (
            "null_dead_reject",
            "struct S{a:[u8;{if false{let _=core::ptr::null::<Self>();}Self::N}]}impl S{const N:usize=2;}fn main(){assert_eq!(S{a:[7;2]}.a[1],7);}",
        ),
        (
            "null_mut_reject",
            "struct S{a:[u8;{let _=core::ptr::null_mut::<Self>();Self::N}]}impl S{const N:usize=2;}fn main(){assert_eq!(S{a:[7;2]}.a[1],7);}",
        ),
        (
            "null_nested_reject",
            "const fn f<T>()->*const T{core::ptr::null::<T>()}struct S{a:[u8;{let _=f::<Self>();Self::N}]}impl S{const N:usize=2;}fn main(){assert_eq!(S{a:[7;2]}.a[1],7);}",
        ),
        (
            "null_item_reject",
            "struct S{a:[u8;{let _=core::ptr::null::<Self>;Self::N}]}impl S{const N:usize=2;}fn main(){assert_eq!(S{a:[7;2]}.a[1],7);}",
        ),
        (
            "enum",
            "enum S{A([u8;{Self::N+1}])}impl S{const N:usize=2;}fn main(){let S::A(a)=S::A([4;3]);assert_eq!(a[2],4);}",
        ),
        (
            "union",
            "union S{a:[u8;{Self::N+1}],b:u32}impl S{const N:usize=3;}fn main(){let x=S{a:[7;4]};assert_eq!(unsafe{x.a[3]},7);}",
        ),
        (
            "nested_names",
            "struct S{a:[u8;{Self::N}]}impl S{const N:usize=2;}mod child{pub struct S{pub a:[u8;{Self::N}]}impl S{const N:usize=3;}}fn main(){assert_eq!(S{a:[0;2]}.a.len(),2);assert_eq!(child::S{a:[0;3]}.a.len(),3);}",
        ),
        (
            "nested_const_block",
            "struct S{a:[u8;const{Self::N+1}]}impl S{const N:usize=2;}fn main(){assert_eq!(S{a:[0;3]}.a.len(),3);}",
        ),
        (
            "generic_type_reject",
            "struct S<T>{a:[u8;{Self::N}],t:T}impl<T>S<T>{const N:usize=3;}fn main(){}",
        ),
        (
            "generic_const_reject",
            "struct S<const N:usize>{a:[u8;{Self::N}]}impl<const N:usize>S<N>{const N:usize=N;}fn main(){}",
        ),
        (
            "generic_lifetime_reject",
            "struct S<'a>{a:[u8;{Self::N}],t:&'a()}impl<'a>S<'a>{const N:usize=3;}fn main(){}",
        ),
        (
            "inner_function_reject",
            "struct S{a:[u8;{fn f()->usize{Self::N}f()}]}impl S{const N:usize=3;}fn main(){}",
        ),
        (
            "inner_item_reject",
            "struct S{a:[u8;{const N:usize=Self::N;N}]}impl S{const N:usize=3;}fn main(){}",
        ),
        ("outside_reject", "const N:usize=Self::N;fn main(){}"),
    ] {
        let input = root.join(format!("{name}.rs"));
        std::fs::write(&input, source).unwrap();
        let reference = root.join(format!("{name}-rustc"));
        let expected = Command::new("rustc")
            .args(["--edition=2024", "-C", "panic=abort"])
            .arg(&input)
            .arg("-o")
            .arg(&reference)
            .output()
            .unwrap();
        for optimized in [false, true] {
            let output = root.join(format!("{name}-{optimized}"));
            let mut cmd = Command::new(env!("CARGO_BIN_EXE_fluxc"));
            cmd.arg("build").arg(&input).arg("-o").arg(&output);
            if optimized {
                cmd.arg("-O");
            }
            let actual = cmd.output().unwrap();
            assert_eq!(
                actual.status.success(),
                expected.status.success(),
                "{name}: rustc={} flux={}",
                String::from_utf8_lossy(&expected.stderr),
                String::from_utf8_lossy(&actual.stderr)
            );
            if expected.status.success() {
                let rustc = Command::new(&reference).output().unwrap();
                let flux = Command::new(output).output().unwrap();
                assert_eq!(flux.status.code(), rustc.status.code(), "{name}");
                assert_eq!(flux.stdout, rustc.stdout, "{name}");
                assert_eq!(flux.stderr, rustc.stderr, "{name}");
            }
        }
    }
    std::fs::remove_dir_all(root).unwrap();
}

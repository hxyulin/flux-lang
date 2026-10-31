//! Imports finish each namespace; private inherent functions permit trait lookup.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
use std::process::Command;

#[test]
fn namespace_candidates_match_rustc() {
    let root =
        std::env::temp_dir().join(format!("flux-namespace-candidates-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source) in [
        (
            "late_function",
            "mod consumer{use crate::value;pub fn read()->u8{value()}}mod value{}mod source{pub fn value()->u8{7}}pub use crate::source::value;fn main(){println!(\"{}\",consumer::read());}",
        ),
        (
            "reexport_chain",
            "mod consumer{use crate::value;pub fn read()->u8{value()}}mod value{}pub use middle::value;mod middle{pub use crate::source::value;}mod source{pub fn value()->u8{9}}fn main(){println!(\"{}\",consumer::read());}",
        ),
        (
            "macro_namespace",
            "mod consumer{use crate::value;pub fn read()->u8{value!()}}mod value{}mod source{macro_rules! value{()=>{11u8}}pub(crate) use value;}use source::value;fn main(){println!(\"{}\",consumer::read());}",
        ),
        (
            "glob_then_value",
            "mod consumer{use crate::value;pub fn read()->u8{value()}}mod value{}use source::*;mod source{pub fn value()->u8{13}}fn main(){println!(\"{}\",consumer::read());}",
        ),
        (
            "duplicate_values",
            "mod value{}mod a{pub fn value(){}}mod b{pub fn value(){}}use a::value;use b::value;fn main(){}",
        ),
        ("missing", "use crate::missing;fn main(){}"),
        (
            "private_trait_fallback",
            "trait Make{fn custom(x:u8)->Self;}mod m{pub struct E(pub u8);impl E{fn custom(x:u8,_y:u8)->Self{Self(x)}}impl crate::Make for E{fn custom(x:u8)->Self{Self(x)}}}fn main(){println!(\"{}\",m::E::custom(17).0);}",
        ),
        (
            "private_item_value",
            "trait Make{fn custom(x:u8)->Self;}mod m{pub struct E(pub u8);impl E{fn custom(x:u8,_y:u8)->Self{Self(x)}}impl crate::Make for E{fn custom(x:u8)->Self{Self(x)}}}fn main(){let f:fn(u8)->m::E=m::E::custom;println!(\"{}\",f(19).0);}",
        ),
        (
            "private_same_arity",
            "trait Make{fn custom(x:u8)->Self;}mod m{pub struct E(pub u8);impl E{fn custom(x:u8)->Self{Self(x+1)}}impl crate::Make for E{fn custom(x:u8)->Self{Self(x)}}}fn main(){println!(\"{}\",m::E::custom(23).0);}",
        ),
        (
            "local_inherent_wins",
            "trait Make{fn custom(x:u8)->Self;}struct E(u8);impl E{fn custom(x:u8)->Self{Self(x+1)}}impl Make for E{fn custom(x:u8)->Self{Self(x)}}fn main(){println!(\"{}\",E::custom(29).0);}",
        ),
        (
            "private_no_trait",
            "mod m{pub struct E;impl E{fn custom()->Self{Self}}}fn main(){let _=m::E::custom();}",
        ),
        (
            "trait_not_imported",
            "mod m{pub trait Make{fn custom()->Self;}pub struct E;impl E{fn custom()->Self{Self}}impl Make for E{fn custom()->Self{Self}}}fn main(){let _=m::E::custom();}",
        ),
        (
            "public_wrong_arity",
            "trait Make{fn custom(x:u8)->Self;}mod m{pub struct E;impl E{pub fn custom(x:u8,_y:u8)->Self{Self}}impl crate::Make for E{fn custom(x:u8)->Self{Self}}}fn main(){let _=m::E::custom(1);}",
        ),
        (
            "ambiguous_traits",
            "trait A{fn custom()->Self;}trait B{fn custom()->Self;}mod m{pub struct E;impl E{fn custom()->Self{Self}}impl crate::A for E{fn custom()->Self{Self}}impl crate::B for E{fn custom()->Self{Self}}}fn main(){let _=m::E::custom();}",
        ),
        (
            "private_constant",
            "trait Make{const VALUE:u8;}mod m{pub struct E;impl E{const VALUE:u8=3;}impl crate::Make for E{const VALUE:u8=7;}}fn main(){println!(\"{}\",m::E::VALUE);}",
        ),
        (
            "specialized_private",
            "trait Make{fn f()->u8;}mod m{pub struct S<T>(pub T);impl S<u8>{pub fn f()->u8{1}}impl S<u16>{fn f()->u8{2}}impl crate::Make for S<u16>{fn f()->u8{3}}}fn main(){println!(\"{}\",m::S::<u16>::f());}",
        ),
        (
            "specialized_reverse",
            "trait Make{fn f()->u8;}mod m{pub struct S<T>(pub T);impl S<u16>{fn f()->u8{2}}impl S<u8>{pub fn f()->u8{1}}impl crate::Make for S<u16>{fn f()->u8{3}}}fn main(){println!(\"{}\",m::S::<u16>::f());}",
        ),
        (
            "specialized_private_no_trait",
            "mod m{pub struct S<T>(pub T);impl S<u8>{pub fn f()->u8{1}}impl S<u16>{fn f()->u8{2}}}fn main(){println!(\"{}\",m::S::<u16>::f());}",
        ),
        (
            "specialized_constant",
            "trait Make{const C:u8;}mod m{pub struct S<T>(pub T);impl S<u8>{pub const C:u8=1;}impl S<u16>{const C:u8=2;}impl crate::Make for S<u16>{const C:u8=3;}}fn main(){println!(\"{} {}\",m::S::<u16>::C,m::S::<u8>::C);}",
        ),
        (
            "private_method_fallback",
            "trait Make{fn f(&self)->u8;}mod m{pub struct S;impl S{fn f(&self)->u8{1}}impl crate::Make for S{fn f(&self)->u8{3}}}fn main(){println!(\"{}\",m::S.f());}",
        ),
        (
            "public_type_private_value",
            "mod a{pub enum N{V}fn N()->u8{7}}use a::N;fn main(){let _=N::V;}",
        ),
        (
            "private_type_public_value",
            "mod a{mod N{}pub fn N()->u8{7}}use a::N;fn main(){println!(\"{}\",N());}",
        ),
    ] {
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let reference = root.join(format!("{name}-rustc"));
        let expected = Command::new("rustc")
            .args(["--edition=2024", "-C", "panic=abort"])
            .arg(&path)
            .arg("-o")
            .arg(&reference)
            .output()
            .unwrap();
        for optimized in [false, true] {
            let output = root.join(format!("{name}-{optimized}"));
            let mut command = Command::new(env!("CARGO_BIN_EXE_fluxc"));
            command.arg("build").arg(&path).arg("-o").arg(&output);
            if optimized {
                command.arg("-O");
            }
            let actual = command.output().unwrap();
            assert_eq!(
                actual.status.success(),
                expected.status.success(),
                "{name}: rustc={} flux={}",
                String::from_utf8_lossy(&expected.stderr),
                String::from_utf8_lossy(&actual.stderr)
            );
            if expected.status.success() {
                let expected = Command::new(&reference).output().unwrap();
                let actual = Command::new(&output).output().unwrap();
                assert_eq!(actual.status.code(), expected.status.code(), "{name}");
                assert_eq!(actual.stdout, expected.stdout, "{name}");
                assert_eq!(actual.stderr, expected.stderr, "{name}");
            }
        }
    }
    std::fs::remove_dir_all(root).unwrap();
}

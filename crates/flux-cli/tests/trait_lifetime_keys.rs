//! Generic lifetime trait contracts used by the current backend's key views.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn trait_lifetime_keys_match_rustc() {
    support::oracle::compare_fixture("trait_lifetime_keys");
}
#[test]
fn invalid_key_views_match_rustc() {
    use std::{process::Command, time::Duration};
    let scratch = support::scratch::Scratch::new(std::env::temp_dir().join("flux-key-view-errors"));
    let source = include_str!("../../../tests/run/trait_lifetime_keys.rs");
    let prefix = source.split("fn main()").next().unwrap();
    for (name, tail) in [
        (
            "object_container_default",
            "fn escape<'a>(marker:&'a ())->Box<dyn core::fmt::Display>{let holder:ObjectHolder<'a,dyn core::fmt::Display>=ObjectHolder{marker,value:Box::new(7)};holder.value}fn main(){}",
        ),
        (
            "object_alias_container_default",
            "fn escape<'a>(marker:&'a ())->Box<dyn core::fmt::Display>{let holder:BoundObject<'a,dyn core::fmt::Display>=ObjectHolder{marker,value:Box::new(7)};holder.value}fn main(){}",
        ),
        (
            "object_ambiguous_container",
            "struct Ambiguous<'a,'b,T:?Sized+'a+'b>{first:&'a (),second:&'b (),value:Box<T>}fn ambiguous<'a,'b>()->Ambiguous<'a,'b,dyn core::fmt::Display>{panic!()}fn main(){}",
        ),
        (
            "flatten_static_escape",
            "fn escape<'a>(nodes:&'a Nodes<&'a i32,1>)->impl Iterator<Item=(&'a i32,u32)>+'static{nodes.iter()}fn main(){}",
        ),
        (
            "flatten_mutation",
            "fn main(){let mut nodes=Nodes{nodes:[Some((1,2))],len:1};let mut iterator=nodes.iter();nodes.nodes[0]=None;println!(\"{:?}\",iterator.next());}",
        ),
        (
            "object_alias_default",
            "type Alias=Box<dyn core::fmt::Display>;fn main(){let value=String::from(\"local\");let display:Alias=Box::new(value.as_str());println!(\"{}\",display);}",
        ),
        (
            "object_explicit_static",
            "fn main(){let value=String::from(\"local\");let display:Box<dyn core::fmt::Display+'static>=Box::new(value.as_str());println!(\"{}\",display);}",
        ),
        (
            "object_return_default",
            "fn escape<'a>(value:&'a str)->Box<dyn core::fmt::Display>{let display:Box<dyn core::fmt::Display>=Box::new(value);display}fn main(){}",
        ),
        (
            "object_static_supertrait",
            "fn main(){let value=String::from(\"local\");let object:Box<dyn core::any::Any>=Box::new(value.as_str());drop(object);}",
        ),
        (
            "self_static_reference",
            "fn main(){let value=4;let output=StaticView(&value).get();println!(\"{}\",output);}",
        ),
        (
            "self_static_type",
            "fn main(){let value=4;fn id<T>(_:&T)->core::any::TypeId{core::marker::PhantomData::<T>.id()}let _=id(&value);}",
        ),
        (
            "callback_escape",
            "fn main(){let keys=[String::from(\"local\")];let saved=std::cell::Cell::new(None);View(&keys).position(|key|{saved.set(Some(key));std::cmp::Ordering::Equal});println!(\"{}\",saved.get().unwrap());}",
        ),
        (
            "escape",
            "fn escape()->&'static String{let keys=[String::from(\"local\")];first(View(&keys))}fn main(){}",
        ),
        (
            "unsafe_call",
            "fn main(){let keys=[1];let view=View(&keys);let _=view.get(0);}",
        ),
        (
            "mutation",
            "fn main(){let mut keys=[1];let value=first(View(&keys));keys[0]=2;println!(\"{}\",value);}",
        ),
        (
            "owner_bound",
            "fn require<L:Keys<'static,&'static i32>>(_:L){}fn main(){let value=1;let keys=[&value];require(View(&keys));}",
        ),
    ] {
        let path = scratch.path().join(format!("{name}.rs"));
        std::fs::write(&path, format!("{prefix}{tail}")).unwrap();
        let oracle = support::process::run(
            Command::new("rustc")
                .args(["--edition=2024", "--emit=metadata"])
                .arg(&path)
                .arg("-o")
                .arg(scratch.path().join("probe.rmeta")),
            scratch.path(),
            &format!("rustc-{name}"),
            Duration::from_secs(30),
        )
        .completed();
        let actual = support::process::run(
            Command::new(env!("CARGO_BIN_EXE_fluxc"))
                .arg("mir")
                .arg(&path),
            scratch.path(),
            &format!("flux-{name}"),
            Duration::from_secs(30),
        )
        .completed();
        assert!(!oracle.status.success(), "invalid rustc probe {name}");
        assert!(!actual.status.success(), "invalid Flux probe {name}");
    }
}

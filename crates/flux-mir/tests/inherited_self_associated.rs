//! Resolve inherited associated names without assuming impl validity.
use std::process::Command;

#[test]
fn inherited_self_names_match_rustc() {
    let dir = std::env::temp_dir().join(format!("flux-inherited-self-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, accepted, declaration) in [
        (
            "direct",
            true,
            r#"trait Base {type Item;} trait Child: Base {fn get()->Self::Item;} struct S; impl Base for S {type Item=u32;} impl Child for S {fn get()->Self::Item {7}}"#,
        ),
        (
            "transitive",
            true,
            r#"trait Base<T>{type Item;} trait Mid<T>:Base<(T,u8)>{} trait Child<T>:Mid<T>{fn get()->Self::Item;} struct S; impl Base<(u32,u8)> for S {type Item=u64;} impl Mid<u32> for S{} impl Child<u32> for S{fn get()->Self::Item{7}}"#,
        ),
        (
            "swapped",
            true,
            r#"trait Base<A,B>{type Item;} trait Child<A,B>:Base<B,A>{fn get()->Self::Item;} struct S; impl Base<u16,u8> for S{type Item=u64;} impl Child<u8,u16> for S{fn get()->Self::Item{7}}"#,
        ),
        (
            "defaulted",
            true,
            r#"trait Base<T=u8>{type Item;} trait Child:Base{fn get()->Self::Item;} struct S; impl Base for S{type Item=u32;} impl Child for S{fn get()->Self::Item{7}}"#,
        ),
        (
            "diamond",
            true,
            r#"trait Base{type Item;} trait L:Base{} trait R:Base{} trait Child:L+R{fn get()->Self::Item;} struct S; impl Base for S{type Item=u32;} impl L for S{} impl R for S{} impl Child for S{fn get()->Self::Item{7}}"#,
        ),
        (
            "rhs",
            true,
            r#"trait Base{type Item;} trait Child:Base{type Other;} struct S; impl Base for S{type Item=u8;} impl Child for S{type Other=Self::Item;}"#,
        ),
        (
            "body",
            true,
            r#"trait Base{type Item;} trait Child:Base{fn get()->u8;} struct S; impl Base for S{type Item=u8;} impl Child for S{fn get()->u8{let x:Self::Item=7;x}}"#,
        ),
        (
            "missing_impl",
            false,
            r#"trait Base{type Item;} trait Child:Base{fn get()->Self::Item;} struct S; impl Child for S{fn get()->Self::Item{7}}"#,
        ),
        (
            "ambiguous",
            false,
            r#"trait A{type Item;} trait B{type Item;} trait Child:A+B{fn get();} struct S; impl A for S{type Item=u8;} impl B for S{type Item=u8;} impl Child for S{fn get(){let _:Option<Self::Item>=None;}}"#,
        ),
        (
            "different_args",
            false,
            r#"trait A<T>{type Item;} trait Child:A<u8>+A<u16>{fn get();} struct S; impl A<u8> for S{type Item=u32;} impl A<u16> for S{type Item=u32;} impl Child for S{fn get(){let _:Option<Self::Item>=None;}}"#,
        ),
        (
            "wrong_output",
            false,
            r#"trait Base{type Item;} trait Child:Base{fn get()->Self::Item;} struct S; impl Base for S{type Item=u32;} impl Child for S{fn get()->Self::Item{false}}"#,
        ),
        (
            "missing_name",
            false,
            r#"trait Base{type Item;} trait Child:Base{fn get();} struct S; impl Base for S{type Item=u32;} impl Child for S{fn get(){let _:Option<Self::Absent>=None;}}"#,
        ),
        (
            "qualified",
            false,
            r#"trait Base{type Item;} trait Child:Base{fn get();} struct S; impl Base for S{type Item=u8;} impl Child for S{fn get(){let _:Option<<Self as Child>::Item>=None;}}"#,
        ),
        (
            "wrong_substitution",
            false,
            r#"trait Base<T>{type Item;} trait Child:Base<u16>{fn get()->Self::Item;} struct S; impl Base<u8> for S{type Item=u32;} impl Child for S{fn get()->Self::Item{7}}"#,
        ),
        (
            "escape",
            false,
            r#"trait Base{type Item;} trait Child:Base{fn get(&self)->Self::Item;} struct S(u8); impl Base for S{type Item=&'static u8;} impl Child for S{fn get(&self)->Self::Item{&self.0}}"#,
        ),
        (
            "reference",
            true,
            r#"trait Base{type Item;} trait Child:Base{fn get(&self)->Self::Item;} struct S<'a>(&'a u8); impl<'a> Base for S<'a>{type Item=&'a u8;} impl Child for S<'_>{fn get(&self)->Self::Item{self.0}}"#,
        ),
        (
            "second_slot",
            true,
            r#"trait Base{type Item;} trait Child:Base{fn get(&self)->Self::Item;} struct S<'a,'b>(&'a u8,&'b u8); impl<'a,'b> Base for S<'a,'b>{type Item=&'b u8;} impl Child for S<'_,'_>{fn get(&self)->Self::Item{self.1}}"#,
        ),
        (
            "wrong_slot",
            false,
            r#"trait Base{type Item;} trait Child:Base{fn get(&self)->Self::Item;} struct S<'a,'b>(&'a u8,&'b u8); impl<'a,'b> Base for S<'a,'b>{type Item=&'b u8;} impl Child for S<'_,'_>{fn get(&self)->Self::Item{self.0}}"#,
        ),
        (
            "static_only",
            false,
            r#"trait Base{type Item;} trait Child:Base{fn get(&self)->Self::Item;} struct S<'a>(&'a u8); impl Base for S<'static>{type Item=&'static u8;} impl Child for S<'_>{fn get(&self)->Self::Item{self.0}}"#,
        ),
        (
            "static_escape",
            false,
            r#"trait Base{type Item;} trait Child:Base{fn get(&self)->Self::Item;} struct S<'a>(&'a u8); impl<'a> Base for S<'a>{type Item=&'a u8;} impl Child for S<'_>{fn get(&self)->Self::Item{self.0}} fn bad<'a>(s:S<'a>)-> &'static u8{s.get()}"#,
        ),
        (
            "repeated",
            false,
            r#"trait Base{type Item;} trait Child:Base{fn get(&self)->Self::Item;} struct S<'a,'b>(&'a u8,&'b u8); impl<'a> Base for S<'a,'a>{type Item=&'a u8;} impl Child for S<'_,'_>{fn get(&self)->Self::Item{self.0}}"#,
        ),
    ] {
        let text = format!("{declaration} fn main() {{}}");
        let path = dir.join(format!("{name}.rs"));
        std::fs::write(&path, &text).unwrap();
        let reference = Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata", "--out-dir"])
            .arg(&dir)
            .arg(&path)
            .output()
            .unwrap();
        assert_eq!(
            reference.status.success(),
            accepted,
            "rustc {name}: {}",
            String::from_utf8_lossy(&reference.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text(&path, &text);
        let mut errors: Vec<_> = hir.diagnostics.iter().map(|d| d.message.clone()).collect();
        if errors.is_empty() {
            let typed = flux_mir::check_file(&hir);
            errors.extend(typed.diagnostics.iter().map(|d| d.message.clone()));
            if errors.is_empty() {
                errors.extend(
                    flux_mir::build_file(&hir, &typed)
                        .diagnostics
                        .into_iter()
                        .map(|d| d.message),
                );
            }
        }
        assert_eq!(errors.is_empty(), accepted, "flux {name}: {errors:?}");
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn unrepresented_inherited_region_maps_reject_explicitly() {
    let declarations = [
        (
            "body",
            "trait Base{type Item;} trait Child:Base{fn run(&self);} struct S<'a>(&'a u8); impl<'a> Base for S<'a>{type Item=&'a u8;} impl Child for S<'_>{fn run(&self){let _:Self::Item=self.0;}}",
            "outside function signatures",
        ),
        (
            "rhs",
            "trait Base{type Item;} trait Child:Base{type Other;} struct S<'a>(&'a u8); impl<'a> Base for S<'a>{type Item=&'a u8;} impl Child for S<'_>{type Other=Self::Item;}",
            "outside function signatures",
        ),
        (
            "trait_arg",
            "trait Base<T>{type Item;} trait Child:Base<&'static u8>{fn get()->Self::Item;} struct S; impl Base<&'static u8> for S{type Item=u8;} impl Child for S{fn get()->Self::Item{7}}",
            "lifetimes in inherited trait projections",
        ),
        (
            "expanded_slots",
            "trait Base{type Item;} trait Child:Base{fn get(&self)->Self::Item;} struct S<T>(T); impl<T> Base for S<T>{type Item=T;} impl Child for S<&u8>{fn get(&self)->Self::Item{self.0}}",
            "regions in inherited associated types are not supported",
        ),
    ];
    let dir =
        std::env::temp_dir().join(format!("flux-inherited-boundaries-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, declaration, message) in declarations {
        let text = format!("{declaration} fn main() {{}}");
        let path = dir.join(format!("{name}.rs"));
        std::fs::write(&path, &text).unwrap();
        let reference = Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata", "--out-dir"])
            .arg(&dir)
            .arg(&path)
            .output()
            .unwrap();
        assert!(
            reference.status.success(),
            "rustc {name}: {}",
            String::from_utf8_lossy(&reference.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text(&path, &text);
        assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        assert!(
            typed
                .diagnostics
                .iter()
                .any(|d| d.message.contains(message)),
            "{name}: {:?}",
            typed.diagnostics
        );
    }
    std::fs::remove_dir_all(dir).unwrap();
}

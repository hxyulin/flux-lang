//! Slice callbacks may retain references for the receiver's lifetime.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;

#[test]
fn slice_callbacks_match_rustc() {
    let directory =
        std::env::temp_dir().join(format!("flux-slice-callbacks-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    for (name, body, accepted) in [
        (
            "search_capture",
            "fn f<'a>(s:&'a [u32])->Option<&'a u32>{let mut seen=None;let _=s.binary_search_by(|x|{seen=Some(x);x.cmp(&2)});seen}",
            true,
        ),
        (
            "search_key_borrow",
            "fn f<'a>(s:&'a [u32],x:&&'a u32){let _=s.binary_search_by_key(x,|p|p);}",
            true,
        ),
        (
            "sorted_capture",
            "fn f<'a>(s:&'a [u32])->Option<&'a u32>{let mut seen=None;let _=s.is_sorted_by(|a,b|{seen=Some(a);a<=b});seen}",
            true,
        ),
        (
            "sorted_key_borrow",
            "fn f<'a>(s:&'a [u32])->bool{s.is_sorted_by_key(|x|x)}",
            true,
        ),
        (
            "search_static",
            "fn f(s:&[u32])->&'static u32{let mut seen=&0;let _=s.binary_search_by(|x|{seen=x;x.cmp(&2)});seen}",
            false,
        ),
        (
            "sorted_static",
            "fn f(s:&[u32])->&'static u32{let mut seen=&0;let _=s.is_sorted_by(|a,b|{seen=a;a<=b});seen}",
            false,
        ),
        (
            "expired_search",
            "fn main(){let p;{let a=[1u32,2];let mut seen=None;let _=a.binary_search_by(|x|{seen=Some(x);x.cmp(&2)});p=seen;}println!(\"{:?}\",p);}",
            false,
        ),
        (
            "sorted_mut_conflict",
            "fn f(s:&mut [u32]){let mut p=None;let _=s.is_sorted_by(|a,b|{p=Some(a);a<=b});s[0]=9;println!(\"{:?}\",p);}",
            false,
        ),
    ] {
        if !case_filter::selected(name) {
            continue;
        }
        let source = if body.starts_with("fn main") {
            body.to_owned()
        } else {
            format!("{body}fn main(){{}}")
        };
        let path = directory.join(format!("{name}.rs"));
        std::fs::write(&path, &source).unwrap();
        let reference = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata", "--out-dir"])
            .arg(&directory)
            .arg(&path)
            .output()
            .unwrap();
        assert_eq!(
            reference.status.success(),
            accepted,
            "rustc {name}: {}",
            String::from_utf8_lossy(&reference.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text(&path, &source);
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
    std::fs::remove_dir_all(directory).unwrap();
}

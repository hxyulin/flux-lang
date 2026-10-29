//! Local slice metadata does not read mutable constant storage.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;
use std::process::Command;

#[test]
fn local_metadata_matches_rustc() {
    let dir = std::env::temp_dir().join(format!("flux-local-metadata-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, rustc_accepted, flux_accepted, text) in [
        (
            "indexed_loop",
            true,
            true,
            "const fn table()->[u8;32]{let mut data=[0;32];let mut i=0;while i<data.len(){data[i]=i as u8;i+=1;}data}static X:[u8;32]=table();fn main(){assert_eq!(X[31],31);}",
        ),
        (
            "empty_array",
            true,
            true,
            "const X:usize={let mut a:[u8;0]=[];a=[];a.len()};fn main(){assert_eq!(X,0);}",
        ),
        (
            "cross_frame",
            true,
            true,
            "const fn n(a:&[u16])->usize{a.len()}const X:usize={let mut a=[3u16;7];a[1]=9;n(&a)};fn main(){assert_eq!(X,7);}",
        ),
        (
            "projected_array",
            true,
            true,
            "struct S{a:[u8;3],b:[u16;5]}const X:usize={let mut s=S{a:[1;3],b:[2;5]};s.a[1]=9;s.b=[4;5];s.a.len()+s.b.len()};fn main(){assert_eq!(X,8);}",
        ),
        (
            "raw_metadata_after_mutation",
            true,
            true,
            "const X:usize={let mut a=[1u8;4];let p=(&raw const a) as *const [u8];a=[9;4];p.len()};fn main(){assert_eq!(X,4);}",
        ),
        (
            "invalid_elements_not_read",
            true,
            true,
            "const X:usize={let mut a=[2u8;3];a[0]=3;let p=(&raw const a) as *const [u8] as *const [bool];p.len()};fn main(){assert_eq!(X,3);}",
        ),
        (
            "stale_data_read_stays_unsupported",
            true,
            false,
            "const X:u8={let mut a=[1u8;4];let p=(&raw const a) as *const u8;a=[9;4];unsafe{*p}};fn main(){assert_eq!(X,9);}",
        ),
        (
            "dead_frame_read",
            false,
            false,
            "const fn p()->*const u8{let a=[1u8;4];(&raw const a) as *const u8}const X:u8=unsafe{*p()};fn main(){let _=X;}",
        ),
        (
            "uninitialized_storage",
            false,
            false,
            "const X:usize={let a:[u8;3];a.len()};fn main(){let _=X;}",
        ),
        (
            "dead_frame_slice_read",
            false,
            false,
            "const fn p()->*const [u8;3]{let a=[1u8;3];&raw const a}const X:usize=unsafe{(&*p()).len()+(*p())[0] as usize};fn main(){let _=X;}",
        ),
        (
            "null_reference_metadata",
            false,
            false,
            "const P:&[u8;0]=unsafe{&*(0usize as *const [u8;0])};const X:usize=P.len();fn main(){let _=X;}",
        ),
        (
            "invalid_reference_metadata",
            false,
            false,
            "const P:&[bool;3]=unsafe{&*(&[2u8;3] as *const [u8;3] as *const [bool;3])};const X:usize=P.len();fn main(){let _=X;}",
        ),
        (
            "invalid_scalar_read",
            false,
            false,
            "const X:bool={let mut a=[2u8;3];a[0]=3;unsafe{*((&raw const a) as *const bool)}};fn main(){let _=X;}",
        ),
        (
            "shared_raw_write",
            false,
            false,
            "const X:u8={let a=1u8;let p=&a as *const u8 as *mut u8;unsafe{*p=2;}a};fn main(){let _=X;}",
        ),
        (
            "mutable_static_read",
            false,
            false,
            "static mut A:[u8;3]=[1;3];const X:u8=unsafe{*((&raw const A) as *const u8)};fn main(){let _=X;}",
        ),
    ] {
        if !case_filter::selected(name) {
            continue;
        }
        let path = dir.join(format!("{name}.rs"));
        std::fs::write(&path, text).unwrap();
        let rustc = Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata", "--out-dir"])
            .arg(&dir)
            .arg(&path)
            .output()
            .unwrap();
        assert_eq!(
            rustc.status.success(),
            rustc_accepted,
            "rustc {name}: {}",
            String::from_utf8_lossy(&rustc.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text(&path, text);
        let mut errors = hir.diagnostics.clone();
        if errors.is_empty() {
            let typed = flux_mir::check_file(&hir);
            errors.extend(typed.diagnostics.clone());
            if errors.is_empty() {
                errors.extend(flux_mir::build_file(&hir, &typed).diagnostics);
            }
        }
        assert_eq!(errors.is_empty(), flux_accepted, "flux {name}: {errors:?}");
    }
    std::fs::remove_dir_all(dir).unwrap();
}

//! Array ownership moves preserve the Arc allocation and its region constraints.
use std::process::Command;

#[test]
fn arc_array_coercion_boundaries_match_rustc() {
    let dir = std::env::temp_dir().join(format!("flux-arc-array-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, accepts, text) in [
        (
            "empty",
            true,
            "use std::sync::Arc;fn main(){let a:Arc<[String]>=Arc::new([]);let _=a;}",
        ),
        (
            "array",
            true,
            "use std::sync::Arc;fn main(){let a:Arc<[u8]>=Arc::new([1,2]);let _=a;}",
        ),
        (
            "cast",
            true,
            "use std::sync::Arc;fn main(){let a=Arc::new([1u8,2]);let _=a as Arc<[u8]>;}",
        ),
        (
            "generic",
            true,
            "use std::sync::Arc;fn f<T,const N:usize>(a:Arc<[T;N]>)->Arc<[T]>{a}fn main(){let _=f(Arc::new([1u8,2]));}",
        ),
        (
            "borrowed",
            true,
            "use std::sync::Arc;fn f<'a,const N:usize>(a:Arc<[&'a u32;N]>)->Arc<[&'a u32]>{a}fn main(){let n=1;let _=f(Arc::new([&n]));}",
        ),
        (
            "escape",
            false,
            "use std::sync::Arc;fn f<'a>(a:Arc<[&'a u32;1]>)->Arc<[&'static u32]>{a}fn main(){}",
        ),
        (
            "invariant",
            false,
            "use std::sync::Arc;use std::cell::Cell;fn f<'a,'b:'a>(a:Arc<[Cell<&'b u32>;1]>)->Arc<[Cell<&'a u32>]>{a}fn main(){}",
        ),
        (
            "element",
            false,
            "use std::sync::Arc;fn main(){let _:Arc<[u8]>=Arc::new([1u16,2]);}",
        ),
        (
            "reference",
            false,
            "use std::sync::Arc;fn main(){let a=Arc::new([1u8,2]);let _: &Arc<[u8]>=&a;}",
        ),
        (
            "mut_reference",
            false,
            "use std::sync::Arc;fn main(){let mut a=Arc::new([1u8,2]);let _: &mut Arc<[u8]>=&mut a;}",
        ),
        (
            "moved",
            false,
            "use std::sync::Arc;fn main(){let a=Arc::new([1u8,2]);let b:Arc<[u8]>=a;drop(b);let _=a.clone();}",
        ),
        (
            "fake",
            false,
            "mod fake{pub struct Arc<T:?Sized>{pub data:*const T}}fn main(){let a=fake::Arc{data:&[1u8,2] as *const [u8;2]};let _:fake::Arc<[u8]>=a;}",
        ),
    ] {
        let file = dir.join(format!("{name}.rs"));
        std::fs::write(&file, text).unwrap();
        let rustc = Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&file)
            .arg("-o")
            .arg(dir.join(format!("{name}.rmeta")))
            .output()
            .unwrap();
        assert_eq!(
            rustc.status.success(),
            accepts,
            "rustc {name}: {}",
            String::from_utf8_lossy(&rustc.stderr)
        );
        let flux = Command::new(env!("CARGO_BIN_EXE_flux"))
            .arg("mir")
            .arg(&file)
            .output()
            .unwrap();
        assert_eq!(
            flux.status.success(),
            accepts,
            "flux {name}: {}",
            String::from_utf8_lossy(&flux.stderr)
        );
        assert!(
            !String::from_utf8_lossy(&flux.stderr).contains("internal compiler error"),
            "{name}"
        );
        assert!(
            !String::from_utf8_lossy(&flux.stderr).contains("panicked at"),
            "{name}"
        );
    }
    std::fs::remove_dir_all(dir).unwrap();
}
